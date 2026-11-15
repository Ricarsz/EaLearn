//! 提示词构造。
//!
//! 协议要求：模型只吐一个 JSON 对象，`reply` 必须是第一个字段——`reply` 排在前
//! 面，[`crate::reply`] 才能在流式阶段把正文提出来做打字效果。

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

impl ChatMessage {
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: "system".into(),
            content: content.into(),
        }
    }
    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: "user".into(),
            content: content.into(),
        }
    }
    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: "assistant".into(),
            content: content.into(),
        }
    }
}

pub struct PromptContext<'a> {
    /// 目标语言代码：en / ko
    pub lang: &'a str,
    /// 面向学习者的语言名，用于提示词里自述
    pub target_label: &'a str,
    /// 话题：gossip / dating / smalltalk / complaint / shopping ...
    pub theme: &'a str,
    /// 1–5
    pub level: u8,
    /// 本课希望覆盖的俚语或固定表达，能自然用上就用
    pub slang: &'a [String],
}

pub fn system_prompt(ctx: &PromptContext) -> String {
    let slang = if ctx.slang.is_empty() {
        "（本课没有指定表达）".to_string()
    } else {
        ctx.slang.join("、")
    };
    format!(
        r#"你是 EaLearn 的语伴，陪中文母语者练「{label}」。当前话题：{theme}。难度：{level}/5（1 最简单，5 接近母语者日常）。

规则：
1. 只输出一个 JSON 对象。不要代码围栏、不要任何解释文字、不要在 JSON 前后加话。
2. 第一个字段必须是 "reply"，值是你要说的话，1–3 句，像真人发消息一样自然口语，不要书面腔。
3. "translation" 是 "reply" 的中文直译，保持口语，不要意译成文雅书面语。
4. "annotations" 从 "reply" 里挑值得学的词或短语，最多 6 条，每条：
   {{"surface": 逐字出现在 reply 里的片段, "lemma": 原形或词典形, "kind": "slang|idiom|grammar|vocab", "gloss": 中文释义, "note": 用法提示（可省）}}
   surface 必须能在 reply 中原文找到，否则整条会被丢弃。
5. 如果用户上一句有语法或用词错误，"correction" 给出 {{"original": 用户原句, "fixed": 改正后, "note": 错在哪}}；没有错误就不要这个字段。
6. "difficulty" 用 1–5 表示这条 reply 的难度。
7. 不要纠正用户想表达的内容，只改语言形式；不要连续追问，允许自然结束话题。

本课想覆盖的表达（能自然用上就用，不要硬塞）：{slang}

输出示例：
{{"reply":"...","translation":"...","annotations":[{{"surface":"...","kind":"slang","gloss":"..."}}],"difficulty":2}}"#,
        label = ctx.target_label,
        theme = ctx.theme,
        level = ctx.level.clamp(1, 5),
        slang = slang
    )
}

/// 拼出发给模型的完整消息序列。`history` 是 (role, content) 的旧对话。
pub fn build_messages(
    system: String,
    history: &[(String, String)],
    user: &str,
) -> Vec<ChatMessage> {
    let mut out = vec![ChatMessage::system(system)];
    for (role, content) in history {
        out.push(ChatMessage {
            role: role.clone(),
            content: content.clone(),
        });
    }
    out.push(ChatMessage::user(user));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx<'a>(slang: &'a [String]) -> PromptContext<'a> {
        PromptContext {
            lang: "en",
            target_label: "英语",
            theme: "gossip",
            level: 2,
            slang,
        }
    }

    #[test]
    fn prompt_states_the_contract() {
        let p = system_prompt(&ctx(&[]));
        assert!(p.contains("\"reply\""));
        assert!(p.contains("第一个字段"));
        assert!(p.contains("不要代码围栏"));
        assert!(p.contains("gossip"));
    }

    #[test]
    fn prompt_lists_slang() {
        let slang = vec!["ghost".to_string(), "spill the tea".to_string()];
        let p = system_prompt(&ctx(&slang));
        assert!(p.contains("ghost、spill the tea"));
    }

    #[test]
    fn level_is_clamped() {
        let mut c = ctx(&[]);
        c.level = 99;
        assert!(system_prompt(&c).contains("难度：5/5"));
    }

    #[test]
    fn messages_order_system_history_user() {
        let history = vec![
            ("user".to_string(), "hi".to_string()),
            ("assistant".to_string(), "hey".to_string()),
        ];
        let msgs = build_messages("SYS".into(), &history, "how are you");
        assert_eq!(msgs.len(), 4);
        assert_eq!(msgs[0].role, "system");
        assert_eq!(msgs[3].content, "how are you");
    }
}
