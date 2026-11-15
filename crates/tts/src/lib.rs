//! TTS 合成层。
//!
//! 引擎可换：manifest 的 `voice` 字段写成 `espeak:en-us` 或 `piper:en_US-amy`，
//! 合成时按前缀选实现。音色变了 hash 就变了，旧音频不会被误用。
//!
//! 音频一律按「引擎标识 + 文本」的 sha256 命名并落在 pack 的 `media/audio/` 下，
//! 练习时零网络、零等待。

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use sha2::{Digest, Sha256};

#[derive(Debug)]
pub enum TtsError {
    UnknownVoice(String),
    Unsupported(String),
    Spawn(String),
    Empty(String),
}

impl std::fmt::Display for TtsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TtsError::UnknownVoice(v) => {
                write!(
                    f,
                    "无法识别的音色「{v}」，应形如 espeak:en-us 或 piper:en_US-amy"
                )
            }
            TtsError::Unsupported(e) => write!(f, "该音色不支持此引擎: {e}"),
            TtsError::Spawn(e) => write!(f, "启动合成引擎失败: {e}"),
            TtsError::Empty(e) => write!(f, "合成没有返回音频: {e}"),
        }
    }
}

impl std::error::Error for TtsError {}

/// `espeak:en-us` → ("espeak", "en-us")
pub fn parse_voice(spec: &str) -> Result<(&str, &str), TtsError> {
    let (engine, name) = spec
        .split_once(':')
        .ok_or_else(|| TtsError::UnknownVoice(spec.to_string()))?;
    if engine.is_empty() || name.is_empty() {
        return Err(TtsError::UnknownVoice(spec.to_string()));
    }
    Ok((engine, name))
}

/// 音频文件名：同一引擎同一文本永远映射到同一个文件。
pub fn cache_name(spec: &str, text: &str) -> String {
    let mut h = Sha256::new();
    h.update(spec.as_bytes());
    h.update([0]);
    h.update(text.as_bytes());
    format!("{:x}.wav", h.finalize())
}

pub trait Tts {
    /// 合成一段文本，返回完整的 wav 字节。
    fn synth_wav(&self, text: &str) -> Result<Vec<u8>, TtsError>;
}

pub struct Espeak {
    pub voice: String,
    /// 语速，词/分钟。默认 165，比 espeak 的 175 稍慢，初学者听得清。
    pub speed: u32,
}

impl Espeak {
    pub fn new(voice: impl Into<String>) -> Self {
        Self {
            voice: voice.into(),
            speed: 165,
        }
    }
}

impl Tts for Espeak {
    fn synth_wav(&self, text: &str) -> Result<Vec<u8>, TtsError> {
        let out = Command::new("espeak-ng")
            .args([
                "-v",
                &self.voice,
                "-s",
                &self.speed.to_string(),
                "--stdout",
                text,
            ])
            .stdin(Stdio::null())
            .output()
            .map_err(|e| TtsError::Spawn(e.to_string()))?;
        if !out.status.success() {
            return Err(TtsError::Empty(
                String::from_utf8_lossy(&out.stderr).trim().to_string(),
            ));
        }
        if out.stdout.is_empty() {
            return Err(TtsError::Empty("espeak-ng 输出为空".into()));
        }
        Ok(out.stdout)
    }
}

pub struct Piper {
    /// onnx 模型路径
    pub model: PathBuf,
}

impl Tts for Piper {
    /// piper 1.8 的 stdout 输出不可靠（实测输出空文件），一律写临时文件再读回。
    fn synth_wav(&self, text: &str) -> Result<Vec<u8>, TtsError> {
        use std::io::Write;

        if !self.model.exists() {
            return Err(TtsError::Unsupported(format!(
                "没有找到模型 {}",
                self.model.display()
            )));
        }

        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let out = std::env::temp_dir().join(format!("ealearn-tts-{}-{stamp}.wav", std::process::id()));

        let mut child = Command::new(piper_bin())
            .args(["-m", &self.model.to_string_lossy(), "-f", &out.to_string_lossy()])
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| TtsError::Spawn(e.to_string()))?;
        child
            .stdin
            .as_mut()
            .ok_or_else(|| TtsError::Spawn("拿不到 piper 的 stdin".into()))?
            .write_all(text.as_bytes())
            .map_err(|e| TtsError::Spawn(e.to_string()))?;
        let status = child
            .wait_with_output()
            .map_err(|e| TtsError::Spawn(e.to_string()))?;

