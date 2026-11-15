//! `forge` — 内容生产与校验 CLI。
//!
//! 现在的职责只有三件事：校验 pack、打印分词标注、算内容指纹。
//! LLM 生成（`forge gen`）与打包（`forge pack`）在后面里程碑接入。

use std::process::ExitCode;

use ealearn_core::entry::Entry;
use ealearn_core::pack::Pack;
use ealearn_core::validate::validate_pack;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(cmd) = args.first().map(String::as_str) else {
        usage();
        return ExitCode::from(2);
    };

    match (cmd, args.get(1).map(String::as_str)) {
        ("validate", Some(dir)) => validate(dir),
        ("hash", Some(dir)) => hash(dir),
        ("annotate", Some(lang)) => annotate(lang, args.get(2).map(String::as_str)),
        ("tokens", Some(lang)) => tokens(lang, args.get(2).map(String::as_str)),
        ("audio", Some(dir)) => audio(dir, &args[2..]),
        ("voices", Some(lang)) => voices(lang),
        ("say", Some(spec)) => say(spec, &args[2..]),
        ("gen", Some("dialog")) => gen_dialog(&args[2..]),
        ("corpus", Some(kind)) => corpus(kind, &args[2..]),
        ("check-import", Some(path)) => check_import(path),
        ("--help" | "-h" | "help", _) => {
            usage();
            ExitCode::SUCCESS
        }
        _ => {
            usage();
            ExitCode::from(2)
        }
    }
}

fn usage() {
    eprintln!(
        "用法:
  forge validate <pack-dir>        校验 pack（标注对齐、题目、对话图、素材、指纹）
  forge hash <pack-dir>            打印内容指纹，用于写进 manifest.json 的 content_hash
  forge annotate <lang> [text]     打印分词与标注；不给 text 则从 stdin 读
  forge tokens <lang> [text]       只输出 token 的 JSON 数组，供生成 pack 时内联
  forge audio <pack-dir> [--voice piper:en_US-lessac] [--force]
                                   预合成词条与句子的音频，写回 pack 并更新指纹
  forge voices <lang>              列出该语言可用的音色（本机 piper 模型）
  forge say <voice> <文字> <输出.wav>
                                   用指定音色合成一句，排查音色问题时用
  forge check-import <file>        试解析一个词书文件，报告能识别多少词
  forge corpus words <pack-dir> --from=<ecdict.csv> [--tag=cet4] [--limit=2000]
                                   从 ECDICT 批量导入词条，按词频排序
  forge corpus sentences <pack-dir> --from=<pairs.tsv> [--limit=200] [--max-words=14]
                                   从 Tatoeba 句对批量导入句子（英文<TAB>中文）
  forge gen dialog <lang> <theme> [--count N] [--level L] [--nodes N]
                   [--out DIR] [--dry-run]
                                   让 LLM 生成剧本对话草稿，标注后写入 DIR
                                   环境变量：EALEARN_LLM_KEY / EALEARN_LLM_BASE / EALEARN_LLM_MODEL"
    );
}

fn validate(dir: &str) -> ExitCode {
    let pack = match Pack::load(dir) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("加载失败: {e}");
            return ExitCode::from(1);
        }
    };
    let report = validate_pack(&pack);
    for issue in &report.issues {
        let level = match issue.severity {
            ealearn_core::validate::Severity::Error => "错误",
            ealearn_core::validate::Severity::Warn => "提示",
        };
        eprintln!("[{level}] {} — {}", issue.at, issue.message);
    }
    let errors = report.errors().count();
    let warns = report.warnings().count();
    println!(
        "{} · {} 条词条 / {} 句 / {} 篇 / {} 段对话 · {errors} 错误 {warns} 提示",
        pack.manifest.display,
        pack.words.len(),
        pack.sentences.len(),
        pack.articles.len(),
        pack.dialogs.len()
    );
    if report.is_ok() {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
}

fn hash(dir: &str) -> ExitCode {
    match Pack::load(dir) {
        Ok(pack) => match pack.content_hash() {
            Ok(h) => {
                println!("{h}");
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("计算失败: {e}");
                ExitCode::from(1)
            }
        },
        Err(e) => {
            eprintln!("加载失败: {e}");
            ExitCode::from(1)
        }
    }
}

