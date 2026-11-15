//! 生成剧本对话（dialogs.json）的提示词与解析。
//!
//! 和自由聊不同，这里要的是一张**对话图**：节点固定、跳转明确、没有环。
//! 模型很容易生成「互相引用」的节点，所以解析后一律用
//! `Dialog::is_well_formed` 卡一遍：起始节点存在、跳转目标存在、无环。

use ealearn_core::dialog::{Dialog, Node, Speaker};

pub struct DialogPromptCtx<'a> {
    pub lang: &'a str,
    pub target_label: &'a str,
    /// 话题，例如「被人放鸽子」
    pub theme: &'a str,
    /// 1–5
    pub level: u8,
    /// 想要多少条消息节点
    pub nodes: usize,
    /// 希望覆盖的词或表达
    pub target_words: &'a [String],
}

pub fn dialog_prompt(ctx: &DialogPromptCtx) -> String {
    let words = if ctx.target_words.is_empty() {
        "（不指定）".to_string()
    } else {
        ctx.target_words.join("、")
    };
    let register = if ctx.lang == "ko" {
        "韩语默认 해요体；只有明确是同龄密友之间才用 반말，且要标 anchor。"
    } else {
        "用日常口语，可以有 gonna / kinda 这类缩略，但别为了显得地道而堆俚语。"
    };

    format!(
        r#"你在为一款语言学习应用写一段「情景对话」。学习者扮演 B（自己），对方是 A。话题：{theme}。目标语言：{label}。难度：{level}/5。

要求：
1. 只输出一个 JSON 对象，不要代码围栏，不要解释。
2. 结构固定：
{{"title":"中文短标题","theme":"一句话话题","start":"n1","nodes":[ ... ]}}
3. 每个节点：
   - "id"：n1、n2…按顺序编号
   - "speaker"："a"（对方）/"b"（学习者）/"narrator"（舞台提示，text 留空）
   - "text"：这条消息的原文。narrator 节点留空字符串。
   - "next"：下一条的 id；如果这个节点是学习者要选回复的地方，就不要 next
   - "choices"：只在学习者做选择时出现，2–3 个，每个 {{"text":"学习者说的话","next":"目标节点id","awkward":false}}
     awkward 表示这句在语气/礼貌度上不太合适
4. 跳转只能往后，禁止成环；所有 next / choices.next 必须指向真实存在的节点 id。
5. 总共约 {nodes} 个节点，其中 A 说 3–5 句，学习者至少做 2 次选择。
6. 对话要有一个自然的结尾节点（没有 next 也没有 choices）。

写作要求：
- {register}
- 难度对应：1–2 用短句和最常见词；3 可以有一个从句；4–5 允许地道俚语。
- 俚语、缩写、语气词自然出现即可，不要每条都塞。
- 希望覆盖的表达（能用就用，别硬塞）：{words}

示例：
{{"title":"被放鸽子","theme":"朋友又临时爽约","start":"n1","nodes":[
 {{"id":"n1","speaker":"a","text":"...","next":"n2"}},
 {{"id":"n2","speaker":"narrator","text":"","choices":[{{"text":"...","next":"n3","awkward":false}},{{"text":"...","next":"n3","awkward":true}}]}},
 {{"id":"n3","speaker":"a","text":"..."}}
]}}"#,
        theme = ctx.theme,
        label = ctx.target_label,
        level = ctx.level.clamp(1, 5),
        nodes = ctx.nodes.clamp(4, 20),
        register = register,
        words = words
    )
}

/// 模型输出的草稿：还没做标注，也没校验。
pub type DialogDraft = Dialog;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DialogParseError {
    NoJson,
    BadJson(String),
    IllFormed(String),
}

impl std::fmt::Display for DialogParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DialogParseError::NoJson => write!(f, "回复里没有 JSON 对象"),
            DialogParseError::BadJson(e) => write!(f, "JSON 解析失败: {e}"),
            DialogParseError::IllFormed(why) => write!(f, "对话图不合法: {why}"),
        }
    }
}

impl std::error::Error for DialogParseError {}

