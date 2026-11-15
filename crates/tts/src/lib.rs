//! TTS 合成层。
//!
//! 只用 piper：神经网络音色，英语/韩语都有可用模型，质量明显好过传统共振峰合成。
//! manifest 的 `voice` 字段写成 `piper:<模型名>`，合成时按这个名字去模型目录找
//! `<模型名>.onnx`。
//!
//! 音频一律按「音色标识 + 文本」的 sha256 命名并落在 pack 的 `media/audio/` 下，
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
                write!(f, "无法识别的音色「{v}」，应形如 piper:en_US-lessac")
            }
            TtsError::Unsupported(e) => write!(f, "该音色不可用: {e}"),
            TtsError::Spawn(e) => write!(f, "启动 piper 失败: {e}"),
            TtsError::Empty(e) => write!(f, "合成没有返回音频: {e}"),
        }
    }
}

impl std::error::Error for TtsError {}

/// `piper:en_US-lessac` → ("piper", "en_US-lessac")
pub fn parse_voice(spec: &str) -> Result<(&str, &str), TtsError> {
    let (engine, name) = spec
        .split_once(':')
        .ok_or_else(|| TtsError::UnknownVoice(spec.to_string()))?;
    if engine.is_empty() || name.is_empty() {
        return Err(TtsError::UnknownVoice(spec.to_string()));
    }
    Ok((engine, name))
}

/// 音频文件名：同一音色同一文本永远映射到同一个文件。
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

pub struct Piper {
    /// onnx 模型路径
    pub model: PathBuf,
    /// 可执行文件路径
    pub bin: PathBuf,
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
        let out =
            std::env::temp_dir().join(format!("ealearn-tts-{}-{stamp}.wav", std::process::id()));

        let mut child = Command::new(&self.bin)
            .args([
                "-m",
                &self.model.to_string_lossy(),
                "-f",
                &out.to_string_lossy(),
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| TtsError::Spawn(format!("{}: {e}", self.bin.display())))?;
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
        Ok(normalize_wav(bytes))
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
    PathBuf::from(std::env::var("HOME").unwrap_or_else(|_| ".".into()))
        .join(".local/share/ealearn/piper")
}

/// 模型目录，供报错信息与文档使用。
pub fn model_dir() -> PathBuf {
    piper_dir()
}

pub fn engine_for(spec: &str) -> Result<Box<dyn Tts>, TtsError> {
    let (engine, name) = parse_voice(spec)?;
    if engine != "piper" {
        return Err(TtsError::Unsupported(format!(
            "未知引擎 {engine}；现在只支持 piper，把模型放进 {}",
            piper_dir().display()
        )));
    }
    let model = piper_dir().join(format!("{name}.onnx"));
    if !model.exists() {
        return Err(TtsError::Unsupported(format!(
            "没有找到模型 {}",
            model.display()
        )));
    }
    Ok(Box::new(Piper {
        model,
        bin: piper_bin(),
    }))
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

/// 修正 RIFF/data 的长度字段。
///
/// 有些合成器（espeak-ng 的 `--stdout` 就是）会把两个长度写成占位值，GStreamer
/// 拿到后按「超长音频」处理，WebKitGTK 里表现为点了没声。这里统一按实际长度重写。
pub fn normalize_wav(mut bytes: Vec<u8>) -> Vec<u8> {
    if bytes.len() < 44 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return bytes;
    }

    let mut pos = 12usize;
    while pos + 8 <= bytes.len() {
        let id = &bytes[pos..pos + 4];
        let size = u32::from_le_bytes([
            bytes[pos + 4],
            bytes[pos + 5],
            bytes[pos + 6],
            bytes[pos + 7],
        ]) as usize;
        if id == b"data" {
            let payload = bytes.len() - (pos + 8);
            bytes[pos + 4..pos + 8].copy_from_slice(&(payload as u32).to_le_bytes());
            break;
        }
        if size == 0 || pos + 8 + size > bytes.len() {
            break;
        }
        pos += 8 + size + (size & 1);
    }

    let riff = (bytes.len() - 8) as u32;
    bytes[4..8].copy_from_slice(&riff.to_le_bytes());
    bytes
}

// ── 音色枚举 ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct VoiceInfo {
    /// 直接可写进 manifest / 设置里的标识，形如 `piper:en_US-lessac`
    pub id: String,
    /// 给用户看的名字
    pub label: String,
    pub engine: String,
    /// 配置文件是否就绪（缺 .onnx.json 时 piper 跑不起来）
    pub ready: bool,
}

/// 列出本机已下载的、属于该语言的 piper 模型。
pub fn available(lang: &str) -> Vec<VoiceInfo> {
    let Ok(entries) = std::fs::read_dir(piper_dir()) else {
        return Vec::new();
    };
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
                label: display_name(&stem),
                engine: "piper".into(),
                ready: path.with_extension("onnx.json").exists(),
            })
        })
        .collect();
    models.sort_by(|a, b| a.id.cmp(&b.id));
    models
}