fn annotate(lang: &str, text: Option<&str>) -> ExitCode {
    let Some(profile) = ealearn_lang::profile(lang) else {
        eprintln!(
            "未知语言 {lang}，已实现: {}",
            ealearn_lang::available().join(", ")
        );
        return ExitCode::from(1);
    };
    let text = match text {
        Some(t) => t.to_string(),
        None => {
            let mut buf = String::new();
            if std::io::Read::read_to_string(&mut std::io::stdin(), &mut buf).is_err() {
                eprintln!("读取 stdin 失败");
                return ExitCode::from(1);
            }
            buf
        }
    };
    let text = text.trim_end_matches('\n');
    let tokens = profile.tokenize(text);
    for t in &tokens {
        println!(
            "{:<8} span={:?} pos={:<6} lemma={:<10} reading={}",
            t.surface,
            t.span,
            t.pos.as_deref().unwrap_or("-"),
            t.lemma.as_deref().unwrap_or("-"),
            t.reading.as_deref().unwrap_or("-"),
        );
    }
    println!("— {} 个 token", tokens.len());

    let report = ealearn_core::validate::check_coverage("stdin", text, &tokens);
    for issue in report.errors() {
        eprintln!("[错误] {}", issue.message);
    }
    if report.is_ok() {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
}

fn read_text(text: Option<&str>) -> Option<String> {
    match text {
        Some(t) => Some(t.to_string()),
        None => {
            let mut buf = String::new();
            std::io::Read::read_to_string(&mut std::io::stdin(), &mut buf).ok()?;
            Some(buf)
        }
    }
}

/// 只输出 token 的 JSON 数组。生成 pack 时用它把标注内联进词条，
/// 保证 span 与 surface 由分词器产生，而不是手写。
fn tokens(lang: &str, text: Option<&str>) -> ExitCode {
    let Some(profile) = ealearn_lang::profile(lang) else {
        eprintln!(
            "未知语言 {lang}，已实现: {}",
            ealearn_lang::available().join(", ")
        );
        return ExitCode::from(1);
    };
    let Some(raw) = read_text(text) else {
        eprintln!("读取输入失败");
        return ExitCode::from(1);
    };
    let text = raw.trim_end_matches('\n');
    let tokens = profile.tokenize(text);
    let report = ealearn_core::validate::check_coverage("input", text, &tokens);
    if !report.is_ok() {
        for issue in report.errors() {
            eprintln!("[错误] {}", issue.message);
        }
        return ExitCode::from(1);
    }
    match serde_json::to_string(&tokens) {
        Ok(json) => {
            println!("{json}");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("序列化失败: {e}");
            ExitCode::from(1)
        }
    }
}

/// 预合成音频。文件名按「音色 + 文本」的 hash 决定，所以换音色不会误用旧文件，
/// 同音色重跑则全部命中缓存。
fn audio(dir: &str, rest: &[String]) -> ExitCode {
    use ealearn_core::pack::Pack;

    let mut voice: Option<String> = None;
    let mut force = false;
    for arg in rest {
        if let Some(v) = arg.strip_prefix("--voice=") {
            voice = Some(v.to_string());
        } else if arg == "--force" {
            force = true;
        }
    }

    let mut pack = match Pack::load(dir) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("加载失败: {e}");
            return ExitCode::from(1);
        }
    };

    // 音色优先级：命令行指定 → manifest 里已有的 piper 音色 → 本机第一个可用模型
    let spec = match voice {
        Some(v) => v,
        None => {
            let current = pack.manifest.voice.clone();
            if current.starts_with("piper:") && ealearn_tts::engine_for(&current).is_ok() {
                current
            } else {
                match ealearn_tts::available(&pack.manifest.lang)
                    .into_iter()
                    .find(|v| v.ready)
                {
                    Some(v) => v.id,
                    None => {
                        eprintln!(
                            "没有可用的 piper 模型。先下载一个放进 {}，                             例如 en/en_US/lessac/high/en_US-lessac-high.onnx（含 .onnx.json）",
                            ealearn_tts::model_dir().display()
                        );
                        return ExitCode::from(1);
                    }
                }
            }
        }
    };

    let engine = match ealearn_tts::engine_for(&spec) {
        Ok(e) => e,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::from(1);
        }
    };

    let audio_dir = pack.root.join("media/audio");
    let mut made = 0usize;
    let mut reused = 0usize;
    let mut failed = 0usize;

    for entry in pack.words.iter_mut().chain(pack.sentences.iter_mut()) {
        let text = entry.surface.trim();
        if text.is_empty() || !text.chars().any(|c| c.is_alphabetic()) {
            continue;
        }
        if !force {
            if let Some(rel) = &entry.audio {
                if pack.root.join(rel).exists() {
                    reused += 1;
                    continue;
                }
            }
        }
        match ealearn_tts::synth_to_dir(engine.as_ref(), &spec, text, &audio_dir) {
            Ok(name) => {
                entry.audio = Some(format!("media/audio/{name}"));
                made += 1;
            }
            Err(e) => {
                eprintln!("合成失败「{text}」: {e}");
                failed += 1;
            }
        }
    }

    // 写回内容文件
    let words = pack.words.clone();
    let sentences = pack.sentences.clone();
    if let Err(e) = write_items(&pack.root.join("words.json"), &words) {
        eprintln!("写回 words.json 失败: {e}");
        return ExitCode::from(1);
    }
    if let Err(e) = write_items(&pack.root.join("sentences.json"), &sentences) {
        eprintln!("写回 sentences.json 失败: {e}");
        return ExitCode::from(1);
    }

    // 清掉没有词条引用的旧音频（换音色后 hash 会变，旧文件就成了垃圾）
    let referenced: std::collections::HashSet<String> = pack
        .words
        .iter()
        .chain(pack.sentences.iter())
        .filter_map(|e| {
            e.audio
                .as_ref()
                .and_then(|a| a.rsplit('/').next())
                .map(str::to_string)
        })
        .collect();
    let mut pruned = 0usize;
    if let Ok(entries) = std::fs::read_dir(&audio_dir) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if !referenced.contains(&name) && std::fs::remove_file(entry.path()).is_ok() {
                pruned += 1;
            }
        }
    }

    // 音色变了要记在新 manifest 里，否则下次不知道这批音频是谁合成的
    pack.manifest.voice = spec.clone();
    let hash = match pack.content_hash() {
        Ok(h) => h,
        Err(e) => {
            eprintln!("计算指纹失败: {e}");
            return ExitCode::from(1);
        }
    };
    pack.manifest.content_hash = Some(hash.clone());
    if let Err(e) = write_manifest(&pack.root.join("manifest.json"), &pack.manifest) {
        eprintln!("写回 manifest.json 失败: {e}");
        return ExitCode::from(1);
    }

    println!(
        "{} · 音色 {spec} · 新生成 {made} · 复用 {reused} · 清理 {pruned} · 失败 {failed} · hash {}…",
        pack.manifest.display,
        &hash[..12]
    );
    if failed > 0 {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}

