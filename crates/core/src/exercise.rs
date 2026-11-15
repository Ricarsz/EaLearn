use std::fmt;

use serde::{Deserialize, Serialize};

use crate::entry::Entry;

/// 题型定义。题型是「注册项」：core 提供判分器，前端提供渲染组件，
/// 两边各加一行就能上新题型，不改核心。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum Exercise {
    /// 拼句：把打乱的词块按正确顺序排列。
    BuildSentence {
        entry: String,
        /// 展示给用户的词块（已打乱）。
        chunks: Vec<String>,
        /// 第 i 个位置应填的 `chunks` 下标。
        answer: Vec<usize>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        prompt: Option<String>,
    },
    /// 听音选词。
    WordFromAudio {
        entry: String,
        audio: String,
        options: Vec<String>,
        answer: usize,
    },
    /// 文章阅读理解。
    ArticleMcq {
        article: String,
        question: usize,
        prompt: String,
        options: Vec<String>,
        answer: usize,
    },
    /// 背诵：正面出词、背面出义，由用户自评熟练度。
    /// 自定义单词没有音频/图片素材，靠它才能进练习。
    Recall {
        entry: String,
        surface: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        gloss: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reading: Option<String>,
    },
    /// 情景对话：判分靠对话图分支推进，不走单题判分器。
    ChatScenario { dialog: String },
}

/// 用户作答。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum Answer {
    /// 拼句提交的顺序（`chunks` 下标序列）。
    Order { order: Vec<usize> },
    /// 选项下标。
    Choice { index: usize },
    /// 背诵自评：1 不认识 / 2 模糊 / 3 认识 / 4 太简单
    Recall { rating: u8 },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Grade {
    pub correct: bool,
    /// 0.0–1.0 的得分率，拼句支持部分得分。
    pub ratio: f32,
    pub items: Vec<GradeItem>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GradeItem {
    pub index: usize,
    pub correct: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GradeError {
    WrongAnswerShape,
    NotGradable,
}

impl fmt::Display for GradeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GradeError::WrongAnswerShape => write!(f, "作答形状与题型不匹配"),
            GradeError::NotGradable => write!(f, "该题型不走单题判分（由对话图推进）"),
        }
    }
}

impl std::error::Error for GradeError {}

impl Exercise {
    pub fn kind(&self) -> &'static str {
        match self {
            Exercise::BuildSentence { .. } => "build-sentence",
            Exercise::WordFromAudio { .. } => "word-from-audio",
            Exercise::ArticleMcq { .. } => "article-mcq",
            Exercise::Recall { .. } => "recall",
            Exercise::ChatScenario { .. } => "chat-scenario",
        }
    }

    /// 练习页需要的素材引用，便于前端一次取齐。
    pub fn entry_id(&self) -> Option<&str> {
        match self {
            Exercise::BuildSentence { entry, .. }
            | Exercise::WordFromAudio { entry, .. }
            | Exercise::Recall { entry, .. } => Some(entry),
            _ => None,
        }
    }

    pub fn grade(&self, answer: &Answer) -> Result<Grade, GradeError> {
        match (self, answer) {
            (
                Exercise::BuildSentence {
                    chunks,
                    answer: truth,
                    ..
                },
                Answer::Order { order },
            ) => Ok(grade_order(chunks, truth, order)),
            (
                Exercise::WordFromAudio {
                    options,
                    answer: truth,
                    ..
                },
                Answer::Choice { index },
            ) => Ok(grade_choice(*truth, *index, options)),
            (
                Exercise::ArticleMcq {
                    options,
                    answer: truth,
                    ..
                },
                Answer::Choice { index },
            ) => Ok(grade_choice(*truth, *index, options)),
            (Exercise::Recall { gloss, .. }, Answer::Recall { rating }) => {
                let rating = (*rating).clamp(1, 4);
                Ok(Grade {
                    correct: rating >= 3,
                    ratio: rating as f32 / 4.0,
                    items: vec![GradeItem {
                        index: rating as usize,
                        correct: rating >= 3,
                        expected: gloss.clone(),
                        note: None,
                    }],
                })
            }
            (Exercise::ChatScenario { .. }, _) => Err(GradeError::NotGradable),
            _ => Err(GradeError::WrongAnswerShape),
        }
    }

    /// 由句子词条生成拼句题。词块不足 3 个的短句不出题。
    pub fn build_sentence(entry: &Entry, seed: u64) -> Option<Self> {
        let words = chunks_from_surface(&entry.surface);
        if words.len() < 3 {
            return None;
        }
        let n = words.len();
        let mut shown = shuffle(n, seed_of(&entry.id) ^ seed);
        if shown.iter().copied().eq(0..n) {
            shown.rotate_left(1);
        }
        // 第 pos 个位置应填的，是展示序列中「原句第 pos 个词」的下标。
        let mut answer = vec![0usize; n];
        for (shown_index, original) in shown.iter().enumerate() {
            answer[*original] = shown_index;
        }
        let chunks = shown.iter().map(|i| words[*i].clone()).collect();
        Some(Exercise::BuildSentence {
            entry: entry.id.clone(),
            chunks,
            answer,
            prompt: entry.gloss.clone(),
        })
    }
}

