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

pub fn parse(text: &str) -> ImportReport {
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