fn write_items<T: serde::Serialize>(path: &std::path::Path, items: &[T]) -> std::io::Result<()> {
    let json = serde_json::to_string_pretty(items).map_err(std::io::Error::other)?;
    std::fs::write(path, json + "\n")
}

fn write_manifest(
    path: &std::path::Path,
    manifest: &ealearn_core::Manifest,
) -> std::io::Result<()> {
    let json = serde_json::to_string_pretty(manifest).map_err(std::io::Error::other)?;
    std::fs::write(path, json + "\n")
}

fn voices(lang: &str) -> ExitCode {
    let list = ealearn_tts::available(lang);
    if list.is_empty() {
        eprintln!("{lang} 没有可用音色：把 piper 模型（onnx + onnx.json）放进模型目录");
        return ExitCode::from(1);
    }
    for v in &list {
        println!(
            "{:<28} {:<7} {}",
            v.id,
            v.engine,
            if v.ready { "" } else { "（缺 .onnx.json）" }
        );
    }
    println!("— {} 个音色", list.len());
    ExitCode::SUCCESS
}

fn say(spec: &str, rest: &[String]) -> ExitCode {
    let Some(text) = rest.first() else {
        eprintln!("用法: forge say <voice> <文字> <输出.wav>");
        return ExitCode::from(2);
    };
    let out = rest.get(1).map(String::as_str).unwrap_or("out.wav");
    let engine = match ealearn_tts::engine_for(spec) {
        Ok(e) => e,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::from(1);
        }
    };
    match engine.synth_wav(text) {
        Ok(bytes) => match std::fs::write(out, &bytes) {
            Ok(()) => {
                println!("{out} · {} 字节 · {spec}", bytes.len());
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("写文件失败: {e}");
                ExitCode::from(1)
            }
        },
        Err(e) => {
            eprintln!("合成失败: {e}");
            ExitCode::from(1)
        }
    }
}

