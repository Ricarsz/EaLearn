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

/// 解析 JSON 词书。兼容 `name`/`word` 两种字段名，释义支持字符串或字符串数组。
pub fn parse_json(text: &str) -> ImportReport {
    #[derive(serde::Deserialize)]
    struct Item {
        #[serde(default)]
        name: String,
        #[serde(default)]
        word: String,
        #[serde(default)]
        trans: Option<Trans>,
        #[serde(default)]
        translation: Option<Trans>,
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

    let mut report = ImportReport::default();
    let items: Vec<Item> = match serde_json::from_str(text) {
        Ok(v) => v,
        Err(e) => {
            report.skipped.push(format!("JSON 解析失败: {e}"));
            return report;
        }
    };

    let mut seen: Vec<String> = Vec::new();
    for (i, item) in items.into_iter().enumerate() {
        let surface = if item.name.trim().is_empty() {
            item.word.trim().to_string()
        } else {
            item.name.trim().to_string()
        };
        if surface.is_empty() {
            continue;
        }
        let key = surface.to_lowercase();
        if seen.contains(&key) {
            report.skipped.push(format!("第 {} 条：重复的「{surface}」", i + 1));
            continue;
        }
        seen.push(key);

        let gloss = item
            .trans
            .or(item.translation)
            .map(Trans::joined)
            .map(|g| g.split_whitespace().collect::<Vec<_>>().join(" ").trim().to_string())
            .filter(|g| !g.is_empty());

        report.words.push(ImportedWord { surface, gloss, tags: Vec::new() });
    }
    report
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
            report.skipped.push(format!("第 {} 行：重复的「{surface}」", lineno + 1));
            continue;
        }
        seen.push(key);

        let gloss = fields.get(1).filter(|g| !g.is_empty()).map(|g| g.to_string());
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

        report.words.push(ImportedWord { surface: surface.to_string(), gloss, tags });
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
}