        let bytes = std::fs::read(&out).unwrap_or_default();
        let _ = std::fs::remove_file(&out);

        if !status.status.success() || bytes.is_empty() {
            return Err(TtsError::Empty(
                String::from_utf8_lossy(&status.stderr).trim().to_string(),
            ));
        }
        Ok(bytes)
    }
}

/// piper 的安装位置：先看 EALEARN_PIPER_BIN，再找 ~/.local/bin，最后落到 PATH。
fn piper_bin() -> PathBuf {
    if let Ok(p) = std::env::var("EALEARN_PIPER_BIN") {
        return PathBuf::from(p);
    }
    if let Ok(home) = std::env::var("HOME") {
        let p = PathBuf::from(home).join(".local/bin/piper");
        if p.exists() {
            return p;
        }
    }
    PathBuf::from("piper")
}

/// 模型目录：EALEARN_PIPER_DIR → $XDG_DATA_HOME/ealearn/piper → ~/.local/share/ealearn/piper
fn piper_dir() -> PathBuf {
    if let Ok(p) = std::env::var("EALEARN_PIPER_DIR") {
        return PathBuf::from(p);
    }
    if let Ok(x) = std::env::var("XDG_DATA_HOME") {
        return PathBuf::from(x).join("ealearn/piper");
    }
    PathBuf::from(std::env::var("HOME").unwrap_or_else(|_| ".".into())).join(".local/share/ealearn/piper")
}

/// 按音色标识造引擎。piper 的模型目录取 `EALEARN_PIPER_DIR`，默认
/// `<data>/piper`；找不到模型时返回错误而不是静默降级。
pub fn engine_for(spec: &str) -> Result<Box<dyn Tts>, TtsError> {
    let (engine, name) = parse_voice(spec)?;
    match engine {
        "espeak" => Ok(Box::new(Espeak::new(name))),
        "piper" => {
            let model = piper_dir().join(format!("{name}.onnx"));
            if !model.exists() {
                return Err(TtsError::Unsupported(format!(
                    "没有找到模型 {}，先下载 piper 音色或用 espeak:<lang> 顶替",
                    model.display()
                )));
            }
            Ok(Box::new(Piper { model }))
        }
        other => Err(TtsError::Unsupported(format!("未知引擎 {other}"))),
    }
}

/// 合成并写进 `dir`，返回相对该目录的文件名。已存在则直接复用。
pub fn synth_to_dir(
    engine: &dyn Tts,
    spec: &str,
    text: &str,
    dir: &Path,
) -> Result<String, TtsError> {
    let name = cache_name(spec, text);
    let path = dir.join(&name);
    if path.exists() {
        return Ok(name);
    }
    let wav = engine.synth_wav(text)?;
    std::fs::create_dir_all(dir).map_err(|e| TtsError::Empty(e.to_string()))?;
    std::fs::write(&path, wav).map_err(|e| TtsError::Empty(e.to_string()))?;
    Ok(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_voice_splits_engine_and_name() {
        assert_eq!(parse_voice("espeak:en-us").unwrap(), ("espeak", "en-us"));
        assert_eq!(
            parse_voice("piper:en_US-amy").unwrap(),
            ("piper", "en_US-amy")
        );
        assert!(parse_voice("nope").is_err());
        assert!(parse_voice(":x").is_err());
    }

    #[test]
    fn cache_name_is_stable_and_voice_sensitive() {
        let a = cache_name("espeak:en-us", "hello");
        let b = cache_name("espeak:en-us", "hello");
        let c = cache_name("espeak:en-gb", "hello");
        let d = cache_name("espeak:en-us", "hello!");
        assert_eq!(a, b);
        assert_ne!(a, c);
        assert_ne!(a, d);
        assert!(a.ends_with(".wav"));
        assert_eq!(a.len(), 64 + 4);
    }

    #[test]
    fn unknown_engine_is_rejected() {
        assert!(matches!(
            engine_for("festival:kal"),
            Err(TtsError::Unsupported(_))
        ));
    }

    #[test]
    fn espeak_synthesizes_when_available() {
        if Command::new("espeak-ng").arg("--version").output().is_err() {
            eprintln!("跳过：本机没有 espeak-ng");
            return;
        }
        let wav = Espeak::new("en-us").synth_wav("hello there").unwrap();
        assert!(wav.len() > 44, "音频太短: {} 字节", wav.len());
        assert_eq!(&wav[0..4], b"RIFF");
        assert_eq!(&wav[8..12], b"WAVE");
    }

    #[test]
    fn korean_voice_is_available() {
        if Command::new("espeak-ng").arg("--version").output().is_err() {
            return;
        }
        let wav = Espeak::new("ko").synth_wav("안녕하세요").unwrap();
        assert!(wav.len() > 44);
    }
}

// ── 音色枚举 ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct VoiceInfo {
    /// 直接可写进 manifest / 设置里的标识，形如 `espeak:en-us`
    pub id: String,
    /// 给用户看的名字
    pub label: String,
    pub engine: String,
    /// 模型是否就绪（espeak 恒为 true）
    pub ready: bool,
}