// ── 批量语料导入 ──────────────────────────────────────────────────────
//
// 词条和例句不该一条条写：ECDICT 给词条与释义（带考试标签、词频），
// Tatoeba 给句对。这里只负责筛选、去重、标注，剩下的交给 `forge validate`。

fn corpus(kind: &str, rest: &[String]) -> ExitCode {
    let Some(dir) = rest.first().map(String::as_str) else {
        eprintln!(
            "用法: forge corpus words <pack-dir> --from=<ecdict.csv> [--tag=cet4] [--limit=2000]"
        );
        eprintln!("      forge corpus sentences <pack-dir> --from=<pairs.tsv> [--limit=200] [--max-words=14]");
        return ExitCode::from(2);
    };
    let Some(from) = arg_of(rest, "from") else {
        eprintln!("缺少 --from=<文件>");
        return ExitCode::from(2);
    };
    let limit = arg_of(rest, "limit")
        .and_then(|v| v.parse().ok())
        .unwrap_or(1000usize);

    match kind {
        "words" => corpus_words(dir, &from, arg_of(rest, "tag").as_deref(), limit),
        "sentences" => {
            let max_words = arg_of(rest, "max-words")
                .and_then(|v| v.parse().ok())
                .unwrap_or(14usize);
            corpus_sentences(dir, &from, limit, max_words)
        }
        other => {
            eprintln!("未知的语料类型 {other}，只支持 words / sentences");
            ExitCode::from(2)
        }
    }
}

fn arg_of(args: &[String], key: &str) -> Option<String> {
    let prefix = format!("--{key}=");
    args.iter()
        .find_map(|a| a.strip_prefix(&prefix).map(str::to_string))
}