fn grade_order(chunks: &[String], truth: &[usize], order: &[usize]) -> Grade {
    let mut items = Vec::with_capacity(truth.len());
    let mut hit = 0usize;
    for (pos, expected) in truth.iter().enumerate() {
        let correct = order.get(pos) == Some(expected);
        if correct {
            hit += 1;
        }
        items.push(GradeItem {
            index: pos,
            correct,
            expected: chunks.get(*expected).cloned(),
            note: if correct {
                None
            } else {
                Some("位置不对".into())
            },
        });
    }
    Grade {
        correct: hit == truth.len() && order.len() == truth.len(),
        ratio: if truth.is_empty() {
            0.0
        } else {
            hit as f32 / truth.len() as f32
        },
        items,
    }
}

fn grade_choice(truth: usize, got: usize, options: &[String]) -> Grade {
    let correct = truth == got;
    Grade {
        correct,
        ratio: if correct { 1.0 } else { 0.0 },
        items: vec![GradeItem {
            index: got,
            correct,
            expected: options.get(truth).cloned(),
            note: if correct {
                None
            } else {
                Some("再看一次".into())
            },
        }],
    }
}

/// 拼句素材：按空格切块。
///
/// 英语与韩语都以空格分词（韩语切到「어절」级，不切形态素——拖拽形态素
/// 对学习者没有意义）。不按空格分词的语言（日、中）需要 profile 提供切块，
/// 那类语言加进来时再接。
pub fn chunks_from_surface(surface: &str) -> Vec<String> {
    surface.split_whitespace().map(str::to_string).collect()
}

/// 固定种子打乱：同一次练习顺序稳定，下次练习换一个。
pub fn shuffle(len: usize, seed: u64) -> Vec<usize> {
    let mut idx: Vec<usize> = (0..len).collect();
    let mut state = seed | 1;
    for i in (1..len).rev() {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        let j = (state as usize) % (i + 1);
        idx.swap(i, j);
    }
    idx
}

pub fn seed_of(text: &str) -> u64 {
    let mut h: u64 = 1469598103934665603;
    for b in text.as_bytes() {
        h ^= *b as u64;
        h = h.wrapping_mul(1099511628211);
    }
    h
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sentence() -> Entry {
        Entry::new("en.s.1", "I kind of want to bail on the party")
    }

    #[test]
    fn build_sentence_is_solvable() {
        let ex = Exercise::build_sentence(&sentence(), 7).unwrap();
        let Exercise::BuildSentence { chunks, answer, .. } = &ex else {
            panic!("wrong kind")
        };
        assert_eq!(chunks.len(), answer.len());
        // 按 answer 取词应还原原句
        let restored: Vec<&str> = answer.iter().map(|i| chunks[*i].as_str()).collect();
        assert_eq!(restored.join(" "), sentence().surface);
        // 打乱后不应恰好等于原序
        assert_ne!(answer.iter().copied().eq(0..chunks.len()), true);
    }

    #[test]
    fn perfect_order_scores_full() {
        let ex = Exercise::build_sentence(&sentence(), 3).unwrap();
        let Exercise::BuildSentence { answer, .. } = &ex else {
            panic!()
        };
        let g = ex
            .grade(&Answer::Order {
                order: answer.clone(),
            })
            .unwrap();
        assert!(g.correct);
        assert_eq!(g.ratio, 1.0);
    }

    #[test]
    fn partial_order_scores_partial() {
        let ex = Exercise::build_sentence(&sentence(), 3).unwrap();
        let Exercise::BuildSentence { answer, chunks, .. } = &ex else {
            panic!()
        };
        let mut wrong = answer.clone();
        wrong.swap(0, 1);
        let g = ex.grade(&Answer::Order { order: wrong }).unwrap();
        assert!(!g.correct);
        assert!(g.ratio > 0.0 && g.ratio < 1.0);
        assert!(!g.items[0].correct);
        assert_eq!(
            g.items[0].expected.as_deref(),
            Some(chunks[answer[0]].as_str())
        );
    }

    #[test]
    fn choice_grading() {
        let ex = Exercise::WordFromAudio {
            entry: "en.w.1".into(),
            audio: "media/audio/a.wav".into(),
            options: vec!["gonna".into(), "wanna".into(), "gotta".into()],
            answer: 1,
        };
        assert!(ex.grade(&Answer::Choice { index: 1 }).unwrap().correct);
        let g = ex.grade(&Answer::Choice { index: 0 }).unwrap();
        assert!(!g.correct);
        assert_eq!(g.items[0].expected.as_deref(), Some("wanna"));
    }

    #[test]
    fn shape_mismatch_is_error() {
        let ex = Exercise::WordFromAudio {
            entry: "en.w.1".into(),
            audio: "media/audio/a.wav".into(),
            options: vec!["a".into(), "b".into()],
            answer: 0,
        };
        assert_eq!(
            ex.grade(&Answer::Order { order: vec![0] }).unwrap_err(),
            GradeError::WrongAnswerShape
        );
    }

    #[test]
    fn shuffle_is_permutation() {
        for len in 1..12 {
            let mut s = shuffle(len, 42);
            s.sort_unstable();
            assert_eq!(s, (0..len).collect::<Vec<_>>());
        }
    }
}
