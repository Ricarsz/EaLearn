//! 练习排程：从 pack 里组装出一节练习。
//!
//! 题型不存盘，由素材现算：句子有 3 个以上词块才出拼句，有图才出看图选词，
//! 有音频才出听音选词。因此 pack 只需要提供素材，不需要为每种题型各写一份。

use crate::entry::Entry;
use crate::exercise::{seed_of, shuffle, Exercise};
use crate::pack::Pack;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExerciseKind {
    BuildSentence,
    WordFromAudio,
    Recall,
    ArticleMcq,
}

impl ExerciseKind {
    pub fn as_str(self) -> &'static str {
        match self {
            ExerciseKind::BuildSentence => "build-sentence",
            ExerciseKind::WordFromAudio => "word-from-audio",
            ExerciseKind::Recall => "recall",
            ExerciseKind::ArticleMcq => "article-mcq",
        }
    }
}

/// 一个 pack 里能被出成某个题型的素材有多少。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Coverage {
    pub build_sentence: usize,
    pub word_from_audio: usize,
    pub recall: usize,
    pub article_mcq: usize,
}

impl Coverage {
    pub fn total(&self) -> usize {
        self.build_sentence + self.word_from_audio + self.recall + self.article_mcq
    }
}

pub fn coverage(pack: &Pack) -> Coverage {
    let entries = pack.words.iter().chain(pack.sentences.iter());
    let mut c = Coverage::default();
    for e in entries {
        if e.surface.split_whitespace().count() >= 3 {
            c.build_sentence += 1;
        }
        if !e.surface.trim().is_empty() && e.surface.split_whitespace().count() <= 3 {
            c.recall += 1;
        }
        if e.audio.is_some() && is_choice_word(e) {
            c.word_from_audio += 1;
        }
    }
    c.article_mcq = pack
        .articles
        .iter()
        .filter(|a| a.is_well_formed())
        .map(|a| a.questions.len())
        .sum();
    c
}