/// 列出某语言可用的音色：espeak 的内置音色 + 本机已下载的 piper 模型。
pub fn available(lang: &str) -> Vec<VoiceInfo> {
    let mut out = Vec::new();

    if let Ok(listing) = Command::new("espeak-ng")
        .arg(format!("--voices={lang}"))
        .output()
    {
        if listing.status.success() {
            let text = String::from_utf8_lossy(&listing.stdout);
            out.extend(parse_espeak_voices(&text, lang));
        }
    }

    if let Ok(entries) = std::fs::read_dir(piper_dir()) {
        let prefix = format!("{lang}_");
        let mut models: Vec<VoiceInfo> = entries
            .flatten()
            .filter_map(|e| {
                let path = e.path();
                let name = path.file_name()?.to_str()?.to_string();
                let stem = name.strip_suffix(".onnx")?.to_string();
                if !stem.starts_with(&prefix) {
                    return None;
                }
                Some(VoiceInfo {
                    id: format!("piper:{stem}"),
                    label: format!("piper · {stem}"),
                    engine: "piper".into(),
                    ready: path.with_extension("onnx.json").exists(),
                })
            })
            .collect();
        models.sort_by(|a, b| a.id.cmp(&b.id));
        out.extend(models);
    }

    out
}

/// 解析 `espeak-ng --voices=<lang>` 的输出。第二列是可直接用于 `-v` 的音色名。
fn parse_espeak_voices(text: &str, lang: &str) -> Vec<VoiceInfo> {
    text.lines()
        .skip(1)
        .filter_map(|line| {
            let cols: Vec<&str> = line.split_whitespace().collect();
            if cols.len() < 4 {
                return None;
            }
            let id = cols[1];
            if !id.starts_with(lang) {
                return None;
            }
            Some(VoiceInfo {
                id: format!("espeak:{id}"),
                label: format!("espeak · {}", cols[3].replace('_', " ")),
                engine: "espeak".into(),
                ready: true,
            })
        })
        .collect()
}

#[cfg(test)]
mod voice_tests {
    use super::*;

    const SAMPLE: &str = "Pty Language       Age/Gender VoiceName          File                 Other Languages
 2  en-gb           --/M      English_(Great_Britain) gmw/en               (en 2)
 2  en-us           --/M      English_(America)  gmw/en-US            (en 3)
 5  ko              --/M      Korean             ko                   ";

    #[test]
    fn parses_espeak_listing() {
        let v = parse_espeak_voices(SAMPLE, "en");
        assert_eq!(v.len(), 2);
        assert_eq!(v[0].id, "espeak:en-gb");
        assert_eq!(v[1].label, "espeak · English (America)");
        assert!(v.iter().all(|x| x.ready));
    }

    #[test]
    fn filters_by_language() {
        let v = parse_espeak_voices(SAMPLE, "ko");
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].id, "espeak:ko");
    }

    #[test]
    fn empty_listing_is_safe() {
        assert!(parse_espeak_voices("", "en").is_empty());
        assert!(parse_espeak_voices("only one column", "en").is_empty());
    }
}
