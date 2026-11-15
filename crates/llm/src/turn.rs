//! 一轮回复的结构化解析。
//!
//! 模型输出的是 JSON，但它不是可信来源：可能加代码围栏、可能标注它自己没说过的
//! 词、可能给出没改动的「纠正」。这里做三件事——剥外壳、拒坏数据、把可疑内容降级
//! 成警告而不是让整个对话失败。

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Annotation {
    /// 原文里出现的片段，必须是 `reply` 的子串
    pub surface: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lemma: Option<String>,
    /// slang / idiom / grammar / vocab
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gloss: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Correction {
    pub original: String,
    pub fixed: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TutorTurn {
    pub reply: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub translation: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub annotations: Vec<Annotation>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub correction: Option<Correction>,
    /// 1–5，越大越难
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub difficulty: Option<u8>,
    /// 被丢弃或修正的内容，给开发排查用，不显示给学习者
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TurnError {
    NoJson,
    BadJson(String),
    EmptyReply,
}

impl std::fmt::Display for TurnError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TurnError::NoJson => write!(f, "回复里没有 JSON 对象"),
            TurnError::BadJson(e) => write!(f, "回复 JSON 解析失败: {e}"),
            TurnError::EmptyReply => write!(f, "回复正文为空"),
        }
    }
}

impl std::error::Error for TurnError {}

/// 最多保留的标注条数。标得太密会让界面变成一片下划线。
const MAX_ANNOTATIONS: usize = 12;

pub fn parse_turn(raw: &str) -> Result<TutorTurn, TurnError> {
    let body = strip_fence(raw).unwrap_or(raw);
    let start = body.find('{').ok_or(TurnError::NoJson)?;
    let end = body.rfind('}').ok_or(TurnError::NoJson)?;
    if end <= start {
        return Err(TurnError::NoJson);
    }
    let json = &body[start..=end];

    #[derive(Deserialize)]
    struct Raw {
        #[serde(default)]
        reply: String,
        #[serde(default)]
        translation: Option<String>,
        #[serde(default)]
        annotations: Vec<Annotation>,
        #[serde(default)]
        correction: Option<Correction>,
        #[serde(default)]
        difficulty: Option<i64>,
    }

    let raw: Raw = serde_json::from_str(json).map_err(|e| TurnError::BadJson(e.to_string()))?;
    let reply = raw.reply.trim().to_string();
    if reply.is_empty() {
        return Err(TurnError::EmptyReply);
    }

    let mut warnings = Vec::new();

    // 标注必须真的出现在回复里，否则学习者点不到、也复习不了
    let mut annotations: Vec<Annotation> = Vec::new();
    for mut a in raw.annotations {
        a.surface = a.surface.trim().to_string();
        if a.surface.is_empty() {
            continue;
        }
        if !reply.to_lowercase().contains(&a.surface.to_lowercase()) {
            warnings.push(format!("丢弃标注「{}」：回复里没有这个词", a.surface));
            continue;
        }
        if annotations.iter().any(|x| x.surface == a.surface) {
            continue;
        }
        if annotations.len() >= MAX_ANNOTATIONS {
            warnings.push("标注超过上限，其余已丢弃".into());
            break;
        }
        annotations.push(a);
    }

    let correction = raw.correction.and_then(|c| {
        if c.fixed.trim().is_empty() || c.fixed.trim() == c.original.trim() {
            warnings.push("丢弃空纠正".into());
            None
        } else {
            Some(c)
        }
    });

    let difficulty = raw.difficulty.and_then(|d| {
        if d <= 0 {
            return None;
        }
        Some(d.clamp(1, 5) as u8)
    });

    Ok(TutorTurn {
        reply,
        translation: raw.translation.filter(|t| !t.trim().is_empty()),
        annotations,
        correction,
        difficulty,
        warnings,
    })
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
        "reply": "Did you hear? She totally ghosted him.",
        "translation": "你听说了吗？她直接把那人玩消失了。",
        "annotations": [
            {"surface":"ghosted","lemma":"ghost","kind":"slang","gloss":"玩消失"},
            {"surface":"banana","kind":"vocab","gloss":"香蕉"}
        ],
        "correction": {"original":"she ghost him","fixed":"she ghosted him","note":"过去式"},
        "difficulty": 2
    }"#;

    #[test]
    fn parses_and_drops_unmatched_annotation() {
        let turn = parse_turn(GOOD).unwrap();
        assert_eq!(turn.reply, "Did you hear? She totally ghosted him.");
        assert_eq!(turn.annotations.len(), 1);
        assert_eq!(turn.annotations[0].surface, "ghosted");
        assert_eq!(turn.correction.unwrap().fixed, "she ghosted him");
        assert!(turn.warnings.iter().any(|w| w.contains("banana")));
    }

    #[test]
    fn strips_markdown_fence() {
        let wrapped = format!("```json\n{GOOD}\n```");
        let turn = parse_turn(&wrapped).unwrap();
        assert!(turn.reply.starts_with("Did you hear"));
    }

    #[test]
    fn ignores_prose_around_json() {
        let noisy = format!("当然可以：\n{GOOD}\n希望有帮助。");
        assert!(parse_turn(&noisy).is_ok());
    }

    #[test]
    fn dedupes_annotations() {
        let raw = r#"{"reply":"gonna go","annotations":[
            {"surface":"gonna"},{"surface":"gonna"},{"surface":"go"}]}"#;
        let turn = parse_turn(raw).unwrap();
        assert_eq!(turn.annotations.len(), 2);
    }

    #[test]
    fn caps_annotation_count() {
        let mut items = Vec::new();
        for i in 0..20 {
            items.push(format!("{{\"surface\":\"w{i}\"}}"));
        }
        let reply = (0..20)
            .map(|i| format!("w{i}"))
            .collect::<Vec<_>>()
            .join(" ");
        let raw = format!(
            "{{\"reply\":\"{reply}\",\"annotations\":[{}]}}",
            items.join(",")
        );
        let turn = parse_turn(&raw).unwrap();
        assert_eq!(turn.annotations.len(), MAX_ANNOTATIONS);
    }

    #[test]
    fn drops_noop_correction() {
        let raw = r#"{"reply":"ok","correction":{"original":"ok","fixed":" ok "}}"#;
        assert!(parse_turn(raw).unwrap().correction.is_none());
    }

    #[test]
    fn clamps_difficulty() {
        let raw = r#"{"reply":"hi","difficulty":99}"#;
        assert_eq!(parse_turn(raw).unwrap().difficulty, Some(5));
        let zero = r#"{"reply":"hi","difficulty":0}"#;
        assert_eq!(parse_turn(zero).unwrap().difficulty, None);
    }

    #[test]
    fn rejects_empty_reply() {
        assert_eq!(parse_turn(r#"{"reply":"   "}"#), Err(TurnError::EmptyReply));
    }

    #[test]
    fn rejects_non_json() {
        assert_eq!(parse_turn("抱歉，我不能回答"), Err(TurnError::NoJson));
        assert!(matches!(
            parse_turn(r#"{"reply": "#),
            Err(TurnError::BadJson(_)) | Err(TurnError::NoJson)
        ));
    }

    #[test]
    fn annotation_match_is_case_insensitive() {
        let raw = r#"{"reply":"Gonna go","annotations":[{"surface":"gonna"}]}"#;
        assert_eq!(parse_turn(raw).unwrap().annotations.len(), 1);
    }
}
