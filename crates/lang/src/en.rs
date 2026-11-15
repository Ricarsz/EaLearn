use ealearn_core::entry::Token;

use crate::{LanguageProfile, TagDef, VoiceSpec};

/// 英语。
///
/// 切词按「字母串 + 内部撇号/连字符」聚合，其余非空白字符各成一个标点 token。
/// 不把空格当 token——校验器允许空白空隙。
pub struct English;

impl LanguageProfile for English {
    fn id(&self) -> &'static str {
        "en"
    }

    fn tokenize(&self, text: &str) -> Vec<Token> {
        let chars: Vec<(usize, char)> = text.char_indices().collect();
        let mut out = Vec::new();
        let mut i = 0usize;

        while i < chars.len() {
            let (start, ch) = chars[i];
            if ch.is_whitespace() {
                i += 1;
                continue;
            }
            if ch.is_alphanumeric() {
                let mut j = i;
                let mut end = start + ch.len_utf8();
                while j + 1 < chars.len() {
                    let (_, c) = chars[j + 1];
                    let width = c.len_utf8();
                    if c.is_alphanumeric() {
                        j += 1;
                        end += width;
                    } else if matches!(c, '\'' | '\u{2019}' | '-')
                        && j + 2 < chars.len()
                        && chars[j + 2].1.is_alphanumeric()
                    {
                        j += 1;
                        end += width;
                    } else {
                        break;
                    }
                }
                let surface = &text[start..end];
                let mut token = Token::new(surface, (start, end));
                let normalized = surface.replace('\u{2019}', "'");
                token.lemma = self.lemmatize(&normalized);
                out.push(token);
                i = j + 1;
            } else {
                let end = start + ch.len_utf8();
                out.push(Token::new(ch.to_string(), (start, end)));
                i += 1;
            }
        }
        out
    }

    /// 高频不规则表优先，其余按后缀规则还原。宁可还原失败（返回原词），
    /// 也不要把词根猜错——猜错会污染 SRS 的词条键。
    fn lemmatize(&self, surface: &str) -> Option<String> {
        let lower = surface.to_lowercase();
        if let Some(base) = irregular(&lower) {
            return Some(base.to_string());
        }
        Some(strip_suffix(&lower))
    }

    /// 英语读音（IPA）由 pack 提供（词典或生成阶段写入），这里不做算法推导。
    fn reading(&self, _text: &str) -> Option<String> {
        None
    }

    fn voice(&self) -> VoiceSpec {
        VoiceSpec {
            id: "en_US-neutral",
            lang: "en-US",
            sample_rate: 22_050,
        }
    }

    fn stopwords(&self) -> &'static [&'static str] {
        &[
            "a", "an", "the", "and", "or", "but", "if", "of", "at", "by", "for", "with", "about",
            "to", "from", "in", "on", "into", "over", "after", "is", "am", "are", "was", "were",
            "be", "been", "being", "do", "does", "did", "have", "has", "had", "i", "you", "he",
            "she", "it", "we", "they", "me", "him", "her", "us", "them", "my", "your", "his",
            "its", "our", "their", "this", "that", "these", "those", "so", "just", "very", "not",
            "no", "yes", "ok", "okay", "well", "there", "here", "what", "which", "who", "when",
            "where", "why", "how",
        ]
    }

    fn grammar_tags(&self) -> &'static [TagDef] {
        &[
            TagDef {
                tag: "noun",
                desc: "名词",
            },
            TagDef {
                tag: "verb",
                desc: "动词",
            },
            TagDef {
                tag: "adj",
                desc: "形容词",
            },
            TagDef {
                tag: "adv",
                desc: "副词",
            },
            TagDef {
                tag: "pron",
                desc: "代词",
            },
            TagDef {
                tag: "prep",
                desc: "介词",
            },
            TagDef {
                tag: "det",
                desc: "限定词",
            },
            TagDef {
                tag: "conj",
                desc: "连词",
            },
            TagDef {
                tag: "interjection",
                desc: "感叹词",
            },
            TagDef {
                tag: "particle",
                desc: "小品词",
            },
            TagDef {
                tag: "contraction",
                desc: "缩略形式",
            },
        ]
    }
}