/// 组装一节练习。
///
/// 题型轮转：每轮按 `[拼句, 听音, 背诵]` 顺序取，缺素材就跳过，保证一节里
/// 不会全是同一种题型。文章题放在最后。
pub fn plan(pack: &Pack, limit: usize, seed: u64) -> Vec<Exercise> {
    let entries: Vec<&Entry> = pack.words.iter().chain(pack.sentences.iter()).collect();
    let mut order: Vec<usize> = (0..entries.len()).collect();
    order = shuffle(order.len(), seed ^ 0x5EED);
    order.retain(|i| !entries[*i].surface.trim().is_empty());

    let mut out: Vec<Exercise> = Vec::new();
    let mut cursor = 0usize;
    let mut used: std::collections::HashSet<(String, &'static str)> =
        std::collections::HashSet::new();

    while out.len() < limit && !order.is_empty() {
        let mut progressed = false;
        for kind in [
            ExerciseKind::BuildSentence,
            ExerciseKind::WordFromAudio,
            ExerciseKind::Recall,
        ] {
            if out.len() >= limit {
                break;
            }
            // 从 cursor 开始找第一个能出该题型的词条；同一节里不重复同一 (词条, 题型)
            for step in 0..order.len() {
                let idx = order[(cursor + step) % order.len()];
                let entry = entries[idx];
                let key = (entry.id.clone(), kind.as_str());
                if used.contains(&key) {
                    continue;
                }
                if let Some(ex) = build(entry, pack, kind, seed) {
                    out.push(ex);
                    used.insert(key);
                    cursor = (cursor + step + 1) % order.len();
                    progressed = true;
                    break;
                }
            }
        }
        if !progressed {
            break;
        }
    }

    // 文章题兜底补位
    for article in &pack.articles {
        if out.len() >= limit {
            break;
        }
        if !article.is_well_formed() {
            continue;
        }
        for (qi, q) in article.questions.iter().enumerate() {
            if out.len() >= limit {
                break;
            }
            out.push(Exercise::ArticleMcq {
                article: article.id.clone(),
                question: qi,
                prompt: q.prompt.clone(),
                options: q.options.clone(),
                answer: q.answer,
            });
        }
    }

    out
}

/// 按卡片类型挑最合适的题型。
///
/// 到期卡必须能练到：句子卡出拼句；词卡优先听音（有音频时最贴近复习目标），
/// 其次看图，都没有就退回拼句。退回失败说明这条卡没有可练的素材。
pub fn exercise_for(pack: &Pack, entry_id: &str, card_kind: &str, seed: u64) -> Option<Exercise> {
    let entry = pack.entry(entry_id)?;
    let s = seed ^ seed_of(&entry.id);
    if card_kind == "sentence" {
        return Exercise::build_sentence(entry, s);
    }
    build(entry, pack, ExerciseKind::WordFromAudio, s)
        .or_else(|| build(entry, pack, ExerciseKind::Recall, s))
        .or_else(|| Exercise::build_sentence(entry, s))
}

fn build(entry: &Entry, pack: &Pack, kind: ExerciseKind, seed: u64) -> Option<Exercise> {
    let s = seed ^ seed_of(&entry.id);
    match kind {
        ExerciseKind::BuildSentence => Exercise::build_sentence(entry, s),
        ExerciseKind::WordFromAudio => {
            if !is_choice_word(entry) {
                return None;
            }
            let audio = entry.audio.clone()?;
            let (options, answer) = choice_set(pack, entry, 4, s)?;
            Some(Exercise::WordFromAudio {
                entry: entry.id.clone(),
                audio,
                options,
                answer,
            })
        }
        ExerciseKind::Recall => {
            if entry.surface.split_whitespace().count() > 3 {
                return None;
            }
            Some(Exercise::Recall {
                entry: entry.id.clone(),
                surface: entry.surface.clone(),
                gloss: entry.gloss.clone(),
                reading: entry.tokens.first().and_then(|t| t.reading.clone()),
            })
        }
        ExerciseKind::ArticleMcq => None,
    }
}

/// 看图/听音只在「词或短语」上出题。整句拿来当四选一的答案是送分题——长度
/// 一眼就能认出来，练不到东西。
fn is_choice_word(entry: &Entry) -> bool {
    (1..=2).contains(&entry.surface.split_whitespace().count())
}

/// 选出 1 个正确答案 + n-1 个干扰项，返回（打乱后的选项，正确答案下标）。
///
/// 干扰项优先取同 tag 的词（同为俚语、同为日常），没有则退到全库随机——
/// 同 tag 的干扰项才有区分度，翻成「同一个词的近义/同类词」。
fn choice_set(pack: &Pack, entry: &Entry, n: usize, seed: u64) -> Option<(Vec<String>, usize)> {
    if n < 2 {
        return None;
    }
    let mut pool: Vec<&Entry> = pack
        .words
        .iter()
        .filter(|w| w.id != entry.id && !w.surface.trim().is_empty())
        .collect();
    if pool.is_empty() {
        return None;
    }

    let same_tag: Vec<&Entry> = pool
        .iter()
        .copied()
        .filter(|w| w.tags.iter().any(|t| entry.tags.contains(t)))
        .collect();
    if same_tag.len() >= n - 1 {
        pool = same_tag;
    }

    let order = shuffle(pool.len(), seed | 1);
    let mut options: Vec<String> = Vec::with_capacity(n);
    for i in order {
        if options.len() >= n - 1 {
            break;
        }
        let candidate = &pool[i].surface;
        if candidate != &entry.surface && !options.contains(candidate) {
            options.push(candidate.clone());
        }
    }
    if options.len() < n - 1 {
        return None;
    }

    options.push(entry.surface.clone());
    let positions = shuffle(options.len(), seed ^ 0xA11CE);
    let shuffled: Vec<String> = positions.iter().map(|i| options[*i].clone()).collect();
    let answer = positions.iter().position(|i| *i == options.len() - 1)?;
    Some((shuffled, answer))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::article::{Article, Question};
    use crate::entry::Token;

    fn word(id: &str, surface: &str, tags: &[&str]) -> Entry {
        let mut e = Entry::new(id, surface);
        e.tags = tags.iter().map(|t| t.to_string()).collect();
        e.tokens = vec![Token::new(surface, (0, surface.len()))];
        e
    }

    fn pack() -> Pack {
        let mut words = vec![
            word("w1", "gonna", &["slang"]),
            word("w2", "lowkey", &["slang"]),
            word("w3", "vibe", &["slang"]),
            word("w4", "ghost", &["slang"]),
            word("w5", "bail", &["slang"]),
        ];
        words[0].audio = Some("media/audio/w1.wav".into());
        let sentences = vec![Entry::new("s1", "I kind of want to bail")];
        let articles = vec![Article {
            id: "a1".into(),
            title: "t".into(),
            body: "b".into(),
            tokens: vec![],
            questions: vec![Question {
                prompt: "q".into(),
                options: vec!["x".into(), "y".into()],
                answer: 1,
                explain: None,
            }],
            tags: vec![],
            difficulty: None,
        }];
        Pack {
            root: "/nonexistent".into(),
            manifest: crate::manifest::Manifest {
                schema: crate::SCHEMA_VERSION,
                lang: "en".into(),
                display: "英语".into(),
                version: "0".into(),
                profile: "en".into(),
                voice: "v".into(),
                content_hash: None,
            },
            words,
            sentences,
            articles,
            dialogs: vec![],
        }
    }

    #[test]
    fn coverage_counts_available_material() {
        let c = coverage(&pack());
        assert_eq!(c.word_from_audio, 1);
        assert_eq!(c.build_sentence, 1);
        assert_eq!(c.article_mcq, 1);
    }

    #[test]
    fn plan_respects_limit_and_mixes_kinds() {
        let p = pack();
        let ex = plan(&p, 6, 7);
        assert_eq!(ex.len(), 6);
        let kinds: Vec<&str> = ex.iter().map(|e| e.kind()).collect();
        assert!(kinds.contains(&"build-sentence"));
        assert!(kinds.contains(&"word-from-audio"));
    }

    #[test]
    fn audio_choices_include_answer_and_distractors() {
        let p = pack();
        let ex = plan(&p, 10, 3);
        let (options, answer, entry_id) = ex
            .iter()
            .find_map(|e| match e {
                Exercise::WordFromAudio { options, answer, entry, .. } => {
                    Some((options.clone(), *answer, entry.clone()))
                }
                _ => None,
            })
            .expect("没有出听音题");
        assert_eq!(options.len(), 4);
        assert_eq!(&options[answer], &p.entry(&entry_id).unwrap().surface);
        let unique: std::collections::HashSet<_> = options.iter().collect();
        assert_eq!(unique.len(), 4, "选项里有重复: {options:?}");
    }

    #[test]
    fn plan_does_not_repeat_the_same_item() {
        let p = pack();
        let ex = plan(&p, 20, 9);
        let mut seen = std::collections::HashSet::new();
        for e in &ex {
            let key = (e.entry_id().unwrap_or("-").to_string(), e.kind());
            assert!(seen.insert(key.clone()), "同一节里重复出题: {key:?}");
        }
    }

    #[test]
    fn plan_is_deterministic_for_same_seed() {
        let p = pack();
        assert_eq!(plan(&p, 5, 42), plan(&p, 5, 42));
        // 换种子后顺序应当至少有一个种子与原来不同
        let base = plan(&p, 5, 42);
        let differs = [43u64, 44, 45, 46].iter().any(|s| plan(&p, 5, *s) != base);
        assert!(differs, "种子没有影响排程");
    }

    #[test]
    fn card_kind_picks_the_closest_exercise() {
        let p = pack();
        // w1 有音频 → 听音优先
        assert_eq!(exercise_for(&p, "w1", "word", 1).unwrap().kind(), "word-from-audio");
        // w2 没有音频 → 退回背诵
        assert_eq!(exercise_for(&p, "w2", "word", 1).unwrap().kind(), "recall");
        // 句子卡 → 拼句
        assert_eq!(exercise_for(&p, "s1", "sentence", 1).unwrap().kind(), "build-sentence");
        // 不存在的词条不该造出题
        assert!(exercise_for(&p, "nope", "word", 1).is_none());
    }

    #[test]
    fn missing_audio_falls_back_to_recall() {
        let mut p = pack();
        for w in &mut p.words {
            w.audio = None;
        }
        assert_eq!(coverage(&p).word_from_audio, 0);
        let kinds: Vec<&str> = plan(&p, 10, 1).iter().map(|e| e.kind()).collect();
        assert!(!kinds.contains(&"word-from-audio"));
        assert!(kinds.contains(&"recall"));
    }
}