/// 从 ECDICT 的 CSV 里挑词。`--tag=cet4` 只取四级词，按词频从高到低排。
fn corpus_words(dir: &str, from: &str, tag: Option<&str>, limit: usize) -> ExitCode {
    let mut pack = match Pack::load(dir) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("加载失败: {e}");
            return ExitCode::from(1);
        }
    };
    let lang = pack.manifest.lang.clone();
    let Some(profile) = ealearn_lang::profile(&lang) else {
        eprintln!("未实现的语言: {lang}");
        return ExitCode::from(1);
    };
    let file = match std::fs::File::open(from) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("打开 {} 失败: {e}", from);
            return ExitCode::from(1);
        }
    };

    let mut rdr = csv::ReaderBuilder::new()
        .flexible(true)
        .from_reader(std::io::BufReader::new(file));
    let headers = match rdr.headers() {
        Ok(h) => h.clone(),
        Err(e) => {
            eprintln!("读取表头失败: {e}");
            return ExitCode::from(1);
        }
    };
    let idx = |name: &str| {
        headers
            .iter()
            .position(|h| h.trim_start_matches('\u{feff}') == name)
    };
    let (Some(i_word), Some(i_trans), Some(i_tag), Some(i_frq), Some(i_bnc)) = (
        idx("word"),
        idx("translation"),
        idx("tag"),
        idx("frq"),
        idx("bnc"),
    ) else {
        eprintln!("这不是 ECDICT 的 CSV：缺 word / translation / tag / frq / bnc 列");
        return ExitCode::from(1);
    };

    let mut seen: std::collections::HashSet<String> = pack
        .words
        .iter()
        .map(|e| e.surface.to_lowercase())
        .collect();
    let mut picked: Vec<(u32, String, String, Vec<String>)> = Vec::new();

    for rec in rdr.records().flatten() {
        let word = rec.get(i_word).unwrap_or("").trim();
        if word.is_empty()
            || word.split_whitespace().count() != 1
            || !word.chars().any(|c| c.is_alphabetic())
        {
            continue;
        }
        let tags_field = rec.get(i_tag).unwrap_or("").trim();
        if let Some(want) = tag {
            if !tags_field.split_whitespace().any(|t| t == want) {
                continue;
            }
        }
        let Some(gloss) = tidy_gloss(rec.get(i_trans).unwrap_or("")) else {
            continue;
        };
        if !seen.insert(word.to_lowercase()) {
            continue;
        }
        let rank = rec
            .get(i_frq)
            .and_then(|v| v.parse::<u32>().ok())
            .filter(|v| *v > 0)
            .or_else(|| {
                rec.get(i_bnc)
                    .and_then(|v| v.parse::<u32>().ok())
                    .filter(|v| *v > 0)
            })
            .unwrap_or(u32::MAX);
        picked.push((
            rank,
            word.to_string(),
            gloss,
            tags_field.split_whitespace().map(str::to_string).collect(),
        ));
    }

    picked.sort_by_key(|(rank, _, _, _)| *rank);
    picked.truncate(limit);

    let mut seq = next_seq(&pack.words, &lang, "w");
    for (_, word, gloss, tags) in &picked {
        let mut entry = Entry::new(format!("{lang}.w.{seq:04}"), word);
        seq += 1;
        entry.gloss = Some(gloss.clone());
        entry.tags = tags.clone();
        entry.tokens = profile.tokenize(word);
        pack.words.push(entry);
    }

    if picked.is_empty() {
        eprintln!("没有可导入的词（检查 --tag 与词表是否匹配）");
        return ExitCode::from(1);
    }
    if let Err(e) = write_items(&pack.root.join("words.json"), &pack.words) {
        eprintln!("写回 words.json 失败: {e}");
        return ExitCode::from(1);
    }
    if let Err(e) = refresh_hash(&mut pack) {
        eprintln!("{e}");
        return ExitCode::from(1);
    }
    println!(
        "{} · 导入 {} 词 · 现在 {} 词 · 音频用 forge audio 补",
        pack.manifest.display,
        picked.len(),
        pack.words.len()
    );
    ExitCode::SUCCESS
}

/// 从「英文<TAB>中文」句对里挑句子。太短拼不出、太长看着累，按词数卡住。
fn corpus_sentences(dir: &str, from: &str, limit: usize, max_words: usize) -> ExitCode {
    let mut pack = match Pack::load(dir) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("加载失败: {e}");
            return ExitCode::from(1);
        }
    };
    let lang = pack.manifest.lang.clone();
    let Some(profile) = ealearn_lang::profile(&lang) else {
        eprintln!("未实现的语言: {lang}");
        return ExitCode::from(1);
    };
    let text = match std::fs::read_to_string(from) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("读取 {} 失败: {e}", from);
            return ExitCode::from(1);
        }
    };

    let mut seen: std::collections::HashSet<String> = pack
        .sentences
        .iter()
        .map(|e| e.surface.to_lowercase())
        .collect();
    let mut picked: Vec<(String, String)> = Vec::new();

    for line in text.lines() {
        let mut cols = line.split('\t');
        let (Some(raw), Some(gloss)) = (cols.next(), cols.next()) else {
            continue;
        };
        let sentence = raw.trim();
        let gloss = gloss.trim();
        if gloss.is_empty() || sentence.is_empty() {
            continue;
        }
        // 只要这门语言自己的文字：英语句子里不该出现汉字
        if lang == "en"
            && sentence
                .chars()
                .any(|c| ('\u{4e00}'..='\u{9fff}').contains(&c))
        {
            continue;
        }
        let words = sentence.split_whitespace().count();
        if words < 3 || words > max_words {
            continue;
        }
        if !seen.insert(sentence.to_lowercase()) {
            continue;
        }
        picked.push((sentence.to_string(), gloss.to_string()));
        if picked.len() >= limit {
            break;
        }
    }

    if picked.is_empty() {
        eprintln!("没有合格句子（检查 --max-words 与文件格式）");
        return ExitCode::from(1);
    }

    let mut seq = next_seq(&pack.sentences, &lang, "s");
    for (sentence, gloss) in &picked {
        let mut entry = Entry::new(format!("{lang}.s.{seq:04}"), sentence);
        seq += 1;
        entry.gloss = Some(gloss.clone());
        entry.tokens = profile.tokenize(sentence);
        pack.sentences.push(entry);
    }

    if let Err(e) = write_items(&pack.root.join("sentences.json"), &pack.sentences) {
        eprintln!("写回 sentences.json 失败: {e}");
        return ExitCode::from(1);
    }
    if let Err(e) = refresh_hash(&mut pack) {
        eprintln!("{e}");
        return ExitCode::from(1);
    }
    println!(
        "{} · 导入 {} 句 · 现在 {} 句 · 音频用 forge audio 补",
        pack.manifest.display,
        picked.len(),
        pack.sentences.len()
    );
    ExitCode::SUCCESS
}

