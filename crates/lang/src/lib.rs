//! 语言适配层。
//!
//! 每种语言实现一个 [`LanguageProfile`]，pack 的 manifest 里 `profile` 字段
//! 指向它。加一门语言 = 加一个实现 + 一个 pack 目录，app 与 src-tauri 不动。
//!
//! 语言之间真正有差异的只有五件事：怎么切词、怎么还原词形、怎么标读音、
//! 用什么音色、哪些词不算生词。除此之外的一切都必须在核心层共用。

pub mod en;
pub mod ko;

use ealearn_core::entry::Token;

/// TTS 音色描述。具体引擎在合成层决定，这里只声明需求。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct VoiceSpec {
    /// 引擎无关的音色标识，例如 `en_US-amy`。
    pub id: &'static str,
    pub lang: &'static str,
    pub sample_rate: u32,
}

/// 词性/语法标签的说明，用于前端标注层展示。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TagDef {
    pub tag: &'static str,
    pub desc: &'static str,
}

pub trait LanguageProfile: Send + Sync {
    /// 与 manifest 的 `profile` 字段一致。
    fn id(&self) -> &'static str;

    /// 切词并标注。返回的 token 必须满足 `ealearn_core::validate::check_coverage`：
    /// 首尾相接铺满原文（空格除外），每块内容与 span 指向的原文逐字节相等。
    fn tokenize(&self, text: &str) -> Vec<Token>;

    /// 词形还原。变体多、规则杂乱的语言走查表，不走算法猜。
    fn lemmatize(&self, surface: &str) -> Option<String>;

    /// 算法可得的读音：韩语罗马字。英语的 IPA 靠词典/生成阶段写入 pack，
    /// 这里返回 `None`。
    fn reading(&self, text: &str) -> Option<String>;

    fn voice(&self) -> VoiceSpec;

    /// 不算生词的词（用于从对话里挑出「用户没见过的词」）。
    fn stopwords(&self) -> &'static [&'static str];

    fn grammar_tags(&self) -> &'static [TagDef];
}

/// 按 manifest 的 `profile` 取实现。
pub fn profile(id: &str) -> Option<&'static dyn LanguageProfile> {
    match id {
        "en" => Some(&en::English),
        "ko" => Some(ko::korean()),
        _ => None,
    }
}

/// 已实现的语言列表，供设置页展示。
pub fn available() -> &'static [&'static str] {
    &["en", "ko"]
}