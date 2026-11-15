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
  forge tokens <lang> [text]       只输出 token 的 JSON 数组，供生成 pack 时内联"
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
