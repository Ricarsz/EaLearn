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
    /// 用户最近不熟的词，本轮尽量用上 1–2 个（在句子里自然出现才算复习）
    pub target_words: &'a [String],
}

pub fn system_prompt(ctx: &PromptContext) -> String {
    let slang = if ctx.slang.is_empty() {
        "（本课没有指定表达）".to_string()
    } else {
        ctx.slang.join("、")
    };
    let targets = if ctx.target_words.is_empty() {
        "（暂无）".to_string()
    } else {
        ctx.target_words.join("、")
    };
    let level = ctx.level.clamp(1, 5);
    let register = if ctx.lang == "ko" {
        "韩语默认用 해요体（礼貌但不生硬），用户主动改用 반말 之后再跟着改。"
    } else {
        "用日常口语，缩略形式（gonna / kinda）可以用，但不要为了显得地道而堆俚语。"
    };
    let length = match level {
        1 => "每句不超过 8 个词，只用最常见的词和简单句。",
        2 => "每句 8–12 个词，可以用一个从句。",
        3 => "每句 10–15 个词，可以有从句和常见短语动词。",
        _ => "自然长度，可以有口语省略和俚语。",
    };

    format!(
        r#"你是 EaLearn 的语伴，陪中文母语者练「{label}」。当前话题：{theme}。难度：{level}/5。

写作要求：
- 每次只说 1–2 句，像真人发消息，不要演讲。
- {length}
- {register}
- 不要为了教学而重复用户的话，也不要在正文里夹中文解释。
- 用户如果改用中文说话，说明他没跟上：把上一句用更简单的说法重说一遍，并在 translation 里给中文。
- 不要连续追问三个问题，允许自然结束话题。

规则：
1. 只输出一个 JSON 对象。不要代码围栏、不要任何解释文字、不要在 JSON 前后加话。
2. 第一个字段必须是 "reply"，值是你要说的话。
3. "translation" 是 "reply" 的中文直译，保持口语，不要意译成文雅书面语。
4. "annotations" 从 "reply" 里挑值得学的词或短语，最多 4 条，每条：
   {{"surface": 逐字出现在 reply 里的片段, "lemma": 原形或词典形, "kind": "slang|idiom|grammar|vocab", "gloss": 中文释义, "note": 用法提示（可省）}}
   surface 必须能在 reply 中原文找到，否则整条会被丢弃。只标真正值得学的，不要把每个词都标上。
5. 如果用户上一句有语法或用词错误，"correction" 给出 {{"original": 用户原句, "fixed": 改正后, "note": 错在哪}}；没有错误就不要这个字段。
6. "difficulty" 用 1–5 表示这条 reply 的难度。
7. 只改语言形式，不要纠正用户想表达的内容。

本课想覆盖的表达（能自然用上就用，不要硬塞，没用上也没关系）：{slang}
用户最近不熟的词（挑 1–2 个自然用进句子，用完在 annotations 里标出来）：{targets}

输出示例：
{{"reply":"...","translation":"...","annotations":[{{"surface":"...","kind":"slang","gloss":"..."}}],"difficulty":2}}"#,
        label = ctx.target_label,
        theme = ctx.theme,
        level = level,
        length = length,
        register = register,
        slang = slang,
        targets = targets
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
            target_words: &[],
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
    fn prompt_includes_target_words() {
        let slang = Vec::new();
        let targets = vec!["ghost".to_string(), "vibe".to_string()];
        let p = system_prompt(&PromptContext {
            lang: "en",
            target_label: "英语",
            theme: "gossip",
            level: 3,
            slang: &slang,
            target_words: &targets,
        });
        assert!(p.contains("ghost、vibe"));
        assert!(p.contains("10–15"));
    }

    #[test]
    fn korean_prompt_mentions_register() {
        let slang = Vec::new();
        let p = system_prompt(&PromptContext {
            lang: "ko",
            target_label: "韩语",
            theme: "gossip",
            level: 2,
            slang: &slang,
            target_words: &[],
        });
        assert!(p.contains("해요体"));
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
