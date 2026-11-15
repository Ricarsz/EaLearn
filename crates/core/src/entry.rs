use serde::{Deserialize, Serialize};

/// 统一标注单元。
///
/// 英语的 token 是「词」，韩语的 token 是「形态素」（밥을 → 밥 + 을）。
/// 两种语言的形状必须完全一致，否则「换语言只换 pack」不成立。
///
/// `span` 是 `surface` 中的字节区间 `[start, end)`，校验器要求所有 token
/// 首尾相接覆盖整个 `surface`，这是防止 LLM 生成的标注与原文错位的唯一手段。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Token {
    pub surface: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lemma: Option<String>,
    /// 英语为 IPA，韩语为罗马字。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reading: Option<String>,
    /// 词性标签，取值见各 `LanguageProfile::grammar_tags`。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pos: Option<String>,
    /// 面向学习者的母语释义。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gloss: Option<String>,
    pub span: (usize, usize),
}

impl Token {
    pub fn new(surface: impl Into<String>, span: (usize, usize)) -> Self {
        Self {
            surface: surface.into(),
            lemma: None,
            reading: None,
            pos: None,
            gloss: None,
            span,
        }
    }

    pub fn with_lemma(mut self, lemma: impl Into<String>) -> Self {
        self.lemma = Some(lemma.into());
        self
    }

    pub fn with_pos(mut self, pos: impl Into<String>) -> Self {
        self.pos = Some(pos.into());
        self
    }

    /// 标注查询用的键：优先 lemma，退回 surface（小写）。
    pub fn key(&self) -> String {
        self.lemma
            .clone()
            .unwrap_or_else(|| self.surface.to_lowercase())
    }
}

/// 一条可学习内容：词、短语、整句都用同一个形状。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    pub id: String,
    pub surface: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lemma: Option<String>,
    /// 面向学习者的母语释义。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gloss: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tokens: Vec<Token>,
    /// 相对 pack 根目录的音频路径（预合成 TTS）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub audio: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub difficulty: Option<u8>,
}

impl Entry {
    pub fn new(id: impl Into<String>, surface: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            surface: surface.into(),
            lemma: None,
            gloss: None,
            tags: Vec::new(),
            tokens: Vec::new(),
            audio: None,
            difficulty: None,
        }
    }

    pub fn has_tag(&self, tag: &str) -> bool {
        self.tags.iter().any(|t| t == tag)
    }

    pub fn is_slang(&self) -> bool {
        self.has_tag("slang")
    }

    /// 有没有足够的素材支撑听音选词。
    pub fn audio_ready(&self) -> bool {
        self.audio.is_some()
    }
}
