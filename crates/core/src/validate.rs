use crate::article::Article;
use crate::dialog::Dialog;
use crate::entry::{Entry, Token};
use crate::pack::Pack;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warn,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Issue {
    pub severity: Severity,
    /// 出问题的对象，例如 `words[3] en.w.0004`。
    pub at: String,
    pub message: String,
}

impl Issue {
    pub fn error(at: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            severity: Severity::Error,
            at: at.into(),
            message: message.into(),
        }
    }
    pub fn warn(at: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            severity: Severity::Warn,
            at: at.into(),
            message: message.into(),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct Report {
    pub issues: Vec<Issue>,
}

impl Report {
    pub fn push(&mut self, issue: Issue) {
        self.issues.push(issue);
    }
    pub fn extend(&mut self, other: Report) {
        self.issues.extend(other.issues);
    }
    pub fn is_ok(&self) -> bool {
        !self.issues.iter().any(|i| i.severity == Severity::Error)
    }
    pub fn errors(&self) -> impl Iterator<Item = &Issue> {
        self.issues.iter().filter(|i| i.severity == Severity::Error)
    }
    pub fn warnings(&self) -> impl Iterator<Item = &Issue> {
        self.issues.iter().filter(|i| i.severity == Severity::Warn)
    }
}

/// 标注与原文的对齐校验。
///
/// 这是全项目最重要的一条校验：LLM 生成的标注经常出现「词对不上原文」，
/// 一旦放过就会变成错误记忆。规则是 tokens 必须首尾相接、铺满原文、
/// 每块内容与 `surface[start..end]` 逐字节相等。
pub fn check_coverage(at: &str, text: &str, tokens: &[Token]) -> Report {
    let mut report = Report::default();
    if tokens.is_empty() {
        report.push(Issue::warn(at, "没有任何 token 标注"));
        return report;
    }

    let mut cursor = 0usize;
    for (i, tok) in tokens.iter().enumerate() {
        let (start, end) = tok.span;
        if start > end || end > text.len() {
            report.push(Issue::error(
                at,
                format!(
                    "token[{i}] span {start}..{end} 越界（原文 {} 字节）",
                    text.len()
                ),
            ));
            return report;
        }
        if !text.is_char_boundary(start) || !text.is_char_boundary(end) {
            report.push(Issue::error(
                at,
                format!("token[{i}] span {start}..{end} 不在字符边界上"),
            ));
            return report;
        }
        if start < cursor {
            report.push(Issue::error(
                at,
                format!("token[{i}] 与前一块重叠于 {start}..{cursor}"),
            ));
            return report;
        }
        // 空隙只允许是空白：分词器不把空格当 token，但原文里的空格不能丢内容。
        if start > cursor && !text[cursor..start].trim().is_empty() {
            report.push(Issue::error(
                at,
                format!(
                    "token[{i}] 之前的 {cursor}..{start} 有未标注内容「{}」",
                    &text[cursor..start]
                ),
            ));
            return report;
        }
        let actual = &text[start..end];
        if actual != tok.surface {
            report.push(Issue::error(
                at,
                format!(
                    "token[{i}] 内容不一致：标注「{}」，原文「{actual}」",
                    tok.surface
                ),
            ));
        }
        if tok.surface.trim().is_empty() {
            report.push(Issue::warn(at, format!("token[{i}] 是空白块")));
        }
        cursor = end;
    }

    if !text[cursor..].trim().is_empty() {
        report.push(Issue::error(
            at,
            format!(
                "token 只覆盖到 {cursor}，尾部还有未标注内容「{}」",
                &text[cursor..]
            ),
        ));
    }
    report
}

fn check_entry(at: &str, entry: &Entry) -> Report {
    let mut report = Report::default();
    if entry.id.trim().is_empty() {
        report.push(Issue::error(at, "id 为空"));
    }
    if entry.surface.trim().is_empty() {
        report.push(Issue::error(at, "surface 为空"));
    }
    if !entry.tokens.is_empty() {
        report.extend(check_coverage(at, &entry.surface, &entry.tokens));
    }
    report
}

fn check_article(at: &str, article: &Article) -> Report {
    let mut report = Report::default();
    if article.questions.is_empty() {
        report.push(Issue::error(at, "文章没有题目"));
    }
    for (i, q) in article.questions.iter().enumerate() {
        if q.options.len() < 2 {
            report.push(Issue::error(at, format!("第 {i} 题选项少于 2 个")));
        }
        if q.answer >= q.options.len() {
            report.push(Issue::error(at, format!("第 {i} 题答案下标越界")));
        }
    }
    if !article.tokens.is_empty() {
        report.extend(check_coverage(at, &article.body, &article.tokens));
    } else {
        report.push(Issue::warn(at, "正文没有标注，点词查义不可用"));
    }
    report
}

fn check_dialog(at: &str, dialog: &Dialog) -> Report {
    let mut report = Report::default();
    if !dialog.is_well_formed() {
        report.push(Issue::error(
            at,
            "对话图结构不合法（起始节点缺失、跳转断裂或存在环）",
        ));
    }
    for node in &dialog.nodes {
        if !node.tokens.is_empty() {
            report.extend(check_coverage(at, &node.text, &node.tokens));
        }
    }
    report
}

/// 整包校验：id 唯一、标注对齐、题目合法、对话图合法、素材存在、指纹匹配。
pub fn validate_pack(pack: &Pack) -> Report {
    let mut report = Report::default();
    let mut seen: Vec<&str> = Vec::new();

    for (i, entry) in pack.words.iter().chain(pack.sentences.iter()).enumerate() {
        let at = format!("entries[{i}] {}", entry.id);
        report.extend(check_entry(&at, entry));
        if seen.contains(&entry.id.as_str()) {
            report.push(Issue::error(at.clone(), "id 重复"));
        } else {
            seen.push(&entry.id);
        }
        for asset in entry.audio.as_deref().into_iter() {
            if !pack.asset(asset).exists() {
                report.push(Issue::error(at.clone(), format!("素材缺失: {asset}")));
            }
        }
    }

    for article in &pack.articles {
        let at = format!("article {}", article.id);
        report.extend(check_article(&at, article));
        if seen.contains(&article.id.as_str()) {
            report.push(Issue::error(at, "id 与词条重复"));
        } else {
            seen.push(&article.id);
        }
    }

    for dialog in &pack.dialogs {
        let at = format!("dialog {}", dialog.id);
        report.extend(check_dialog(&at, dialog));
        if seen.contains(&dialog.id.as_str()) {
            report.push(Issue::error(at.clone(), "id 重复"));
        } else {
            seen.push(&dialog.id);
        }
    }

    if let Some(expected) = &pack.manifest.content_hash {
        match pack.content_hash() {
            Ok(actual) if &actual == expected => {}
            Ok(actual) => report.push(Issue::error(
                "manifest",
                format!("内容指纹不符：manifest 记 {expected}，实际 {actual}"),
            )),
            Err(e) => report.push(Issue::error("manifest", format!("计算指纹失败: {e}"))),
        }
    }

    report
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entry::Token;

    fn tok(s: &str, span: (usize, usize)) -> Token {
        Token::new(s, span)
    }

    #[test]
    fn perfect_coverage_passes() {
        let text = "going to bed";
        let tokens = vec![
            tok("going", (0, 5)),
            tok(" to ", (5, 9)),
            tok("bed", (9, 12)),
        ];
        assert!(check_coverage("t", text, &tokens).is_ok());
    }

    #[test]
    fn gap_is_error() {
        let text = "going to bed";
        let tokens = vec![tok("going", (0, 5)), tok("bed", (9, 12))];
        let r = check_coverage("t", text, &tokens);
        assert!(!r.is_ok());
        assert!(r.errors().any(|i| i.message.contains("未标注内容")));
    }

    #[test]
    fn overlap_is_error() {
        let text = "abcdef";
        let tokens = vec![tok("abcd", (0, 4)), tok("cdef", (2, 6))];
        assert!(!check_coverage("t", text, &tokens).is_ok());
    }

    #[test]
    fn mismatch_is_error() {
        let text = "한국어";
        // 标注文字与 span 指向的原文不一致（한굴 vs 한국）
        let tokens = vec![tok("한굴", (0, 6)), tok("어", (6, 9))];
        let r = check_coverage("t", text, &tokens);
        assert!(!r.is_ok());
        assert!(r.errors().any(|i| i.message.contains("内容不一致")));
    }

    #[test]
    fn trailing_shortfall_is_error() {
        let text = "abc def";
        let tokens = vec![tok("abc", (0, 3))];
        assert!(!check_coverage("t", text, &tokens).is_ok());
    }

    #[test]
    fn non_boundary_is_error() {
        let text = "한국어";
        let tokens = vec![tok("한", (0, 1)), tok("국어", (1, 9))];
        assert!(!check_coverage("t", text, &tokens).is_ok());
    }
}
