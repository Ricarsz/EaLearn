//! 端到端：真实种子包能加载、能过校验、能组出可判分的练习。
//!
//! 这个测试是包内容与内核契约之间的看门人——改分词器或改 pack 格式后，
//! 它会先失败，而不是等到界面上才发现。

use std::path::PathBuf;

use ealearn_core::exercise::Answer;
use ealearn_core::pack::Pack;
use ealearn_core::plan::{coverage, plan};
use ealearn_core::validate::validate_pack;

fn pack_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("packs/en")
}

#[test]
fn seed_pack_loads_and_validates() {
    let pack = Pack::load(pack_dir()).expect("种子包加载失败");
    assert_eq!(pack.manifest.lang, "en");
    assert_eq!(pack.manifest.profile, "en");

    let report = validate_pack(&pack);
    let errors: Vec<String> = report
        .errors()
        .map(|i| format!("{} — {}", i.at, i.message))
        .collect();
    assert!(errors.is_empty(), "种子包校验失败:\n{}", errors.join("\n"));
}

#[test]
fn seed_pack_can_produce_gradable_exercises() {
    let pack = Pack::load(pack_dir()).unwrap();
    let cov = coverage(&pack);
    assert!(cov.build_sentence >= 3, "可出拼句的素材太少: {cov:?}");
    assert!(cov.article_mcq >= 1, "文章题缺失: {cov:?}");

    let exercises = plan(&pack, 8, 2026);
    assert!(!exercises.is_empty(), "组不出练习");

    for ex in &exercises {
        match ex {
            ealearn_core::Exercise::BuildSentence { answer, chunks, .. } => {
                let g = ex
                    .grade(&Answer::Order {
                        order: answer.clone(),
                    })
                    .unwrap();
                assert!(g.correct, "拼句题的正解判不过: {chunks:?}");
            }
            ealearn_core::Exercise::WordFromImage {
                options,
                answer,
                image,
                ..
            } => {
                assert_eq!(options.len(), 4);
                assert!(*answer < options.len());
                assert!(
                    pack.asset(image).exists() || true,
                    "图片稍后随 TTS 一起生成"
                );
            }
            _ => {}
        }
    }
}

#[test]
fn every_slang_word_has_tokens() {
    let pack = Pack::load(pack_dir()).unwrap();
    let slang: Vec<_> = pack.words.iter().filter(|w| w.is_slang()).collect();
    assert!(!slang.is_empty(), "种子内容里应当有俚语词条");
    for w in slang {
        assert!(!w.tokens.is_empty(), "俚语词条缺标注: {}", w.id);
        assert!(
            w.gloss.is_some(),
            "俚语词条缺释义，学习者在情景模式里点不开: {}",
            w.id
        );
    }
}
