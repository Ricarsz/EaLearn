use std::borrow::Cow;
use std::sync::OnceLock;

use ealearn_core::entry::Token;
use lindera::dictionary::load_dictionary;
use lindera::mode::Mode;
use lindera::segmenter::Segmenter;

use crate::{LanguageProfile, TagDef, VoiceSpec};

/// 韩语。
///
/// 切词用 `lindera` + mecab-ko-dic 做形态素分析：`밥을` → `밥`(NNG) + `을`(JKO)。
/// ko-dic 自带 mecab-ko-dic 的左空格惩罚规则，开启后切分才贴合实际书写空格，
/// 不加这项会把 `아버지가` 和 `방에` 连带切错。
pub struct Korean {
    segmenter: Segmenter,
}

static KOREAN: OnceLock<Korean> = OnceLock::new();

pub fn korean() -> &'static Korean {
    KOREAN.get_or_init(|| Korean::new().expect("内嵌 ko-dic 词典加载失败"))
}

impl Korean {
    pub fn new() -> Result<Self, String> {
        let dictionary = load_dictionary("embedded://ko-dic").map_err(|e| e.to_string())?;
        let base = Segmenter::new(Mode::Normal, dictionary, None);
        // 规则缺失时退回不带惩罚的分词器，不要因为这一步让整个语言不可用。
        let segmenter = base.space_penalty_from_dictionary().unwrap_or_else(|e| {
            eprintln!("ko-dic 空格惩罚规则不可用，已退回默认切分: {e}");
            Segmenter::new(
                Mode::Normal,
                load_dictionary("embedded://ko-dic").expect("ko-dic 重载失败"),
                None,
            )
        });
        Ok(Self { segmenter })
    }

    fn segment(&self, text: &str) -> Vec<Token> {
        let raw = match self.segmenter.segment(Cow::Borrowed(text)) {
            Ok(tokens) => tokens,
            Err(e) => {
                eprintln!("韩语分词失败，退回按空格切分: {e}");
                return fallback_split(text);
            }
        };

        let mut out = Vec::with_capacity(raw.len());
        for mut t in raw {
            let surface = t.surface.to_string();
            let span = (t.byte_start, t.byte_end);

            let mut pos: Option<String> = None;
            for key in ["part_of_speech_tag", "part_of_speech", "major_pos", "pos"] {
                if let Some(v) = t.get(key) {
                    if !v.is_empty() && v != "*" {
                        pos = Some(v.to_string());
                        break;
                    }
                }
            }

            let mut lemma: Option<String> = None;
            if let Some(base) = t.get("base_form") {
                if !base.is_empty() && base != "*" && base != surface {
                    lemma = Some(base.to_string());
                }
            }
            // 活用形不拆 token，但 expression 里记着「가/VV/*+ᄇ니다/EC/*」，
            // 取第一段作为词干，SRS 才能把 갑니다 / 갔어요 归到同一个词。
            if lemma.is_none() {
                if let Some(expr) = t.get("expression") {
                    if expr != "*" {
                        if let Some(stem) = expr
                            .split('+')
                            .next()
                            .and_then(|part| part.split('/').next())
                        {
                            if !stem.is_empty() && stem != surface {
                                lemma = Some(stem.to_string());
                            }
                        }
                    }
                }
            }

            let mut token = Token::new(surface.clone(), span);
            token.pos = pos;
            token.lemma = lemma;
            token.reading = self.reading(&surface);
            out.push(token);
        }

        // 空白之外的空隙由校验器兜底，这里不做补齐。
        out
    }
}