fn irregular(word: &str) -> Option<&'static str> {
    Some(match word {
        "am" | "is" | "are" | "was" | "were" | "been" | "being" => "be",
        "has" | "had" | "having" => "have",
        "does" | "did" | "done" | "doing" => "do",
        "went" | "gone" => "go",
        "said" => "say",
        "made" => "make",
        "took" | "taken" => "take",
        "came" => "come",
        "got" | "gotten" => "get",
        "saw" | "seen" => "see",
        "knew" | "known" => "know",
        "thought" => "think",
        "told" => "tell",
        "became" => "become",
        "left" => "leave",
        "felt" => "feel",
        "brought" => "bring",
        "began" | "begun" => "begin",
        "kept" => "keep",
        "held" => "hold",
        "wrote" | "written" => "write",
        "stood" => "stand",
        "heard" => "hear",
        "let" => "let",
        "meant" => "mean",
        "set" => "set",
        "met" => "meet",
        "ran" => "run",
        "paid" => "pay",
        "sat" => "sit",
        "spoke" | "spoken" => "speak",
        "lay" | "lain" => "lie",
        "led" => "lead",
        "grew" | "grown" => "grow",
        "lost" => "lose",
        "fell" | "fallen" => "fall",
        "sent" => "send",
        "built" => "build",
        "understood" => "understand",
        "drew" | "drawn" => "draw",
        "broke" | "broken" => "break",
        "spent" => "spend",
        "cut" => "cut",
        "rose" | "risen" => "rise",
        "drove" | "driven" => "drive",
        "bought" => "buy",
        "wore" | "worn" => "wear",
        "chose" | "chosen" => "choose",
        "ate" | "eaten" => "eat",
        "better" | "best" => "good",
        "worse" | "worst" => "bad",
        "children" => "child",
        "men" => "man",
        "women" => "woman",
        "people" => "person",
        "feet" => "foot",
        "teeth" => "tooth",
        "mice" => "mouse",
        _ => return None,
    })
}

fn strip_suffix(word: &str) -> String {
    // 缩写先还原，避免被后缀规则切坏（don't → do not 太激进，只处理 's / n't）
    if let Some(stem) = word.strip_suffix("n't") {
        return format!("{stem} not");
    }
    if let Some(stem) = word.strip_suffix("'s").or_else(|| word.strip_suffix("s'")) {
        return stem.to_string();
    }

    if let Some(stem) = word.strip_suffix("ies") {
        if stem.len() >= 2 {
            return format!("{stem}y");
        }
    }
    if let Some(stem) = word.strip_suffix("ing") {
        if stem.len() >= 3 {
            return undouble(stem);
        }
    }
    if let Some(stem) = word.strip_suffix("ed") {
        if stem.len() >= 3 {
            return undouble(stem);
        }
    }
    if let Some(stem) = word.strip_suffix("es") {
        let ends_sibilant = ["s", "x", "z", "ch", "sh"]
            .iter()
            .any(|s| stem.ends_with(s));
        if stem.len() >= 3 && ends_sibilant {
            return stem.to_string();
        }
    }
    if let Some(stem) = word.strip_suffix("s") {
        if stem.len() >= 3 && !stem.ends_with('s') {
            return stem.to_string();
        }
    }
    word.to_string()
}

/// stopping → stop，running → run。
fn undouble(stem: &str) -> String {
    let chars: Vec<char> = stem.chars().collect();
    if chars.len() >= 3 {
        let n = chars.len();
        if chars[n - 1] == chars[n - 2]
            && matches!(chars[n - 1], 'b' | 'd' | 'g' | 'm' | 'n' | 'p' | 'r' | 't')
        {
            return chars[..n - 1].iter().collect();
        }
    }
    stem.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use ealearn_core::validate::check_coverage;

    #[test]
    fn tokens_cover_text() {
        let text = "I'm kind of gonna bail on the party, tbh.";
        let tokens = English.tokenize(text);
        let report = check_coverage("t", text, &tokens);
        assert!(report.is_ok(), "{:?}", report.errors().collect::<Vec<_>>());
    }

    #[test]
    fn apostrophe_stays_inside_word() {
        let tokens = English.tokenize("don't");
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].surface, "don't");
        assert_eq!(tokens[0].lemma.as_deref(), Some("do not"));
    }

    #[test]
    fn hyphenated_word_is_one_token() {
        let tokens = English.tokenize("well-known");
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].span, (0, 10));
    }

    #[test]
    fn punctuation_is_split_out() {
        let tokens = English.tokenize("Party?");
        assert_eq!(tokens.len(), 2);
        assert_eq!(tokens[1].surface, "?");
    }

    #[test]
    fn irregular_and_suffix_lemmas() {
        assert_eq!(English.lemmatize("went").as_deref(), Some("go"));
        assert_eq!(English.lemmatize("stories").as_deref(), Some("story"));
        assert_eq!(English.lemmatize("running").as_deref(), Some("run"));
        assert_eq!(English.lemmatize("watched").as_deref(), Some("watch"));
        assert_eq!(English.lemmatize("parties").as_deref(), Some("party"));
        // 不认识的词原样返回，不硬猜
        assert_eq!(English.lemmatize("gonna").as_deref(), Some("gonna"));
    }

    #[test]
    fn non_ascii_text_does_not_panic() {
        let text = "café — 3.5€";
        let tokens = English.tokenize(text);
        assert!(check_coverage("t", text, &tokens).is_ok());
    }
}
