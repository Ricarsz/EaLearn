//! 边到边抽出 `reply` 字段。
//!
//! 模型被要求先输出 `{"reply": "……", "annotations": [...]}`，因此在 JSON 还没
//! 闭合时就能把 `reply` 的字符串内容一个个字符地喂给界面——用户看到的是对话
//! 正在打字，而不是等整段 JSON 生成完再一起出现。
//!
//! 这里不追求通用 JSON 解析，只处理「找 `"reply"` 键 + 解 JSON 字符串转义」。

/// 从可能不完整的 JSON 文本里取出 `reply` 值当前已可确定的部分。
///
/// 三种情况返回 `None`：还没读到键、值不是字符串、转义序列还没收齐。
pub fn extract_reply_prefix(text: &str) -> Option<String> {
    let key_at = text.find("\"reply\"")?;
    let mut rest = &text[key_at + "\"reply\"".len()..];
    rest = rest.trim_start();
    rest = rest.strip_prefix(':')?;
    rest = rest.trim_start();
    rest = rest.strip_prefix('"')?;

    let mut out = String::new();
    let mut chars = rest.char_indices();
    while let Some((i, ch)) = chars.next() {
        match ch {
            '"' => return Some(out),
            '\\' => {
                let Some((_, esc)) = chars.next() else {
                    // 反斜杠刚出现，转义还没到达
                    return Some(out);
                };
                match esc {
                    '"' => out.push('"'),
                    '\\' => out.push('\\'),
                    '/' => out.push('/'),
                    'b' => out.push('\u{8}'),
                    'f' => out.push('\u{c}'),
                    'n' => out.push('\n'),
                    'r' => out.push('\r'),
                    't' => out.push('\t'),
                    'u' => {
                        let start = i + 2;
                        if rest.len() < start + 4 {
                            return Some(out);
                        }
                        let hex = &rest[start..start + 4];
                        let Ok(code) = u32::from_str_radix(hex, 16) else {
                            return Some(out);
                        };
                        // 跳过 4 个十六进制数字
                        for _ in 0..4 {
                            chars.next();
                        }
                        // 处理代理对
                        if (0xD800..0xDC00).contains(&code) {
                            let mut probe = rest[start + 4..].chars();
                            if probe.next() == Some('\\') && probe.next() == Some('u') {
                                let rest2 = &rest[start + 4..];
                                if rest2.len() >= 6 {
                                    if let Ok(low) = u32::from_str_radix(&rest2[2..6], 16) {
                                        if (0xDC00..0xE000).contains(&low) {
                                            for _ in 0..6 {
                                                chars.next();
                                            }
                                            let combined =
                                                0x10000 + ((code - 0xD800) << 10) + (low - 0xDC00);
                                            if let Some(c) = char::from_u32(combined) {
                                                out.push(c);
                                            }
                                            continue;
                                        }
                                    }
                                }
                            }
                            return Some(out);
                        }
                        if let Some(c) = char::from_u32(code) {
                            out.push(c);
                        }
                    }
                    other => out.push(other),
                }
            }
            '\n' if out.is_empty() => {
                // 键后紧跟换行说明不是正常字符串，放弃
                return None;
            }
            c => out.push(c),
        }
    }
    Some(out)
}

/// 记录已经推给界面的部分，每次拿到新增的尾巴。
#[derive(Debug, Default)]
pub struct ReplyTracker {
    emitted: String,
}

impl ReplyTracker {
    /// 传入「截至目前累积的全文」，返回可以继续显示的增量。
    pub fn on_text(&mut self, accumulated: &str) -> Option<String> {
        let full = extract_reply_prefix(accumulated)?;
        if full.len() <= self.emitted.len() || !full.starts_with(&self.emitted) {
            return None;
        }
        let delta = full[self.emitted.len()..].to_string();
        self.emitted = full;
        Some(delta)
    }

    pub fn text(&self) -> &str {
        &self.emitted
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_plain_value() {
        assert_eq!(
            extract_reply_prefix(r#"{"reply":"hello"}"#).as_deref(),
            Some("hello")
        );
    }

    #[test]
    fn none_before_key() {
        assert_eq!(extract_reply_prefix(r#"{"translation":"#), None);
    }

    #[test]
    fn handles_partial_value() {
        assert_eq!(
            extract_reply_prefix(r#"{"reply":"he"#).as_deref(),
            Some("he")
        );
    }

    #[test]
    fn decodes_escapes() {
        let raw = r#"{"reply":"she said \"ok\"\nand left"}"#;
        assert_eq!(
            extract_reply_prefix(raw).as_deref(),
            Some("she said \"ok\"\nand left")
        );
    }

    #[test]
    fn decodes_unicode_escape() {
        assert_eq!(
            extract_reply_prefix(r#"{"reply":"\u4f60\u597d"}"#).as_deref(),
            Some("你好")
        );
    }

    #[test]
    fn waits_for_complete_unicode_escape() {
        assert_eq!(
            extract_reply_prefix(r#"{"reply":"a\u4f6"#).as_deref(),
            Some("a")
        );
    }

    #[test]
    fn waits_for_trailing_backslash() {
        assert_eq!(
            extract_reply_prefix(r#"{"reply":"a\"#).as_deref(),
            Some("a")
        );
    }

    #[test]
    fn tracker_emits_only_new_suffix() {
        let mut t = ReplyTracker::default();
        assert_eq!(t.on_text(r#"{"reply":"he"#).as_deref(), Some("he"));
        assert_eq!(t.on_text(r#"{"reply":"hello"#).as_deref(), Some("llo"));
        assert_eq!(t.on_text(r#"{"reply":"hello"#), None);
        assert_eq!(t.text(), "hello");
    }

    #[test]
    fn tracker_handles_chinese_text() {
        let mut t = ReplyTracker::default();
        let mut acc = String::from(r#"{"reply":""#);
        for ch in "我先走了".chars() {
            acc.push(ch);
            // 每个字符到达时都应能取出
            let _ = t.on_text(&acc);
        }
        assert_eq!(t.text(), "我先走了");
    }
}