/// 解析一段模型输出，返回校验过的对话图（tokens 为空，由 forge 再补）。
pub fn parse_dialogs(raw: &str, id: &str) -> Result<Dialog, DialogParseError> {
    let body = strip_fence(raw).unwrap_or(raw);
    let start = body.find('{').ok_or(DialogParseError::NoJson)?;
    let end = body.rfind('}').ok_or(DialogParseError::NoJson)?;
    if end <= start {
        return Err(DialogParseError::NoJson);
    }
    let json = &body[start..=end];

    #[derive(serde::Deserialize)]
    struct Raw {
        #[serde(default)]
        title: String,
        #[serde(default)]
        theme: String,
        #[serde(default)]
        start: String,
        #[serde(default)]
        nodes: Vec<RawNode>,
        #[serde(default)]
        tags: Vec<String>,
    }
    #[derive(serde::Deserialize)]
    struct RawNode {
        #[serde(default)]
        id: String,
        #[serde(default)]
        speaker: String,
        #[serde(default)]
        text: String,
        #[serde(default)]
        next: Option<String>,
        #[serde(default)]
        choices: Vec<RawChoice>,
    }
    #[derive(serde::Deserialize)]
    struct RawChoice {
        #[serde(default)]
        text: String,
        #[serde(default)]
        next: String,
        #[serde(default)]
        awkward: bool,
    }

    let raw: Raw = serde_json::from_str(json).map_err(|e| DialogParseError::BadJson(e.to_string()))?;
    if raw.nodes.is_empty() {
        return Err(DialogParseError::IllFormed("没有任何节点".into()));
    }

    let nodes: Vec<Node> = raw
        .nodes
        .into_iter()
        .filter(|n| !n.id.trim().is_empty())
        .map(|n| Node {
            id: n.id,
            speaker: match n.speaker.to_lowercase().as_str() {
                "b" => Speaker::B,
                "narrator" | "narration" => Speaker::Narrator,
                _ => Speaker::A,
            },
            text: n.text,
            tokens: Vec::new(),
            next: n.next.filter(|s| !s.trim().is_empty()),
            choices: n
                .choices
                .into_iter()
                .filter(|c| !c.text.trim().is_empty() && !c.next.trim().is_empty())
                .map(|c| ealearn_core::dialog::Choice {
                    text: c.text,
                    next: c.next,
                    reply: None,
                    awkward: c.awkward,
                })
                .collect(),
        })
        .collect();

    let start = if raw.start.trim().is_empty() {
        nodes.first().map(|n| n.id.clone()).unwrap_or_default()
    } else {
        raw.start
    };

    let dialog = Dialog {
        id: id.to_string(),
        title: if raw.title.trim().is_empty() { "未命名对话".into() } else { raw.title },
        theme: raw.theme,
        start,
        nodes,
        tags: raw.tags,
    };

    if !dialog.is_well_formed() {
        return Err(DialogParseError::IllFormed(
            "起始节点缺失、跳转目标不存在或存在环".into(),
        ));
    }
    Ok(dialog)
}

fn strip_fence(raw: &str) -> Option<&str> {
    let start = raw.find("```")?;
    let after = &raw[start + 3..];
    let after = after.strip_prefix("json").unwrap_or(after);
    let after = after.strip_prefix('\n').unwrap_or(after);
    let end = after.find("```")?;
    Some(&after[..end])
}

#[cfg(test)]
mod tests {
    use super::*;

    const GOOD: &str = r#"{
      "title":"被放鸽子","theme":"朋友又爽约","start":"n1","nodes":[
        {"id":"n1","speaker":"a","text":"You coming tonight?","next":"n2"},
        {"id":"n2","speaker":"narrator","text":"","choices":[
          {"text":"Yeah, I am on my way.","next":"n3"},
          {"text":"Probably not.","next":"n3","awkward":true}]},
        {"id":"n3","speaker":"a","text":"Cool, see you."}
      ]}"#;

    fn ctx<'a>(lang: &'a str, words: &'a [String]) -> DialogPromptCtx<'a> {
        DialogPromptCtx {
            lang,
            target_label: "英语",
            theme: "被人放鸽子",
            level: 2,
            nodes: 6,
            target_words: words,
        }
    }

    #[test]
    fn prompt_specifies_graph_rules() {
        let p = dialog_prompt(&ctx("en", &[]));
        assert!(p.contains("禁止成环"));
        assert!(p.contains("\"choices\""));
        assert!(p.contains("awkward"));
        assert!(p.contains("没有 next 也没有 choices"));
    }

    #[test]
    fn korean_prompt_mentions_register() {
        let p = dialog_prompt(&ctx("ko", &[]));
        assert!(p.contains("해요体"));
    }

    #[test]
    fn parses_valid_dialog() {
        let d = parse_dialogs(GOOD, "en.d.test").unwrap();
        assert_eq!(d.id, "en.d.test");
        assert_eq!(d.title, "被放鸽子");
        assert_eq!(d.nodes.len(), 3);
        assert_eq!(d.start_node().unwrap().text, "You coming tonight?");
        let n2 = d.node("n2").unwrap();
        assert_eq!(n2.choices.len(), 2);
        assert!(n2.choices[1].awkward);
    }

    #[test]
    fn strips_markdown_fence() {
        let wrapped = format!("```json\n{GOOD}\n```");
        assert!(parse_dialogs(&wrapped, "x").is_ok());
    }

    #[test]
    fn rejects_cycle() {
        let bad = r#"{"title":"t","start":"a","nodes":[
            {"id":"a","speaker":"a","text":"hi","next":"b"},
            {"id":"b","speaker":"a","text":"yo","next":"a"}]}"#;
        assert!(matches!(parse_dialogs(bad, "x"), Err(DialogParseError::IllFormed(_))));
    }

    #[test]
    fn rejects_dangling_jump() {
        let bad = r#"{"title":"t","start":"a","nodes":[
            {"id":"a","speaker":"a","text":"hi","next":"nowhere"}]}"#;
        assert!(matches!(parse_dialogs(bad, "x"), Err(DialogParseError::IllFormed(_))));
    }

    #[test]
    fn speaker_maps_unknown_to_a() {
        let raw = r#"{"title":"t","start":"a","nodes":[
            {"id":"a","speaker":"B","text":"hey"},
            {"id":"b","speaker":"narrator","text":""}]}"#;
        let d = parse_dialogs(raw, "x").unwrap();
        assert_eq!(d.node("a").unwrap().speaker, Speaker::B);
        assert_eq!(d.node("b").unwrap().speaker, Speaker::Narrator);
    }

    #[test]
    fn fills_start_when_missing() {
        let raw = r#"{"title":"t","nodes":[{"id":"first","speaker":"a","text":"hi"}]}"#;
        let d = parse_dialogs(raw, "x").unwrap();
        assert_eq!(d.start, "first");
    }
}