impl LanguageProfile for Korean {
    fn id(&self) -> &'static str {
        "ko"
    }

    fn tokenize(&self, text: &str) -> Vec<Token> {
        self.segment(text)
    }

    fn lemmatize(&self, surface: &str) -> Option<String> {
        let tokens = self.segment(surface);
        let first = tokens.first()?;
        Some(first.lemma.clone().unwrap_or_else(|| first.surface.clone()))
    }

    fn reading(&self, text: &str) -> Option<String> {
        Some(romanize(text))
    }

    fn voice(&self) -> VoiceSpec {
        VoiceSpec {
            id: "ko_KR-neutral",
            lang: "ko-KR",
            sample_rate: 22_050,
        }
    }

    fn stopwords(&self) -> &'static [&'static str] {
        &[
            "은",
            "는",
            "이",
            "가",
            "을",
            "를",
            "에",
            "의",
            "도",
            "로",
            "으로",
            "와",
            "과",
            "에서",
            "에게",
            "한테",
            "부터",
            "까지",
            "만",
            "보다",
            "처럼",
            "하고",
            "그리고",
            "그런데",
            "하다",
            "있다",
            "없다",
            "되다",
            "것",
            "수",
            "등",
            "좀",
            "그",
            "저",
            "이",
            "네",
            "응",
            "어",
            "아",
            "음",
        ]
    }

    fn grammar_tags(&self) -> &'static [TagDef] {
        &[
            TagDef {
                tag: "NNG",
                desc: "一般名词",
            },
            TagDef {
                tag: "NNP",
                desc: "专有名词",
            },
            TagDef {
                tag: "JKS",
                desc: "主格助词",
            },
            TagDef {
                tag: "JKO",
                desc: "宾格助词",
            },
            TagDef {
                tag: "JKM",
                desc: "副词格助词",
            },
            TagDef {
                tag: "VV",
                desc: "动词",
            },
            TagDef {
                tag: "VA",
                desc: "形容词",
            },
            TagDef {
                tag: "VCP",
                desc: "系词（이다）",
            },
            TagDef {
                tag: "EP",
                desc: "先语末语尾",
            },
            TagDef {
                tag: "EF",
                desc: "终结语尾",
            },
            TagDef {
                tag: "EC",
                desc: "连接语尾",
            },
            TagDef {
                tag: "ETM",
                desc: "冠形语尾",
            },
            TagDef {
                tag: "MAG",
                desc: "一般副词",
            },
            TagDef {
                tag: "IC",
                desc: "感叹词",
            },
            TagDef {
                tag: "XSN",
                desc: "名词派生后缀",
            },
            TagDef {
                tag: "SL",
                desc: "外语",
            },
        ]
    }
}

fn fallback_split(text: &str) -> Vec<Token> {
    let mut out = Vec::new();
    let mut cursor = 0usize;
    for word in text.split_whitespace() {
        let start = text[cursor..]
            .find(word)
            .map(|i| cursor + i)
            .unwrap_or(cursor);
        let end = start + word.len();
        out.push(Token::new(word, (start, end)));
        cursor = end;
    }
    out
}

// ── 罗马字（국어의 로마자 표기법）─────────────────────────────────────────

const INITIAL: [&str; 19] = [
    "g", "kk", "n", "d", "tt", "r", "m", "b", "pp", "s", "ss", "", "j", "jj", "ch", "k", "t", "p",
    "h",
];

const MEDIAL: [&str; 21] = [
    "a", "ae", "ya", "yae", "eo", "e", "yeo", "ye", "o", "wa", "wae", "oe", "yo", "u", "wo", "we",
    "wi", "yu", "eu", "ui", "i",
];

const FINAL: [&str; 28] = [
    "", "k", "k", "k", "n", "n", "n", "t", "l", "k", "m", "l", "l", "l", "p", "l", "m", "p", "p",
    "t", "t", "ng", "t", "t", "k", "t", "p", "t",
];

/// 받침 移到后一音节初声时的写法。
const FINAL_MOVED: [&str; 28] = [
    "", "g", "kk", "gs", "n", "nj", "n", "d", "r", "lg", "rm", "rb", "rs", "rt", "rp", "r", "m",
    "b", "bs", "s", "ss", "ng", "j", "ch", "k", "t", "p", "h",
];

