use serde::{Deserialize, Serialize};

/// pack 的元信息。`profile` 决定用哪个 `LanguageProfile` 实现，
/// `voice` 决定 TTS 音色，两者都与具体内容无关。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Manifest {
    /// pack 的 schema 版本，必须等于 `crate::SCHEMA_VERSION`。
    pub schema: u32,
    /// BCP-47 语言标签：en / ko ...
    pub lang: String,
    /// 面向学习者的语言名（中文）。
    pub display: String,
    pub version: String,
    /// 语言适配实现名，见 `ealearn-lang`。
    pub profile: String,
    /// TTS 音色标识。
    pub voice: String,
    /// pack 内容（不含 manifest 自身）的 sha256。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_hash: Option<String>,
}
