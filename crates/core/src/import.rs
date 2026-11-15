//! 用户自定义词条的导入解析。
//!
//! 每行一个词：`word<TAB>释义[<TAB>标签,标签]`，没有制表符时按逗号切。
//! 空行与 `#` 开头的行忽略，`surface` 重复的只留第一次。

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportedWord {
    pub surface: String,
    pub gloss: Option<String>,
    pub tags: Vec<String>,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ImportReport {
    pub words: Vec<ImportedWord>,
    /// 跳过的行及原因，给用户看
    pub skipped: Vec<String>,
}

/// 自动识别格式：以 `[` / `{` 开头按 JSON 词库解析（qwerty-learner 那种
/// `[{"name":"...","trans":[...]}]`），其余按逐行 TSV/CSV。
pub fn parse(text: &str) -> ImportReport {
    let head = text.trim_start();
    if head.starts_with('[') || head.starts_with('{') {
        return parse_json(text);
    }
    parse_lines(text)
}

/// 一行词表项。兼容几种常见字段名：
/// - 词：`name` / `word` / `korean_term`
/// - 释义：`trans` / `translation`（字符串或数组）、`meaning`、`english_term`
///
/// `english_term` 看着像英文，其实是 HuggingFace 上那些「一词表一语言」文件里放
/// 目标语言释义的字段——`words.zh-TW.jsonl` 的 `english_term` 就是中文。
#[derive(serde::Deserialize)]
struct RawItem {
    #[serde(default)]
    name: String,
    #[serde(default)]
    word: String,
    #[serde(default)]
    korean_term: String,
    #[serde(default)]
    trans: Option<Trans>,
    #[serde(default)]
    translation: Option<Trans>,
    #[serde(default)]
    meaning: Option<String>,
    #[serde(default)]
    english_term: Option<String>,
}

#[derive(serde::Deserialize)]
#[serde(untagged)]
enum Trans {
    One(String),
    Many(Vec<String>),
}

impl Trans {
    fn joined(self) -> String {
        match self {
            Trans::One(s) => s,
            Trans::Many(v) => v.join("；"),
        }
    }
}

/// 解析 JSON 词书。先按 JSON 数组解析；失败再按 JSONL（每行一个对象）解析。
pub fn parse_json(text: &str) -> ImportReport {
    let mut report = ImportReport::default();
    let items = match parse_items(text) {
        Ok(v) => v,
        Err(e) => {
            report.skipped.push(format!("JSON 解析失败: {e}"));
            return report;
        }
    };

    let mut seen: Vec<String> = Vec::new();
    for (i, item) in items.into_iter().enumerate() {
        let surface = [
            item.name.as_str(),
            item.word.as_str(),
            item.korean_term.as_str(),
        ]
        .into_iter()
        .map(str::trim)
        .find(|s| !s.is_empty())
        .unwrap_or("")
        .to_string();
        if surface.is_empty() {
            continue;
        }
        let key = surface.to_lowercase();
        if seen.contains(&key) {
            report
                .skipped
                .push(format!("第 {} 条：重复的「{surface}」", i + 1));
            continue;
        }
        seen.push(key);

        let gloss = item
            .trans
            .or(item.translation)
            .map(Trans::joined)
            .or(item.meaning)
            .or(item.english_term)
            .map(|g| {
                g.split_whitespace()
                    .collect::<Vec<_>>()
                    .join(" ")
                    .trim()
                    .to_string()
            })
            .filter(|g| !g.is_empty());

        report.words.push(ImportedWord {
            surface,
            gloss,
            tags: Vec::new(),
        });
    }
    report
}

fn parse_items(text: &str) -> Result<Vec<RawItem>, serde_json::Error> {
    if let Ok(items) = serde_json::from_str::<Vec<RawItem>>(text) {
        return Ok(items);
    }
    let mut out = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        out.push(serde_json::from_str::<RawItem>(line)?);
    }
    Ok(out)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedArticle {
    pub title: String,
    pub body: String,
    pub questions: Vec<crate::article::Question>,
}

/// 解析用户粘贴的文章。
///
/// 约定：`#` 标题、`?` 题干、`-` 选项、`=` 正确选项序号（从 1 开始），
/// 其余非空行都是正文；空行分段。
pub fn parse_article(text: &str) -> Result<ParsedArticle, String> {
    use crate::article::Question;

    let mut title = String::new();
    let mut body: Vec<String> = Vec::new();
    let mut questions: Vec<Question> = Vec::new();
    let mut current: Option<Question> = None;

    for (i, raw) in text.lines().enumerate() {
        let line = raw.trim();
        let no = i + 1;
        if line.is_empty() {
            if body.last().map(|l| !l.is_empty()).unwrap_or(false) {
                body.push(String::new());
            }
            continue;
        }

        if let Some(t) = line.strip_prefix('#') {
            if title.is_empty() {
                title = t.trim().to_string();
            }
            continue;
        }

        if let Some(p) = line.strip_prefix('?') {
            if let Some(q) = current.take() {
                questions.push(finish_question(q, no)?);
            }
            current = Some(Question {
                prompt: p.trim().to_string(),
                options: Vec::new(),
                answer: usize::MAX,
                explain: None,
            });
            continue;
        }

        if let Some(o) = line.strip_prefix('-') {
            let Some(q) = current.as_mut() else {
                return Err(format!("第 {no} 行：选项要写在题干之后"));
            };
            q.options.push(o.trim().to_string());
            continue;
        }

        if let Some(a) = line.strip_prefix('=') {
            let idx = a
                .trim()
                .parse::<usize>()
                .map_err(|_| format!("第 {no} 行：答案写选项序号，例如 `= 2`"))?;
            let Some(q) = current.as_mut() else {
                return Err(format!("第 {no} 行：答案要写在题干之后"));
            };
            q.answer = idx;
            continue;
        }

        if current.is_some() {
            return Err(format!("第 {no} 行：题目之后只能跟 `-` 选项或 `=` 答案"));
        }
        body.push(line.to_string());
    }

    if let Some(q) = current.take() {
        questions.push(finish_question(q, text.lines().count())?);
    }

    while body.last().map(|l| l.is_empty()).unwrap_or(false) {
        body.pop();
    }
    if body.is_empty() {
        return Err("缺少正文".into());
    }
    if questions.is_empty() {
        return Err("至少要有一道题".into());
    }

    Ok(ParsedArticle {
        title: if title.is_empty() {
            "未命名文章".into()
        } else {
            title
        },
        body: body.join("\n"),
        questions,
    })
}

fn finish_question(
    q: crate::article::Question,
    line: usize,
) -> Result<crate::article::Question, String> {
    if q.prompt.trim().is_empty() {
        return Err(format!("第 {line} 行附近：题干为空"));
    }
    if q.options.len() < 2 {
        return Err(format!("「{}」至少要有两个选项", q.prompt));
    }
    if q.answer == usize::MAX || q.answer == 0 || q.answer > q.options.len() {
        return Err(format!(
            "「{}」的答案要写成 1–{} 之间的序号",
            q.prompt,
            q.options.len()
        ));
    }
    Ok(crate::article::Question {
        answer: q.answer - 1,
        ..q
    })
}

fn parse_lines(text: &str) -> ImportReport {
    let mut report = ImportReport::default();
    let mut seen: Vec<String> = Vec::new();

    for (lineno, raw) in text.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        let fields: Vec<&str> = if line.contains('\t') {
            line.split('\t').map(str::trim).collect()
        } else {
            line.split(',').map(str::trim).collect()
        };

        let surface = fields[0];
        if surface.is_empty() {
            report.skipped.push(format!("第 {} 行：词为空", lineno + 1));
            continue;
        }
        let key = surface.to_lowercase();
        if seen.contains(&key) {
            report
                .skipped
                .push(format!("第 {} 行：重复的「{surface}」", lineno + 1));
            continue;
        }
        seen.push(key);

        let gloss = fields
            .get(1)
            .filter(|g| !g.is_empty())
            .map(|g| g.to_string());
        let tags = fields
            .get(2)
            .map(|t| {
                t.split(|c| c == ',' || c == ' ')
                    .map(str::trim)
                    .filter(|t| !t.is_empty() && *t != "标签")
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default();

        report.words.push(ImportedWord {
            surface: surface.to_string(),
            gloss,
            tags,
        });
    }

    report
}

#[cfg(test)]
mod tests {
    use super::*;

    const QWERTY: &str = r#"[
      {"name":"cancel","trans":["取消， 撤销； 删去"],"usphone":"'kænsl"},
      {"name":"explosive","trans":["爆炸的","炸药"]},
      {"word":"vibe","translation":"氛围"},
      {"name":"cancel","trans":["重复"]},
      {"name":"","trans":["无词"]}
    ]"#;

    #[test]
    fn parses_qwerty_style_json() {
        let r = parse(QWERTY);
        assert_eq!(r.words.len(), 3);
        assert_eq!(r.words[0].surface, "cancel");
        assert_eq!(r.words[0].gloss.as_deref(), Some("取消， 撤销； 删去"));
        assert_eq!(r.words[1].gloss.as_deref(), Some("爆炸的；炸药"));
        assert_eq!(r.words[2].surface, "vibe");
        assert_eq!(r.words[2].gloss.as_deref(), Some("氛围"));
        // 重复的被记下来，空词的直接忽略
        assert_eq!(r.skipped.len(), 1);
        assert!(r.skipped[0].contains("重复"));
    }

    #[test]
    fn parses_meaning_only_wordlist() {
        // 韩语教材词表用 word + meaning，没有 trans / translation
        let text = r#"[
          {"word":"미국","meaning":"美国","partOfSpeech":"名词"},
          {"name":"은행","meaning":"银行"},
          {"word":"학교","meaning":""}
        ]"#;
        let r = parse(text);
        assert_eq!(r.words.len(), 3);
        assert_eq!(r.words[0].surface, "미국");
        assert_eq!(r.words[0].gloss.as_deref(), Some("美国"));
        assert_eq!(r.words[1].surface, "은행");
        assert_eq!(r.words[1].gloss.as_deref(), Some("银行"));
        assert!(r.words[2].gloss.is_none());
    }

    #[test]
    fn parses_jsonl_localized_wordlist() {
        // HuggingFace 词表是一行一个对象，释义在 english_term 里
        let text =
            "{\"korean_term\":\"사랑\",\"english_term\":\"愛\",\"romanization\":\"Sarang\"}\n\
                    {\"korean_term\":\"감사합니다\",\"english_term\":\"感謝\"}\n";
        let r = parse(text);
        assert_eq!(r.words.len(), 2);
        assert_eq!(r.words[0].surface, "사랑");
        assert_eq!(r.words[0].gloss.as_deref(), Some("愛"));
        assert_eq!(r.words[1].surface, "감사합니다");
    }

    #[test]
    fn detects_json_by_leading_bracket() {
        assert_eq!(parse(QWERTY).words.len(), 3);
        assert_eq!(parse("  \n[{\"name\":\"a\"}]").words.len(), 1);
    }

    #[test]
    fn broken_json_is_reported_not_panicking() {
        let r = parse("[{\"name\": }]");
        assert!(r.words.is_empty());
        assert_eq!(r.skipped.len(), 1);
    }

    #[test]
    fn parses_tsv_and_csv() {
        let text = "gonna\t将要，口语\nlowkey,偷偷地\n";
        let r = parse(text);
        assert_eq!(r.words.len(), 2);
        assert_eq!(r.words[0].surface, "gonna");
        // 逗号切分时第二段就是释义，不会把释义里的逗号再切
        assert_eq!(r.words[1].gloss.as_deref(), Some("偷偷地"));
    }

    #[test]
    fn tsv_keeps_commas_in_gloss() {
        let r = parse("bail\t放鸽子，中途退出\tslang");
        assert_eq!(r.words[0].gloss.as_deref(), Some("放鸽子，中途退出"));
        assert_eq!(r.words[0].tags, vec!["slang"]);
    }

    #[test]
    fn skips_blank_comments_and_duplicates() {
        let r = parse("\n# 注释\nGhost\nghost\n");
        assert_eq!(r.words.len(), 1);
        assert_eq!(r.skipped.len(), 1);
    }

    #[test]
    fn word_without_gloss_is_allowed() {
        let r = parse("vibe");
        assert_eq!(r.words.len(), 1);
        assert!(r.words[0].gloss.is_none());
    }

    #[test]
    fn empty_input() {
        assert!(parse("").words.is_empty());
        assert!(parse("   \n\n").words.is_empty());
    }

    #[test]
    fn parses_custom_article() {
        let text = "# 为什么我们玩消失\n\nGhosting used to be rude.\nNow it is normal.\n\n\
                    ? 人们不回消息的原因是什么？\n- 对话太沉重\n- 手机没电\n- 大家都变礼貌\n= 1\n\n\
                    ? 作者对 ghosting 的态度是？\n- 中立描述\n- 强烈反对\n= 1\n";
        let a = parse_article(text).unwrap();
        assert_eq!(a.title, "为什么我们玩消失");
        assert!(a.body.starts_with("Ghosting"));
        assert_eq!(a.questions.len(), 2);
        assert_eq!(a.questions[0].options.len(), 3);
        assert_eq!(a.questions[0].answer, 0);
        assert_eq!(a.questions[1].options.len(), 2);
    }

    #[test]
    fn article_answer_must_be_in_range() {
        let text = "# t\nbody\n? q\n- a\n- b\n= 3\n";
        assert!(parse_article(text).unwrap_err().contains("1–2"));
    }

    #[test]
    fn article_needs_body_and_question() {
        assert!(parse_article("? q\n- a\n- b\n= 1\n").is_err());
        assert!(parse_article("just body\n").is_err());
    }

    #[test]
    fn article_option_before_question_is_rejected() {
        assert!(parse_article("body\n- a\n").is_err());
    }
}