/// 释义压成一行：多行用「；」连，超长截断。
fn tidy_gloss(raw: &str) -> Option<String> {
    // ECDICT 把多行释义写成字面量 `\n`，先还原成换行再拼
    let text = raw.replace("\\n", "\n");
    let joined = text
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join("；");
    let mut s = joined.split_whitespace().collect::<Vec<_>>().join(" ");
    if s.is_empty() {
        return None;
    }
    if s.chars().count() > 80 {
        s = s.chars().take(80).collect::<String>() + "…";
    }
    Some(s)
}

fn next_seq(entries: &[Entry], lang: &str, kind: &str) -> usize {
    let prefix = format!("{lang}.{kind}.");
    entries
        .iter()
        .filter_map(|e| e.id.strip_prefix(&prefix)?.parse::<usize>().ok())
        .max()
        .unwrap_or(0)
        + 1
}

fn refresh_hash(pack: &mut Pack) -> Result<String, String> {
    let hash = pack
        .content_hash()
        .map_err(|e| format!("计算指纹失败: {e}"))?;
    pack.manifest.content_hash = Some(hash.clone());
    write_manifest(&pack.root.join("manifest.json"), &pack.manifest)
        .map_err(|e| format!("写回 manifest.json 失败: {e}"))?;
    Ok(hash)
}

// ── LLM 生成剧本对话 ──────────────────────────────────────────────────