/// `en_US-lessac` → `English (US) · lessac`，读不出语言就原样返回。
fn display_name(stem: &str) -> String {
    match stem.split_once('-') {
        Some((locale, voice)) => {
            let lang = match locale.split('_').next() {
                Some("en") => "英语",
                Some("ko") => "韩语",
                _ => "",
            };
            let region = locale.split_once('_').map(|(_, r)| r).unwrap_or("");
            if lang.is_empty() {
                format!("{stem}")
            } else if region.is_empty() {
                format!("{lang} · {voice}")
            } else {
                format!("{lang} ({region}) · {voice}")
            }
        }
        None => format!("piper · {stem}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn any_model() -> Option<PathBuf> {
        let dir = piper_dir();
        std::fs::read_dir(dir)
            .ok()?
            .flatten()
            .map(|e| e.path())
            .find(|p| p.extension().map(|x| x == "onnx").unwrap_or(false))
    }

    #[test]
    fn parse_voice_splits_engine_and_name() {
        assert_eq!(
            parse_voice("piper:en_US-lessac").unwrap(),
            ("piper", "en_US-lessac")
        );
        assert!(parse_voice("nope").is_err());
        assert!(parse_voice(":x").is_err());
    }

    #[test]
    fn cache_name_is_stable_and_voice_sensitive() {
        let a = cache_name("piper:en_US-amy", "hello");
        let b = cache_name("piper:en_US-amy", "hello");
        let c = cache_name("piper:en_US-lessac", "hello");
        let d = cache_name("piper:en_US-amy", "hello!");
        assert_eq!(a, b);
        assert_ne!(a, c);
        assert_ne!(a, d);
        assert!(a.ends_with(".wav"));
        assert_eq!(a.len(), 64 + 4);
    }

    #[test]
    fn unknown_engine_is_rejected() {
        assert!(matches!(
            engine_for("espeak:en-us"),
            Err(TtsError::Unsupported(_))
        ));
        assert!(matches!(
            engine_for("festival:kal"),
            Err(TtsError::Unsupported(_))
        ));
    }

    #[test]
    fn display_name_is_readable() {
        assert_eq!(display_name("en_US-lessac"), "英语 (US) · lessac");
        assert_eq!(display_name("ko_KR-kss"), "韩语 (KR) · kss");
        assert_eq!(display_name("weird"), "piper · weird");
    }

    #[test]
    fn normalize_fixes_bogus_sizes() {
        let mut wav = Vec::new();
        wav.extend_from_slice(b"RIFF");
        wav.extend_from_slice(&0x7F_FF_FF_24u32.to_le_bytes());
        wav.extend_from_slice(b"WAVEfmt ");
        wav.extend_from_slice(&16u32.to_le_bytes());
        wav.extend_from_slice(&[1, 0, 1, 0]);
        wav.extend_from_slice(&22_050u32.to_le_bytes());
        wav.extend_from_slice(&44_100u32.to_le_bytes());
        wav.extend_from_slice(&[2, 0, 16, 0]);
        wav.extend_from_slice(b"data");
        wav.extend_from_slice(&0x7F_FF_FF_24u32.to_le_bytes());
        wav.extend_from_slice(&[0u8; 100]);

        let fixed = normalize_wav(wav.clone());
        assert_eq!(fixed.len(), wav.len());
        assert_eq!(
            u32::from_le_bytes([fixed[4], fixed[5], fixed[6], fixed[7]]),
            (wav.len() - 8) as u32
        );
        assert_eq!(
            u32::from_le_bytes([fixed[40], fixed[41], fixed[42], fixed[43]]),
            100
        );
    }

    #[test]
    fn piper_synthesizes_when_model_present() {
        let Some(model) = any_model() else {
            eprintln!("跳过：本机没有 piper 模型");
            return;
        };
        let engine = Piper {
            model,
            bin: piper_bin(),
        };
        let wav = engine.synth_wav("hello there").unwrap();
        assert_eq!(&wav[0..4], b"RIFF");
        assert_eq!(&wav[8..12], b"WAVE");
        assert_eq!(
            u32::from_le_bytes([wav[4], wav[5], wav[6], wav[7]]) as usize,
            wav.len() - 8
        );
    }

    #[test]
    fn available_lists_only_local_models() {
        let all = available("zz");
        assert!(all.is_empty(), "不该给没有模型的语言编造音色");
        for v in available("en") {
            assert!(v.id.starts_with("piper:en_"), "{}", v.id);
            assert_eq!(v.engine, "piper");
        }
    }
}
