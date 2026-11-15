use serde::{Deserialize, Serialize};

use crate::entry::Token;

/// 文章模式：正文 + 阅读理解题。
///
/// `tokens` 覆盖 `body`，前端据此在正文上叠加可点的查词层（复用情景模式
/// 的标注层渲染器）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Article {
    pub id: String,
    pub title: String,
    pub body: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tokens: Vec<Token>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub questions: Vec<Question>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub difficulty: Option<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Question {
    pub prompt: String,
    pub options: Vec<String>,
    /// 正确选项下标。
    pub answer: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub explain: Option<String>,
}

impl Article {
    pub fn word_count(&self) -> usize {
        self.tokens.len()
    }

    pub fn is_well_formed(&self) -> bool {
        !self.questions.is_empty()
            && self
                .questions
                .iter()
                .all(|q| q.options.len() >= 2 && q.answer < q.options.len())
    }
}
