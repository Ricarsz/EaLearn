//! `forge` — 内容生产与校验 CLI。
//!
//! 现在的职责只有三件事：校验 pack、打印分词标注、算内容指纹。
//! LLM 生成（`forge gen`）与打包（`forge pack`）在后面里程碑接入。

use std::process::ExitCode;

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
  forge audio <pack-dir> [--voice espeak:en-us] [--force]
                                   预合成词条与句子的音频，写回 pack 并更新指纹
  forge voices <lang>              列出该语言可用的音色（espeak + 本机 piper 模型）
  forge say <voice> <文字> <输出.wav>
                                   用指定音色合成一句，排查音色问题时用"
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
        .filter_map(|e| e.audio.as_ref().and_then(|a| a.rsplit('/').next()).map(str::to_string))
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
    if failed > 0 { ExitCode::from(1) } else { ExitCode::SUCCESS }
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
        eprintln!("{lang} 没有可用音色：确认已装 espeak-ng，或把 piper 模型放进模型目录");
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