fn gen_dialog(rest: &[String]) -> ExitCode {
    let Some(lang) = rest.first().cloned() else {
        eprintln!("用法: forge gen dialog <lang> <theme> [--count N] [--level L] [--nodes N] [--out DIR] [--dry-run]");
        return ExitCode::from(2);
    };
    let Some(theme) = rest.get(1).cloned() else {
        eprintln!("缺少话题。例如: forge gen dialog en \"被人放鸽子\"");
        return ExitCode::from(2);
    };

    let mut count = 1usize;
    let mut level = 2u8;
    let mut nodes = 6usize;
    let mut out_dir = "drafts".to_string();
    let mut dry_run = false;
    for arg in &rest[2..] {
        if let Some(v) = arg.strip_prefix("--count=") {
            count = v.parse().unwrap_or(1).clamp(1, 10);
        } else if let Some(v) = arg.strip_prefix("--level=") {
            level = v.parse().unwrap_or(2).clamp(1, 5);
        } else if let Some(v) = arg.strip_prefix("--nodes=") {
            nodes = v.parse().unwrap_or(6).clamp(4, 20);
        } else if let Some(v) = arg.strip_prefix("--out=") {
            out_dir = v.to_string();
        } else if arg == "--dry-run" {
            dry_run = true;
        }
    }

    let Some(profile) = ealearn_lang::profile(&lang) else {
        eprintln!("未实现的语言: {lang}");
        return ExitCode::from(1);
    };
    let label = match lang.as_str() {
        "ko" => "韩语",
        _ => "英语",
    };

    let mut results = Vec::new();
    for i in 0..count {
        let prompt = ealearn_llm::dialog_prompt(&ealearn_llm::dialog::DialogPromptCtx {
            lang: &lang,
            target_label: label,
            theme: &theme,
            level,
            nodes,
            target_words: &[],
        });
        if dry_run {
            println!("{prompt}");
            return ExitCode::SUCCESS;
        }

        let key = std::env::var("EALEARN_LLM_KEY")
            .or_else(|_| std::env::var("OPENAI_API_KEY"))
            .unwrap_or_default();
        if key.is_empty() {
            eprintln!("没有 API key：设置 EALEARN_LLM_KEY 后再跑（也可以加 --dry-run 只看提示词）");
            return ExitCode::from(1);
        }
        let base = std::env::var("EALEARN_LLM_BASE")
            .unwrap_or_else(|_| "https://api.deepseek.com/v1".into());
        let model = std::env::var("EALEARN_LLM_MODEL").unwrap_or_else(|_| "deepseek-chat".into());

        let messages = vec![
            ealearn_llm::prompt::ChatMessage::system(prompt),
            ealearn_llm::prompt::ChatMessage::user(format!(
                "生成第 {} 段对话。话题固定为「{theme}」，请换一个具体场景角度。",
                i + 1
            )),
        ];

        let cfg = ealearn_llm::LlmConfig {
            base_url: base,
            api_key: key,
            model,
            temperature: 0.9,
            timeout_secs: 120,
        };
        let runtime = match tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
        {
            Ok(rt) => rt,
            Err(e) => {
                eprintln!("创建运行时失败: {e}");
                return ExitCode::from(1);
            }
        };
        let client = match ealearn_llm::LlmClient::new(cfg) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("{e}");
                return ExitCode::from(1);
            }
        };

        let raw = match runtime.block_on(client.complete(&messages)) {
            Ok(t) => t,
            Err(e) => {
                eprintln!("第 {} 段生成失败: {e}", i + 1);
                continue;
            }
        };

        let id = format!("{lang}.d.gen{:04}", i + 1);
        match ealearn_llm::parse_dialogs(&raw, &id) {
            Ok(mut dialog) => {
                // 补标注：token 由语言 profile 产出，和 pack 里其它内容同一套规则
                for node in &mut dialog.nodes {
                    if node.text.trim().is_empty() {
                        continue;
                    }
                    node.tokens = profile.tokenize(&node.text);
                    let report = ealearn_core::validate::check_coverage(
                        &format!("{id}/{}", node.id),
                        &node.text,
                        &node.tokens,
                    );
                    for issue in report.errors() {
                        eprintln!("[标注] {} — {}", issue.at, issue.message);
                    }
                }
                if dialog.theme.trim().is_empty() {
                    dialog.theme = theme.clone();
                }
                results.push(dialog);
            }
            Err(e) => eprintln!("第 {} 段解析失败: {e}", i + 1),
        }
        eprintln!("第 {}/{} 段完成", i + 1, count);
    }

    if results.is_empty() {
        eprintln!("没有生成任何可用对话");
        return ExitCode::from(1);
    }
    if std::fs::create_dir_all(&out_dir).is_err() {
        eprintln!("建目录失败: {out_dir}");
        return ExitCode::from(1);
    }
    for dialog in &results {
        let path = std::path::Path::new(&out_dir).join(format!("{}.json", dialog.id));
        match serde_json::to_string_pretty(dialog) {
            Ok(json) => {
                if let Err(e) = std::fs::write(&path, json + "\n") {
                    eprintln!("写入 {} 失败: {e}", path.display());
                } else {
                    println!("{} · {} 个节点", path.display(), dialog.nodes.len());
                }
            }
            Err(e) => eprintln!("序列化失败: {e}"),
        }
    }
    println!(
        "生成 {} 段 · 音色无关 · 草稿在 {out_dir}/，人工校对后再并入 dialogs.json",
        results.len()
    );
    ExitCode::SUCCESS
}

/// 试解析一个词书文件（TSV/CSV/JSON），不写库。用来在导入前确认格式。
fn check_import(path: &str) -> ExitCode {
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("读取失败: {e}");
            return ExitCode::from(1);
        }
    };
    let report = ealearn_core::import::parse(&text);
    for w in report.words.iter().take(5) {
        println!(
            "{:<24} {}",
            w.surface,
            w.gloss.as_deref().unwrap_or("（无释义）")
        );
    }
    if report.words.len() > 5 {
        println!("…");
    }
    println!(
        "识别 {} 词 · 跳过 {} 行",
        report.words.len(),
        report.skipped.len()
    );
    for s in report.skipped.iter().take(5) {
        eprintln!("[跳过] {s}");
    }
    if report.words.is_empty() {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}