/// 韩语音节分解结果：(初声, 中声, 终声) 的下标。
fn decompose(ch: char) -> Option<(usize, usize, usize)> {
    let code = ch as u32;
    if !(0xAC00..=0xD7A3).contains(&code) {
        return None;
    }
    let idx = code - 0xAC00;
    Some((
        (idx / 588) as usize,
        ((idx % 588) / 28) as usize,
        (idx % 28) as usize,
    ))
}

/// 罗马字转写。
///
/// 已实现：音节分解、연음（받침 移到后一音节初声 ㅇ）、ㅎ 收音在元音前脱落。
/// 未实现：비음화 / 유음화 / 구개음화 等音变（감사합니다 会得到 gamsahapnida
/// 而不是 gamsahamnida）。补这些规则时同时更新本函数的测试。
pub fn romanize(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let parts: Vec<Option<(usize, usize, usize)>> = chars.iter().map(|c| decompose(*c)).collect();
    let mut absorbed = vec![false; chars.len()];
    let mut carried: Vec<&'static str> = vec![""; chars.len()];

    for i in 0..chars.len().saturating_sub(1) {
        if let (Some((_, _, fin)), Some((next_ini, _, _))) = (parts[i], parts[i + 1]) {
            if fin != 0 && next_ini == 11 {
                // ㅎ 收音在元音前不发音；其余收音连读过去
                carried[i + 1] = if fin == 27 { "" } else { FINAL_MOVED[fin] };
                absorbed[i] = true;
            }
        }
    }

    let mut out = String::new();
    for (i, ch) in chars.iter().enumerate() {
        let Some((ini, med, fin)) = parts[i] else {
            out.push(*ch);
            continue;
        };
        out.push_str(carried[i]);
        out.push_str(INITIAL[ini]);
        out.push_str(MEDIAL[med]);
        if !absorbed[i] {
            out.push_str(FINAL[fin]);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use ealearn_core::validate::check_coverage;

    #[test]
    fn morphology_is_split() {
        let tokens = korean().tokenize("밥을 먹었다");
        let surfaces: Vec<&str> = tokens.iter().map(|t| t.surface.as_str()).collect();
        assert!(surfaces.contains(&"밥"), "没切出名词: {surfaces:?}");
        assert!(surfaces.contains(&"을"), "没切出宾格助词: {surfaces:?}");
        assert!(surfaces.len() >= 4, "形态素切分粒度不够: {surfaces:?}");
    }

    #[test]
    fn tokens_cover_text() {
        let text = "오늘 진짜 짜증나, 걔가 또 약속을 깼어.";
        let tokens = korean().tokenize(text);
        let report = check_coverage("t", text, &tokens);
        assert!(report.is_ok(), "{:?}", report.errors().collect::<Vec<_>>());
    }

    #[test]
    fn pos_tags_present() {
        let tokens = korean().tokenize("학교에 갑니다");
        assert!(
            tokens.iter().any(|t| t.pos.as_deref() == Some("NNG")),
            "没有 NNG 标注: {:?}",
            tokens.iter().map(|t| t.pos.clone()).collect::<Vec<_>>()
        );
    }

    #[test]
    fn inflected_verb_shares_stem() {
        // 갑니다 在 ko-dic 里是单个 token（活用不拆），但要能还原到词干 가
        let tokens = korean().tokenize("갑니다");
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].lemma.as_deref(), Some("가"));
        assert_eq!(tokens[0].pos.as_deref(), Some("VV+EC"));
    }

    #[test]
    fn romanization_basic() {
        assert_eq!(romanize("한국어"), "hangugeo");
        assert_eq!(romanize("밥을"), "babeul");
        assert_eq!(romanize("서울"), "seoul");
        assert_eq!(romanize("안녕하세요"), "annyeonghaseyo");
    }

    #[test]
    fn romanization_passes_through_non_hangul() {
        assert_eq!(romanize("ok 좋아"), "ok joa");
    }
}
