//! Tauri 壳层。这里只做三件事：把输入递进内核、把结果序列化回前端、
//! 管理状态。任何判分或调度逻辑都不写在这里——它属于 `ealearn-core`。

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use ealearn_core::article::Article;
use ealearn_core::dialog::Dialog;
use ealearn_core::entry::Token;
use ealearn_core::exercise::{Answer, Exercise, Grade};
use ealearn_core::pack::Pack;
use ealearn_core::plan::{self, Coverage, Mode};
use ealearn_core::srs::{CardState, Rating, Scheduler};
use ealearn_core::validate::check_coverage;
use ealearn_store::{CardKind, DayActivity, Stats, Store, StoreError};
use serde::Serialize;
use tauri::{Manager, State};

pub struct AppState {
    store: Mutex<Store>,
    packs: Mutex<HashMap<String, Pack>>,
}

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

fn packs_dir(app: &tauri::AppHandle) -> PathBuf {
    if let Ok(p) = std::env::var("EALEARN_PACKS") {
        return PathBuf::from(p);
    }
    let dev = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../packs");
    if dev.is_dir() {
        return dev;
    }
    // 打包安装后：随包安装的只读内容优先，其次系统级目录，最后落到用户数据目录。
    if let Ok(res) = app.path().resource_dir() {
        let packaged = res.join("packs");
        if packaged.is_dir() {
            return packaged;
        }
    }
    for sys in ["/usr/share/ealearn/packs", "/usr/local/share/ealearn/packs"] {
        let p = PathBuf::from(sys);
        if p.is_dir() {
            return p;
        }
    }
    app.path()
        .app_data_dir()
        .map(|d| d.join("packs"))
        .unwrap_or_else(|_| PathBuf::from("packs"))
}

fn load_packs(dir: &Path) -> Vec<Pack> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return out;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        match Pack::load(&path) {
            Ok(pack) => out.push(pack),
            Err(e) => eprintln!("跳过 {}: {e}", path.display()),
        }
    }
    out.sort_by(|a, b| a.manifest.display.cmp(&b.manifest.display));
    out
}

/// 从磁盘加载所有 pack，并把库里导入的词条与文章合并进去。
/// 启动和「清空数据」之后走同一条路径，保证内存与磁盘一致。
fn build_packs(app: &tauri::AppHandle, store: &Store) -> Result<HashMap<String, Pack>, String> {
    let at = now();
    let mut map = HashMap::new();
    for mut pack in load_packs(&packs_dir(app)) {
        let lang = pack.manifest.lang.clone();
        store
            .upsert_pack(
                &lang,
                &pack.manifest.display,
                &pack.manifest.version,
                pack.manifest.content_hash.as_deref(),
                true,
                at,
            )
            .map_err(err)?;
        attach_custom(store, &mut pack, &lang);
        map.insert(lang, pack);
    }
    Ok(map)
}

fn attach_custom(store: &Store, pack: &mut Pack, lang: &str) {
    let Some(profile) = ealearn_lang::profile(lang) else {
        return;
    };
    if let Ok(list) = store.custom_entries(lang) {
        for c in list {
            let mut entry = ealearn_core::entry::Entry::new(&c.id, &c.surface);
            entry.gloss = c.gloss;
            entry.tags = c.tags;
            entry.tokens = profile.tokenize(&c.surface);
            pack.words.push(entry);
        }
    }
    if let Ok(list) = store.custom_articles(lang) {
        for a in list {
            let questions = serde_json::from_str(&a.questions).unwrap_or_default();
            let tokens = profile.tokenize(&a.body);
            pack.articles.push(Article {
                id: a.id,
                title: a.title,
                body: a.body,
                tokens,
                questions,
                tags: vec!["custom".into()],
                difficulty: None,
            });
        }
    }
}

#[derive(Serialize)]
pub struct PackSummary {
    lang: String,
    display: String,
    version: String,
    enabled: bool,
    words: usize,
    sentences: usize,
    articles: usize,
    dialogs: usize,
    coverage: Coverage,
}

#[derive(Serialize)]
pub struct CardView {
    card_id: i64,
    kind: CardKind,
    state: CardState,
    label: String,
}

/// 一节练习。`review` 是其中来自到期复习卡的题数，前端用来显示「复习 N 题」。
#[derive(Serialize)]
pub struct SessionPlan {
    exercises: Vec<Exercise>,
    review: usize,
}

#[derive(Serialize)]
pub struct SubmitResult {
    grade: Grade,
    rating: u8,
    card: Option<CardView>,
}

#[derive(Serialize)]
pub struct DialogSummary {
    id: String,
    title: String,
    theme: String,
    nodes: usize,
}

// ── 命令 ──────────────────────────────────────────────────────────────

#[tauri::command]
fn list_packs(state: State<'_, AppState>) -> Result<Vec<PackSummary>, String> {
    let packs = state.packs.lock().map_err(err)?;
    let store = state.store.lock().map_err(err)?;
    let installed = store.packs().map_err(err)?;

    let mut out: Vec<PackSummary> = packs
        .values()
        .map(|p| PackSummary {
            lang: p.manifest.lang.clone(),
            display: p.manifest.display.clone(),
            version: p.manifest.version.clone(),
            enabled: installed
                .iter()
                .find(|r| r.lang == p.manifest.lang)
                .map(|r| r.enabled)
                .unwrap_or(true),
            words: p.words.len(),
            sentences: p.sentences.len(),
            articles: p.articles.len(),
            dialogs: p.dialogs.len(),
            coverage: plan::coverage(p),
        })
        .collect();
    out.sort_by(|a, b| a.display.cmp(&b.display));
    Ok(out)
}

#[tauri::command]
fn start_session(
    state: State<'_, AppState>,
    lang: String,
    limit: usize,
    seed: u64,
    mode: Option<String>,
) -> Result<SessionPlan, String> {
    let packs = state.packs.lock().map_err(err)?;
    let pack = packs
        .get(&lang)
        .ok_or_else(|| format!("未加载的语言包: {lang}"))?;
    let limit = limit.clamp(1, 40);
    let mode_str = mode.as_deref().unwrap_or("mixed");
    let mode = Mode::parse(mode_str);

    // 错题重练：拿最近错的词条重新出题，不掺新素材。
    if mode_str == "weak" {
        let store = state.store.lock().map_err(err)?;
        let since = now() - 30 * 86_400;
        let weak = store
            .weak_entries(since, &[lang.as_str()], limit)
            .map_err(err)?;
        let kinds: HashMap<String, String> = store
            .cards(&lang)
            .map_err(err)?
            .into_iter()
            .map(|c| (c.entry_id.clone(), c.kind.as_str().to_string()))
            .collect();
        drop(store);
        let exercises = weak
            .iter()
            .filter_map(|w| {
                let kind = kinds.get(&w.entry_id).map(String::as_str).unwrap_or("word");
                plan::exercise_for(pack, &w.entry_id, kind, seed ^ w.last_at as u64)
            })
            .collect();
        return Ok(SessionPlan {
            exercises,
            review: 0,
        });
    }

    if mode != Mode::Mixed {
        return Ok(SessionPlan {
            exercises: plan::plan_mode(pack, limit, seed, mode),
            review: 0,
        });
    }

    // 混合模式：到期卡先排，剩下的位置再用新素材补满。
    // 复习队列只按卡种挑题型，不记历史——同一张卡下次可能换一种练法。
    let due: Vec<plan::DueItem> = {
        let store = state.store.lock().map_err(err)?;
        store
            .due_cards(now(), limit, &[&lang])
            .map_err(err)?
            .into_iter()
            .map(|c| plan::DueItem {
                entry_id: c.entry_id,
                kind: c.kind.as_str().to_string(),
            })
            .collect()
    };

    let (exercises, review) = plan::plan_review_first(pack, limit, seed, &due);
    Ok(SessionPlan { exercises, review })
}

/// 阅读题需要正文。正文与题目分开取，练习页先渲染文章再出题。
#[tauri::command]
fn get_article(
    state: State<'_, AppState>,
    lang: String,
    id: String,
) -> Result<ealearn_core::article::Article, String> {
    let packs = state.packs.lock().map_err(err)?;
    let pack = packs
        .get(&lang)
        .ok_or_else(|| format!("未加载的语言包: {lang}"))?;
    pack.article(&id)
        .cloned()
        .ok_or_else(|| format!("没有这篇文章: {id}"))
}

#[tauri::command]
fn submit_answer(
    state: State<'_, AppState>,
    lang: String,
    exercise: Exercise,
    answer: Answer,
    elapsed_ms: i64,
) -> Result<SubmitResult, String> {
    let grade = exercise.grade(&answer).map_err(err)?;
    let packs = state.packs.lock().map_err(err)?;
    let pack = packs
        .get(&lang)
        .ok_or_else(|| format!("未加载的语言包: {lang}"))?;
    let store = state.store.lock().map_err(err)?;
    let at = now();

    let rating = if !grade.correct {
        Rating::Again
    } else if elapsed_ms < 4_000 {
        Rating::Easy
    } else {
        Rating::Good
    };

    let mut card = None;
    if let Some(entry_id) = exercise.entry_id() {
        let kind = card_kind(pack, entry_id);
        let existing = store.card_id(&lang, entry_id, kind).map_err(err)?;
        // 答错必入卡；答对且没卡就只记作答，不把每道题都塞进复习队列。
        let card_id = match (existing, grade.correct) {
            (Some(id), _) => Some(id),
            (None, false) => Some(store.add_card(&lang, entry_id, kind, at).map_err(err)?),
            (None, true) => None,
        };
        store
            .record_attempt(
                &lang,
                entry_id,
                card_id,
                exercise.kind(),
                &grade,
                elapsed_ms,
                at,
            )
            .map_err(err)?;
        if let Some(id) = card_id {
            let st = store
                .review_card(id, rating, at, &Scheduler::default())
                .map_err(err)?;
            card = Some(CardView {
                card_id: id,
                kind,
                state: st,
                label: pack
                    .entry(entry_id)
                    .map(|e| e.surface.clone())
                    .unwrap_or_else(|| entry_id.to_string()),
            });
        }
    } else {
        // 文章题与情景题没有词条，也要记一次作答，否则正确率与打卡漏统计。
        let subject = match &exercise {
            Exercise::ArticleMcq { article, .. } => format!("article:{article}"),
            Exercise::ChatScenario { dialog } => format!("dialog:{dialog}"),
            _ => format!("{}:-", exercise.kind()),
        };
        store
            .record_attempt(&lang, &subject, None, exercise.kind(), &grade, elapsed_ms, at)
            .map_err(err)?;
    }

    Ok(SubmitResult {
        grade,
        rating: rating as u8,
        card,
    })
}

fn card_kind(pack: &Pack, entry_id: &str) -> CardKind {
    match pack.entry(entry_id) {
        Some(e) if e.surface.split_whitespace().count() > 1 => CardKind::Sentence,
        Some(e) if e.is_slang() => CardKind::Slang,
        _ => CardKind::Word,
    }
}

#[tauri::command]
fn add_card(
    state: State<'_, AppState>,
    lang: String,
    entry_id: String,
    kind: Option<CardKind>,
) -> Result<CardView, String> {
    let packs = state.packs.lock().map_err(err)?;
    let pack = packs
        .get(&lang)
        .ok_or_else(|| format!("未加载的语言包: {lang}"))?;
    let store = state.store.lock().map_err(err)?;
    let kind = kind.unwrap_or_else(|| card_kind(pack, &entry_id));
    let at = now();
    let id = store.add_card(&lang, &entry_id, kind, at).map_err(err)?;
    let st = store.card_state(id).map_err(err)?;
    Ok(CardView {
        card_id: id,
        kind,
        state: st,
        label: pack
            .entry(&entry_id)
            .map(|e| e.surface.clone())
            .unwrap_or_else(|| entry_id.clone()),
    })
}

#[tauri::command]
fn stats(state: State<'_, AppState>) -> Result<Stats, String> {
    let store = state.store.lock().map_err(err)?;
    store.stats(now()).map_err(err)
}

#[tauri::command]
fn due_cards(
    state: State<'_, AppState>,
    lang: Option<String>,
    limit: usize,
) -> Result<usize, String> {
    let store = state.store.lock().map_err(err)?;
    let filter: Vec<&str> = lang.as_deref().map(|l| vec![l]).unwrap_or_default();
    Ok(store
        .due_cards(now(), limit.clamp(1, 200), &filter)
        .map_err(err)?
        .len())
}

#[tauri::command]
fn list_dialogs(state: State<'_, AppState>, lang: String) -> Result<Vec<DialogSummary>, String> {
    let packs = state.packs.lock().map_err(err)?;
    let pack = packs
        .get(&lang)
        .ok_or_else(|| format!("未加载的语言包: {lang}"))?;
    Ok(pack
        .dialogs
        .iter()
        .map(|d| DialogSummary {
            id: d.id.clone(),
            title: d.title.clone(),
            theme: d.theme.clone(),
            nodes: d.nodes.len(),
        })
        .collect())
}

#[tauri::command]
fn get_dialog(state: State<'_, AppState>, lang: String, id: String) -> Result<Dialog, String> {
    let packs = state.packs.lock().map_err(err)?;
    let pack = packs
        .get(&lang)
        .ok_or_else(|| format!("未加载的语言包: {lang}"))?;
    pack.dialog(&id)
        .cloned()
        .ok_or_else(|| format!("没有这段对话: {id}"))
}

/// 点词查询：返回该词的标注。标注来自 pack 里已经人工校过的 token，
/// 不是运行时现算的猜测。
#[tauri::command]
fn lookup(
    state: State<'_, AppState>,
    lang: String,
    entry_id: String,
) -> Result<Vec<Token>, String> {
    let packs = state.packs.lock().map_err(err)?;
    let pack = packs
        .get(&lang)
        .ok_or_else(|| format!("未加载的语言包: {lang}"))?;
    Ok(pack
        .entry(&entry_id)
        .map(|e| e.tokens.clone())
        .unwrap_or_default())
}

#[tauri::command]
fn get_setting(state: State<'_, AppState>, key: String) -> Result<Option<String>, String> {
    let store = state.store.lock().map_err(err)?;
    store.get_setting(&key).map_err(err)
}

#[tauri::command]
fn set_setting(state: State<'_, AppState>, key: String, value: String) -> Result<(), String> {
    let store = state.store.lock().map_err(err)?;
    store.set_setting(&key, &value).map_err(err)
}

/// 按词形查词条，供情景模式点词时用。命中优先比 lemma，再比 surface。
#[tauri::command]
fn define(
    state: State<'_, AppState>,
    lang: String,
    word: String,
) -> Result<Option<ealearn_core::entry::Entry>, String> {
    let packs = state.packs.lock().map_err(err)?;
    let pack = packs
        .get(&lang)
        .ok_or_else(|| format!("未加载的语言包: {lang}"))?;
    let key = word.trim().to_lowercase();
    Ok(pack
        .words
        .iter()
        .find(|e| {
            e.surface.to_lowercase() == key
                || e.lemma.as_deref().map(str::to_lowercase).as_deref() == Some(key.as_str())
        })
        .cloned())
}

#[derive(Serialize)]
pub struct LlmSettingsView {
    base_url: String,
    model: String,
    has_key: bool,
    /// 只回显末 4 位，避免把 key 送到前端
    key_hint: String,
}

#[derive(Serialize)]
pub struct ProviderPreset {
    id: String,
    name: String,
    base_url: String,
    note: String,
}

/// 预设供应商。都走 OpenAI 兼容协议，差异只在 base_url。
#[tauri::command]
fn llm_providers() -> Vec<ProviderPreset> {
    let p = |id: &str, name: &str, base_url: &str, note: &str| ProviderPreset {
        id: id.into(),
        name: name.into(),
        base_url: base_url.into(),
        note: note.into(),
    };
    vec![
        p(
            "opencode",
            "OpenCode Console",
            "https://opencode.ai/inference/openai/v1",
            "聚合网关，DeepSeek / GLM / Kimi / MiMo 等都有，含免费模型档",
        ),
        p(
            "deepseek",
            "DeepSeek",
            "https://api.deepseek.com/v1",
            "deepseek-chat / deepseek-reasoner，中文语感好，价格低",
        ),
        p(
            "zhipu",
            "智谱 GLM",
            "https://open.bigmodel.cn/api/paas/v4",
            "glm-4.6 等",
        ),
        p(
            "moonshot",
            "月之暗面 Kimi",
            "https://api.moonshot.cn/v1",
            "长上下文",
        ),
        p(
            "dashscope",
            "阿里通义千问",
            "https://dashscope.aliyuncs.com/compatible-mode/v1",
            "qwen 系列，兼容模式",
        ),
        p(
            "siliconflow",
            "硅基流动",
            "https://api.siliconflow.cn/v1",
            "国内多家开源模型托管",
        ),
        p("openai", "OpenAI", "https://api.openai.com/v1", "官方"),
        p("custom", "自定义", "", "任何 /chat/completions 兼容端点"),
    ]
}

#[tauri::command]
async fn llm_models(
    state: State<'_, AppState>,
    base_url: String,
    api_key: Option<String>,
) -> Result<Vec<String>, String> {
    let key = match api_key.filter(|k| !k.trim().is_empty()) {
        Some(k) => k,
        None => {
            let store = state.store.lock().map_err(err)?;
            store.get_setting(KEY_API).map_err(err)?.unwrap_or_default()
        }
    };
    ealearn_llm::LlmClient::list_models(&base_url, &key)
        .await
        .map_err(err)
}

const KEY_BASE_URL: &str = "llm_base_url";
const KEY_MODEL: &str = "llm_model";
const KEY_API: &str = "llm_api_key";

#[tauri::command]
fn get_llm_settings(state: State<'_, AppState>) -> Result<LlmSettingsView, String> {
    let store = state.store.lock().map_err(err)?;
    let key = store.get_setting(KEY_API).map_err(err)?.unwrap_or_default();
    Ok(LlmSettingsView {
        base_url: store
            .get_setting(KEY_BASE_URL)
            .map_err(err)?
            .unwrap_or_else(|| "https://api.openai.com/v1".into()),
        model: store
            .get_setting(KEY_MODEL)
            .map_err(err)?
            .unwrap_or_else(|| "gpt-4o-mini".into()),
        has_key: !key.is_empty(),
        key_hint: if key.len() > 4 {
            format!("…{}", &key[key.len() - 4..])
        } else {
            String::new()
        },
    })
}

#[tauri::command]
fn set_llm_settings(
    state: State<'_, AppState>,
    base_url: String,
    model: String,
    api_key: Option<String>,
) -> Result<(), String> {
    let store = state.store.lock().map_err(err)?;
    store.set_setting(KEY_BASE_URL, base_url.trim()).map_err(err)?;
    store.set_setting(KEY_MODEL, model.trim()).map_err(err)?;
    if let Some(k) = api_key {
        if !k.trim().is_empty() {
            store.set_setting(KEY_API, k.trim()).map_err(err)?;
        }
    }
    Ok(())
}

/// 一轮自由对话。
///
/// 流式片段通过 `llm://delta` 事件推给前端做打字效果；返回的是解析并校验过的
/// 完整回合。历史从库里取，前端不需要自己维护上下文。
#[tauri::command]
async fn chat_send(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    lang: String,
    session: String,
    theme: String,
    level: u8,
    text: String,
) -> Result<ealearn_llm::TutorTurn, String> {
    use tauri::Emitter;

    let text = text.trim().to_string();
    if text.is_empty() {
        return Err("说点什么再发".into());
    }

    let (cfg, history, slang, targets, label) = {
        let store = state.store.lock().map_err(err)?;
        let packs = state.packs.lock().map_err(err)?;
        let pack = packs.get(&lang).ok_or_else(|| format!("未加载的语言包: {lang}"))?;
        let base_url = store
            .get_setting(KEY_BASE_URL)
            .map_err(err)?
            .unwrap_or_else(|| "https://api.openai.com/v1".into());
        let model = store
            .get_setting(KEY_MODEL)
            .map_err(err)?
            .unwrap_or_else(|| "gpt-4o-mini".into());
        let api_key = store.get_setting(KEY_API).map_err(err)?.unwrap_or_default();

        let history = store.chat_history(&session, 12).map_err(err)?;
        let slang: Vec<String> = pack
            .words
            .iter()
            .filter(|w| w.is_slang())
            .map(|w| w.surface.clone())
            .take(20)
            .collect();
        // 复习队列里最不熟的几个词，让模型主动用上，聊天顺便复习
        let targets: Vec<String> = store
            .cards(&lang)
            .unwrap_or_default()
            .into_iter()
            .filter(|c| c.state.stability < 7.0)
            .filter_map(|c| pack.entry(&c.entry_id).map(|e| e.surface.clone()))
            .take(6)
            .collect();
        (
            ealearn_llm::LlmConfig {
                base_url,
                api_key,
                model,
                temperature: 0.8,
                timeout_secs: 120,
            },
            history,
            slang,
            targets,
            pack.manifest.display.clone(),
        )
    };

    let client = ealearn_llm::LlmClient::new(cfg).map_err(err)?;
    let prompt = ealearn_llm::prompt::system_prompt(&ealearn_llm::PromptContext {
        lang: &lang,
        target_label: &label,
        theme: &theme,
        level,
        slang: &slang,
        target_words: &targets,
    });
    let messages = ealearn_llm::prompt::build_messages(prompt, &history, &text);

    let sink = app.clone();
    let sess = session.clone();
    let on_delta = |chunk: &str| {
        let _ = sink.emit("llm://delta", serde_json::json!({ "session": sess, "text": chunk }));
    };

    let turn = client.chat(&messages, on_delta).await.map_err(err)?;

    {
        let store = state.store.lock().map_err(err)?;
        let at = now();
        store.append_chat(&session, "user", &text, at).map_err(err)?;
        store.append_chat(&session, "tutor", &turn.reply, at + 1).map_err(err)?;
    }

    Ok(turn)
}

#[tauri::command]
fn chat_history(
    state: State<'_, AppState>,
    session: String,
    limit: usize,
) -> Result<Vec<(String, String)>, String> {
    let store = state.store.lock().map_err(err)?;
    store.chat_history(&session, limit.clamp(1, 100)).map_err(err)
}

// ── 发音 ──────────────────────────────────────────────────────────────

const VOICE_KEY_PREFIX: &str = "tts_voice:";

fn to_data_url(bytes: &[u8]) -> String {
    use base64::Engine;
    format!(
        "data:audio/wav;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    )
}

#[tauri::command]
async fn tts_voices(lang: String) -> Result<Vec<ealearn_tts::VoiceInfo>, String> {
    tauri::async_runtime::spawn_blocking(move || ealearn_tts::available(&lang))
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn get_voice(state: State<'_, AppState>, lang: String) -> Result<Option<String>, String> {
    let store = state.store.lock().map_err(err)?;
    store.get_setting(&format!("{VOICE_KEY_PREFIX}{lang}")).map_err(err)
}

#[tauri::command]
fn set_voice(state: State<'_, AppState>, lang: String, voice: String) -> Result<(), String> {
    let store = state.store.lock().map_err(err)?;
    store
        .set_setting(&format!("{VOICE_KEY_PREFIX}{lang}"), voice.trim())
        .map_err(err)
}

/// 朗读一段文本，返回 data URL。
///
/// 优先用 pack 里预生成的音频（当用户选的音色和 pack 的音色一致时）；否则按需
/// 合成并缓存到用户数据目录。合成是阻塞调用，放到 blocking 线程里，别卡 UI。
#[tauri::command]
async fn speak(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    lang: String,
    text: String,
    entry_id: Option<String>,
    voice: Option<String>,
) -> Result<String, String> {
    let text = text.trim().to_string();
    if text.is_empty() {
        return Err("没有可朗读的内容".into());
    }
    // 试听时传 voice：只影响这一次合成，不写回设置
    let preview = voice.filter(|v| !v.trim().is_empty());

    let (spec, pregenerated, dir) = {
        let store = state.store.lock().map_err(err)?;
        let packs = state.packs.lock().map_err(err)?;
        let pack = packs.get(&lang).ok_or_else(|| format!("未加载的语言包: {lang}"))?;
        let spec = preview
            .clone()
            .or_else(|| store.get_setting(&format!("{VOICE_KEY_PREFIX}{lang}")).ok().flatten())
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| pack.manifest.voice.clone());

        let pregenerated = entry_id
            .as_deref()
            .and_then(|id| pack.entry(id))
            .and_then(|e| e.audio.clone())
            .filter(|_| spec == pack.manifest.voice)
            .map(|rel| pack.root.join(rel))
            .filter(|p| p.exists());

        let dir = app
            .path()
            .app_data_dir()
            .map_err(err)?
            .join("tts");
        (spec, pregenerated, dir)
    };

    if let Some(path) = pregenerated {
        let bytes = std::fs::read(&path).map_err(err)?;
        return Ok(to_data_url(&bytes));
    }

    let cached = dir.join(ealearn_tts::cache_name(&spec, &text));
    let bytes = if cached.exists() {
        std::fs::read(&cached).map_err(err)?
    } else {
        let spec2 = spec.clone();
        let text2 = text.clone();
        let cache2 = cached.clone();
        tauri::async_runtime::spawn_blocking(move || -> Result<(), String> {
            let engine = ealearn_tts::engine_for(&spec2).map_err(|e| e.to_string())?;
            if let Some(parent) = cache2.parent() {
                std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            let bytes = engine.synth_wav(&text2).map_err(|e| e.to_string())?;
            std::fs::write(&cache2, bytes).map_err(|e| e.to_string())
        })
        .await
        .map_err(|e| e.to_string())??;
        std::fs::read(&cached).map_err(err)?
    };

    Ok(to_data_url(&bytes))
}

// ── 音色目录 ──────────────────────────────────────────────────────────

const PIPER_VOICES_INDEX: &str =
    "https://huggingface.co/rhasspy/piper-voices/raw/main/voices.json";

#[derive(Serialize)]
pub struct VoiceOption {
    id: String,
    name: String,
    quality: String,
    size_mb: u64,
    installed: bool,
}

/// piper 官方索引里没有、但同样能直接用的社区音色。
struct ExtraVoice {
    key: &'static str,
    label: &'static str,
    lang: &'static str,
    /// 不含扩展名；下载 `<base>.onnx` 与 `<base>.onnx.json`
    base_url: &'static str,
    size_mb: u64,
}

const EXTRA_VOICES: &[ExtraVoice] = &[ExtraVoice {
    key: "ko_KR-yjin-female",
    label: "YJin 韩国女声",
    lang: "ko",
    base_url: "https://huggingface.co/YJin1983/piper_ko_female_v1/resolve/main/piper_ko_female_v1",
    size_mb: 61,
}];

async fn piper_index() -> Result<serde_json::Value, String> {
    reqwest::Client::new()
        .get(PIPER_VOICES_INDEX)
        .timeout(std::time::Duration::from_secs(40))
        .send()
        .await
        .map_err(|e| format!("拉取音色索引失败: {e}"))?
        .json()
        .await
        .map_err(|e| format!("解析音色索引失败: {e}"))
}

/// 列出 piper 官方索引里这门语言的音色，标出本机是否已有。
#[tauri::command]
async fn voice_catalog(lang: String) -> Result<Vec<VoiceOption>, String> {
    let idx = piper_index().await?;
    let map = idx.as_object().ok_or("音色索引格式不对")?;
    let dir = ealearn_tts::model_dir();
    let mut out = Vec::new();
    for (key, entry) in map {
        let code = entry
            .pointer("/language/family")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        if code != lang {
            continue;
        }
        let quality = entry
            .get("quality")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let size = entry
            .get("files")
            .and_then(|f| f.as_object())
            .and_then(|f| {
                f.iter()
                    .find(|(p, _)| p.ends_with(".onnx"))
                    .and_then(|(_, v)| v.get("size_bytes"))
                    .and_then(|v| v.as_u64())
            })
            .unwrap_or(0);
        out.push(VoiceOption {
            id: format!("piper:{key}"),
            name: entry
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or(key)
                .to_string(),
            quality: quality.to_string(),
            size_mb: size / 1_048_576,
            installed: dir.join(format!("{key}.onnx")).exists(),
        });
    }
    out.sort_by(|a, b| a.id.cmp(&b.id));
    for extra in EXTRA_VOICES.iter().filter(|v| v.lang == lang) {
        out.push(VoiceOption {
            id: format!("piper:{}", extra.key),
            name: extra.label.to_string(),
            quality: "community".to_string(),
            size_mb: extra.size_mb,
            installed: dir.join(format!("{}.onnx", extra.key)).exists(),
        });
    }
    Ok(out)
}

/// 下载一个音色（onnx + onnx.json）到模型目录。
#[tauri::command]
async fn install_voice(id: String) -> Result<(), String> {
    let key = id
        .strip_prefix("piper:")
        .ok_or_else(|| format!("音色 id 不对: {id}"))?
        .to_string();
    let dir = ealearn_tts::model_dir();
    std::fs::create_dir_all(&dir).map_err(err)?;
    let client = reqwest::Client::new();

    // 社区音色：文件名与上游不同，按 key 命名存好，引擎按 stem 就能找到。
    if let Some(extra) = EXTRA_VOICES.iter().find(|v| v.key == key) {
        for ext in [".onnx", ".onnx.json"] {
            let url = format!("{}{ext}", extra.base_url);
            let bytes = fetch_bytes(&client, &url).await?;
            std::fs::write(dir.join(format!("{key}{ext}")), &bytes).map_err(err)?;
        }
        return Ok(());
    }

    let idx = piper_index().await?;
    let entry = idx.get(&key).ok_or_else(|| format!("索引里没有 {key}"))?;
    let files = entry
        .get("files")
        .and_then(|f| f.as_object())
        .ok_or_else(|| format!("{key} 缺少文件信息"))?;

    for path in files.keys() {
        if !(path.ends_with(".onnx") || path.ends_with(".onnx.json")) {
            continue;
        }
        let url = format!("https://huggingface.co/rhasspy/piper-voices/resolve/main/{path}");
        let bytes = fetch_bytes(&client, &url).await?;
        let name = path.rsplit('/').next().unwrap_or("voice.onnx");
        std::fs::write(dir.join(name), &bytes).map_err(err)?;
    }
    Ok(())
}

async fn fetch_bytes(client: &reqwest::Client, url: &str) -> Result<Vec<u8>, String> {
    let resp = client
        .get(url)
        .timeout(std::time::Duration::from_secs(300))
        .send()
        .await
        .map_err(|e| format!("下载失败: {e}"))?
        .error_for_status()
        .map_err(|e| format!("下载失败: {e}"))?;
    resp.bytes()
        .await
        .map(|b| b.to_vec())
        .map_err(|e| format!("读取失败: {e}"))
}

// ── 词库与导入 ────────────────────────────────────────────────────────

#[derive(Serialize)]
pub struct CardListItem {
    card_id: i64,
    entry_id: String,
    surface: String,
    gloss: Option<String>,
    kind: CardKind,
    stability: f64,
    difficulty: f64,
    reps: u32,
    lapses: u32,
    due: i64,
    last_review: Option<i64>,
    /// 0–1，由 FSRS 稳定性换算；只用于展示，不参与调度
    mastery: f64,
    is_custom: bool,
    /// 来自哪本单词书，手工导入为空
    book: String,
}

/// 删掉自定义词条及其卡片。词库里的「自定义」条目才允许删。
#[tauri::command]
fn delete_custom_entry(
    state: State<'_, AppState>,
    lang: String,
    entry_id: String,
) -> Result<(), String> {
    if !entry_id.starts_with("custom.") {
        return Err("只能删除自定义词条".into());
    }
    let mut packs = state.packs.lock().map_err(err)?;
    let store = state.store.lock().map_err(err)?;
    if let Some(pack) = packs.get_mut(&lang) {
        pack.words.retain(|e| e.id != entry_id);
    }
    store.delete_custom_entry(&entry_id).map_err(err)
}

#[tauri::command]
fn list_cards(state: State<'_, AppState>, lang: String) -> Result<Vec<CardListItem>, String> {
    let packs = state.packs.lock().map_err(err)?;
    let pack = packs.get(&lang).ok_or_else(|| format!("未加载的语言包: {lang}"))?;
    let store = state.store.lock().map_err(err)?;

    let books: std::collections::HashMap<String, String> = store
        .custom_entries(&lang)
        .map_err(err)?
        .into_iter()
        .map(|c| (c.id, c.book))
        .collect();

    // 导入几千个词之后线性查找会拖慢词库页，先建一次 id → 词条 的索引。
    let index: std::collections::HashMap<&str, &ealearn_core::entry::Entry> = pack
        .words
        .iter()
        .chain(pack.sentences.iter())
        .map(|e| (e.id.as_str(), e))
        .collect();

    Ok(store
        .cards(&lang)
        .map_err(err)?
        .into_iter()
        .map(|c| {
            let entry = index.get(c.entry_id.as_str()).copied();
            CardListItem {
                card_id: c.card_id,
                entry_id: c.entry_id.clone(),
                surface: entry.map(|e| e.surface.clone()).unwrap_or_else(|| c.entry_id.clone()),
                gloss: entry.and_then(|e| e.gloss.clone()),
                kind: c.kind,
                stability: c.state.stability,
                difficulty: c.state.difficulty,
                reps: c.state.reps,
                lapses: c.state.lapses,
                due: c.state.due,
                last_review: c.state.last_review,
                mastery: mastery_of(c.state.stability),
                is_custom: c.entry_id.starts_with("custom."),
                book: books.get(&c.entry_id).cloned().unwrap_or_default(),
            }
        })
        .collect())
}

fn mastery_of(stability: f64) -> f64 {
    if stability <= 0.0 {
        0.0
    } else {
        (1.0 - (-stability / 21.0).exp()).clamp(0.0, 1.0)
    }
}

#[tauri::command]
fn delete_card(state: State<'_, AppState>, card_id: i64) -> Result<(), String> {
    let store = state.store.lock().map_err(err)?;
    store.delete_card(card_id).map_err(err)
}

#[derive(Serialize)]
pub struct ImportResult {
    added: usize,
    /// 已存在、只更新了释义或标签
    updated: usize,
    skipped: Vec<String>,
}

/// 导入自定义词条：每行 `word<TAB>释义[<TAB>标签]`。
/// 导入即入卡——用户特意写下来的词，不该还要再点一次才会复习。
fn do_import(
    state: &AppState,
    lang: &str,
    text: &str,
    book: &str,
) -> Result<ImportResult, String> {
    let report = ealearn_core::import::parse(text);
    if report.words.is_empty() {
        return Ok(ImportResult { added: 0, updated: 0, skipped: report.skipped });
    }

    let mut packs = state.packs.lock().map_err(err)?;
    let store = state.store.lock().map_err(err)?;
    let pack = packs
        .get_mut(lang)
        .ok_or_else(|| format!("未加载的语言包: {lang}"))?;
    let profile = ealearn_lang::profile(lang);
    let at = now();

    let rows: Vec<(String, Option<String>, String, String)> = report
        .words
        .iter()
        .map(|w| {
            (
                w.surface.clone(),
                w.gloss.clone(),
                w.tags.join(","),
                book.to_string(),
            )
        })
        .collect();
    let stored = store.bulk_add_custom_entries(lang, &rows, at).map_err(err)?;

    // 多词短语按句子卡入队，练的时候走拼句；单词才是词卡。
    let mut word_ids: Vec<String> = Vec::new();
    let mut sentence_ids: Vec<String> = Vec::new();
    for ((id, _), w) in stored.iter().zip(report.words.iter()) {
        if w.surface.split_whitespace().count() > 1 {
            sentence_ids.push(id.clone());
        } else {
            word_ids.push(id.clone());
        }
    }
    store
        .bulk_add_cards(lang, &word_ids, CardKind::Word, at)
        .map_err(err)?;
    store
        .bulk_add_cards(lang, &sentence_ids, CardKind::Sentence, at)
        .map_err(err)?;

    // 合并进内存里的 pack，练习组卷才能立刻用上
    let mut index: std::collections::HashMap<String, usize> = pack
        .words
        .iter()
        .enumerate()
        .map(|(i, e)| (e.id.clone(), i))
        .collect();

    let mut added = 0usize;
    let mut updated = 0usize;
    for ((id, created), word) in stored.iter().zip(report.words.iter()) {
        if !*created {
            if let Some(&i) = index.get(id) {
                pack.words[i].gloss = word.gloss.clone();
                pack.words[i].tags = word.tags.clone();
            }
            updated += 1;
            continue;
        }
        let mut entry = ealearn_core::entry::Entry::new(id, &word.surface);
        entry.gloss = word.gloss.clone();
        entry.tags = word.tags.clone();
        if let Some(p) = profile {
            entry.tokens = p.tokenize(&word.surface);
        }
        index.insert(id.clone(), pack.words.len());
        pack.words.push(entry);
        added += 1;
    }

    Ok(ImportResult { added, updated, skipped: report.skipped })
}

/// 导入自定义词条：每行 `word<TAB>释义[<TAB>标签]`。
/// 导入即入卡——用户特意写下来的词，不该还要再点一次才会复习。
#[tauri::command]
fn import_words(
    state: State<'_, AppState>,
    lang: String,
    text: String,
    book: Option<String>,
) -> Result<ImportResult, String> {
    do_import(&state, &lang, &text, book.as_deref().unwrap_or(""))
}

/// 导入一篇自定义阅读文章：`#` 标题、`?` 题干、`-` 选项、`=` 答案，其余是正文。
///
/// 正文用语言 profile 分词并做对齐校验——正文里的标注必须和原文逐字节一致，
/// 否则宁可不收，避免点词查义点到错的地方。
#[tauri::command]
fn import_article(state: State<'_, AppState>, lang: String, text: String) -> Result<String, String> {
    import_article_text(&state, &lang, &text)
}

fn import_article_text(state: &AppState, lang: &str, text: &str) -> Result<String, String> {
    let parsed =
        ealearn_core::import::parse_article(text).map_err(|e| format!("格式有问题：{e}"))?;
    let profile =
        ealearn_lang::profile(lang).ok_or_else(|| format!("未实现的语言: {lang}"))?;
    let tokens = profile.tokenize(&parsed.body);
    let report = check_coverage("article", &parsed.body, &tokens);
    if !report.is_ok() {
        let msg = report
            .errors()
            .next()
            .map(|i| i.message.clone())
            .unwrap_or_default();
        return Err(format!("正文分词没对齐：{msg}"));
    }

    let questions = serde_json::to_string(&parsed.questions).map_err(err)?;
    let mut packs = state.packs.lock().map_err(err)?;
    let store = state.store.lock().map_err(err)?;
    let id = store
        .add_custom_article(lang, &parsed.title, &parsed.body, &questions, now())
        .map_err(err)?;
    if let Some(pack) = packs.get_mut(lang) {
        pack.articles.push(Article {
            id: id.clone(),
            title: parsed.title,
            body: parsed.body,
            tokens,
            questions: parsed.questions,
            tags: vec!["custom".into()],
            difficulty: None,
        });
    }
    Ok(id)
}

#[tauri::command]
fn reset_data(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    scope: String,
) -> Result<(), String> {
    {
        let store = state.store.lock().map_err(err)?;
        match scope.as_str() {
            "progress" => store.clear_progress().map_err(err)?,
            "all" => store.clear_all().map_err(err)?,
            other => return Err(format!("未知的清理范围: {other}")),
        }
    }

    // 清掉的自定义词条与文章不能还留在内存里的 pack 中
    let rebuilt = {
        let store = state.store.lock().map_err(err)?;
        build_packs(&app, &store)?
    };
    let mut packs = state.packs.lock().map_err(err)?;
    *packs = rebuilt;
    Ok(())
}

// ── 单词书 ────────────────────────────────────────────────────────────

fn books_dir(app: &tauri::AppHandle) -> PathBuf {
    if let Ok(p) = std::env::var("EALEARN_BOOKS") {
        return PathBuf::from(p);
    }
    let dev = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../books");
    if dev.is_dir() {
        return dev;
    }
    app.path()
        .app_data_dir()
        .map(|d| d.join("books"))
        .unwrap_or_else(|_| PathBuf::from("books"))
}

const KEY_BOOKS_INDEX: &str = "books_index_url";
const DEFAULT_BOOKS_INDEX: &str =
    "https://raw.githubusercontent.com/Ricarsz/EaLearn/main/books/index.json";

#[derive(Serialize, serde::Deserialize, Clone)]
pub struct RemoteBook {
    id: String,
    lang: String,
    /// words / sentences / article；老索引没有这个字段，按 words 处理
    #[serde(default)]
    kind: Option<String>,
    name: String,
    url: String,
    /// 备用地址（镜像）：主地址失败时依次尝试
    #[serde(default)]
    mirrors: Vec<String>,
    #[serde(default)]
    count: usize,
    #[serde(default)]
    license: String,
    #[serde(default)]
    note: String,
    /// 本地已经导入多少条（按语言 + 书名匹配），用来显示「已下载」
    #[serde(default)]
    imported: usize,
}

#[derive(serde::Deserialize)]
struct BooksIndex {
    #[serde(default)]
    books: Vec<RemoteBook>,
}

/// 读取索引：http(s) 走网络，其余当路径（相对路径按 `books/` 解析）。
async fn load_index(app: &tauri::AppHandle, source: &str) -> Result<BooksIndex, String> {
    if source.starts_with("http://") || source.starts_with("https://") {
        let resp = reqwest::Client::new()
            .get(source)
            .timeout(std::time::Duration::from_secs(20))
            .send()
            .await
            .map_err(|e| format!("拉取索引失败: {e}"))?;
        if !resp.status().is_success() {
            return Err(format!("索引返回 {}", resp.status()));
        }
        return resp.json().await.map_err(|e| format!("解析索引失败: {e}"));
    }

    let raw = source.strip_prefix("file://").unwrap_or(source);
    let path = PathBuf::from(raw);
    let path = if path.is_absolute() {
        path
    } else {
        books_dir(app).join(path)
    };
    let text =
        std::fs::read_to_string(&path).map_err(|e| format!("读取 {} 失败: {e}", path.display()))?;
    serde_json::from_str(&text).map_err(|e| format!("解析 {} 失败: {e}", path.display()))
}

/// 在线单词书索引。索引只是一个 JSON 文件，加书不用发版。
///
/// 地址可以是 http(s) URL、绝对路径，或相对 `books/` 的文件名。开发时仓库里的
/// `books/index.json` 优先——改词书列表不必先推到仓库。
#[tauri::command]
async fn remote_books(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    index_url: Option<String>,
) -> Result<Vec<RemoteBook>, String> {
    let stored = match index_url.filter(|u| !u.trim().is_empty()) {
        Some(u) => Some(u),
        None => {
            let store = state.store.lock().map_err(err)?;
            store
                .get_setting(KEY_BOOKS_INDEX)
                .map_err(err)?
                .filter(|u| !u.trim().is_empty())
        }
    };
    let source = match stored {
        Some(s) => s,
        None => {
            let local = books_dir(&app).join("index.json");
            if local.is_file() {
                local.to_string_lossy().into_owned()
            } else {
                DEFAULT_BOOKS_INDEX.to_string()
            }
        }
    };

    // 索引地址可以写成多行（逗号分隔也行）：第一个失败就换下一个。
    let mut index = None;
    let mut last_err = String::new();
    for candidate in source
        .split(['\n', ','])
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        match load_index(&app, candidate).await {
            Ok(i) => {
                index = Some(i);
                break;
            }
            Err(e) => last_err = e,
        }
    }
    let index = index.ok_or_else(|| format!("读取索引失败：{last_err}"))?;

    // 标出哪些书已经导入过：按 (语言, 书名) 数本地词条。
    let mut books = index.books;
    let store = state.store.lock().map_err(err)?;
    let mut counts: HashMap<(String, String), usize> = HashMap::new();
    let langs: std::collections::HashSet<String> = books.iter().map(|b| b.lang.clone()).collect();
    for lang in langs {
        for c in store.custom_entries(&lang).unwrap_or_default() {
            if c.book.is_empty() {
                continue;
            }
            *counts.entry((lang.clone(), c.book)).or_default() += 1;
        }
    }
    for b in &mut books {
        b.imported = counts
            .get(&(b.lang.clone(), b.name.clone()))
            .copied()
            .unwrap_or(0);
    }
    Ok(books)
}

#[tauri::command]
fn books_index_url(state: State<'_, AppState>) -> Result<String, String> {
    let store = state.store.lock().map_err(err)?;
    Ok(store
        .get_setting(KEY_BOOKS_INDEX)
        .map_err(err)?
        .unwrap_or_default())
}

#[tauri::command]
fn set_books_index_url(state: State<'_, AppState>, url: String) -> Result<(), String> {
    let store = state.store.lock().map_err(err)?;
    store.set_setting(KEY_BOOKS_INDEX, url.trim()).map_err(err)
}

/// 下载并导入一本在线单词书。
#[tauri::command]
async fn install_remote_book(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    lang: String,
    name: String,
    url: String,
    kind: Option<String>,
    mirrors: Option<Vec<String>>,
) -> Result<ImportResult, String> {
    let mut candidates: Vec<String> = vec![url];
    candidates.extend(mirrors.unwrap_or_default());
    candidates.retain(|u| !u.trim().is_empty());
    candidates.dedup();

    let client = reqwest::Client::new();
    let mut last_err = String::from("没有可用的下载地址");
    let mut text = None;
    for candidate in &candidates {
        // 本地路径（不含协议）按 `books/` 解析，方便先在本机试内容再推仓库。
        if !candidate.starts_with("http://") && !candidate.starts_with("https://") {
            let raw = candidate.strip_prefix("file://").unwrap_or(candidate);
            let path = PathBuf::from(raw);
            let path = if path.is_absolute() {
                path
            } else {
                books_dir(&app).join(path)
            };
            match std::fs::read_to_string(&path) {
                Ok(t) => {
                    text = Some(t);
                    break;
                }
                Err(e) => {
                    last_err = format!("读取 {} 失败: {e}", path.display());
                    continue;
                }
            }
        }

        let resp = match client
            .get(candidate)
            .timeout(std::time::Duration::from_secs(60))
            .send()
            .await
        {
            Ok(r) if r.status().is_success() => r,
            Ok(r) => {
                last_err = format!("{candidate} 返回 {}", r.status());
                continue;
            }
            Err(e) => {
                last_err = format!("{candidate} 下载失败: {e}");
                continue;
            }
        };
        match resp.text().await {
            Ok(t) => {
                text = Some(t);
                break;
            }
            Err(e) => last_err = format!("{candidate} 读取失败: {e}"),
        }
    }
    let text = text.ok_or(last_err)?;

    // 句库与词库走同一套解析；文章是文本格式，单独走文章解析。
    match kind.as_deref().unwrap_or("words") {
        "article" | "articles" => {
            import_article_text(&state, &lang, &text)?;
            Ok(ImportResult {
                added: 1,
                updated: 0,
                skipped: Vec::new(),
            })
        }
        _ => do_import(&state, &lang, &text, &name),
    }
}

#[derive(Serialize)]
pub struct BookInfo {
    id: String,
    lang: String,
    name: String,
    count: usize,
    /// 已经导入过多少个词
    imported: usize,
}

/// 文件名形如 `en-daily.tsv` / `ko-daily.tsv`：第一段是语言，其余是书名。
#[tauri::command]
fn list_books(app: tauri::AppHandle, state: State<'_, AppState>) -> Result<Vec<BookInfo>, String> {
    let dir = books_dir(&app);
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return Ok(Vec::new());
    };
    let store = state.store.lock().map_err(err)?;

    let mut out = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("tsv") {
            continue;
        }
        let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        let Some((lang, name)) = stem.split_once('-') else {
            continue;
        };
        let count = std::fs::read_to_string(&path)
            .map(|t| t.lines().filter(|l| !l.trim().is_empty() && !l.starts_with('#')).count())
            .unwrap_or(0);
        let imported = store
            .custom_entries(lang)
            .map(|list| list.iter().filter(|c| c.book == name).count())
            .unwrap_or(0);
        out.push(BookInfo {
            id: stem.to_string(),
            lang: lang.to_string(),
            name: name.to_string(),
            count,
            imported,
        });
    }
    out.sort_by(|a, b| a.lang.cmp(&b.lang).then(a.name.cmp(&b.name)));
    Ok(out)
}

#[tauri::command]
fn import_book(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    id: String,
) -> Result<ImportResult, String> {
    let Some((lang, name)) = id.split_once('-') else {
        return Err(format!("单词书 id 不合法: {id}"));
    };
    let path = books_dir(&app).join(format!("{id}.tsv"));
    let text = std::fs::read_to_string(&path)
        .map_err(|e| format!("读取 {} 失败: {e}", path.display()))?;
    do_import(&state, lang, &text, name)
}

// ── 专注与打卡 ────────────────────────────────────────────────────────

#[tauri::command]
fn record_focus(
    state: State<'_, AppState>,
    minutes: u32,
    completed: bool,
) -> Result<(), String> {
    let store = state.store.lock().map_err(err)?;
    store.add_focus(minutes, completed, now()).map_err(err)
}

#[tauri::command]
fn activity(
    state: State<'_, AppState>,
    days: u32,
    tz_offset_min: i32,
) -> Result<Vec<DayActivity>, String> {
    let store = state.store.lock().map_err(err)?;
    store
        .activity(days.clamp(1, 400), now(), tz_offset_min)
        .map_err(err)
}

#[tauri::command]
fn daily_accuracy(
    state: State<'_, AppState>,
    days: u32,
    tz_offset_min: i32,
) -> Result<Vec<ealearn_store::DayAccuracy>, String> {
    let store = state.store.lock().map_err(err)?;
    store
        .daily_accuracy(days.clamp(1, 400), now(), tz_offset_min)
        .map_err(err)
}

#[tauri::command]
fn kind_accuracy(
    state: State<'_, AppState>,
    days: u32,
    lang: Option<String>,
) -> Result<Vec<ealearn_store::KindAccuracy>, String> {
    let store = state.store.lock().map_err(err)?;
    let since = now() - days.clamp(1, 400) as i64 * 86_400;
    let packs: Vec<&str> = lang.as_deref().into_iter().collect();
    store.kind_accuracy(since, &packs).map_err(err)
}

#[tauri::command]
fn due_forecast(
    state: State<'_, AppState>,
    days: u32,
    tz_offset_min: i32,
    lang: Option<String>,
) -> Result<Vec<ealearn_store::DueBucket>, String> {
    let store = state.store.lock().map_err(err)?;
    let packs: Vec<&str> = lang.as_deref().into_iter().collect();
    store
        .due_forecast(days.clamp(1, 90), now(), tz_offset_min, &packs)
        .map_err(err)
}

#[derive(Serialize)]
pub struct WeakEntryView {
    entry_id: String,
    surface: String,
    gloss: Option<String>,
    total: i64,
    wrong: i64,
    last_at: i64,
}

/// 错得最多的词条，带词面与释义。
#[tauri::command]
fn weak_entries(
    state: State<'_, AppState>,
    lang: String,
    days: u32,
    limit: usize,
) -> Result<Vec<WeakEntryView>, String> {
    let packs = state.packs.lock().map_err(err)?;
    let pack = packs
        .get(&lang)
        .ok_or_else(|| format!("未加载的语言包: {lang}"))?;
    let store = state.store.lock().map_err(err)?;
    let since = now() - days.clamp(1, 400) as i64 * 86_400;
    let rows = store
        .weak_entries(since, &[lang.as_str()], limit.clamp(1, 500))
        .map_err(err)?;

    let index: HashMap<&str, &ealearn_core::entry::Entry> = pack
        .words
        .iter()
        .chain(pack.sentences.iter())
        .map(|e| (e.id.as_str(), e))
        .collect();

    Ok(rows
        .into_iter()
        .map(|r| {
            let entry = index.get(r.entry_id.as_str()).copied();
            WeakEntryView {
                surface: entry
                    .map(|e| e.surface.clone())
                    .unwrap_or_else(|| r.entry_id.clone()),
                gloss: entry.and_then(|e| e.gloss.clone()),
                entry_id: r.entry_id,
                total: r.total,
                wrong: r.wrong,
                last_at: r.last_at,
            }
        })
        .collect())
}

/// 导出卡片复习状态为 CSV（Anki 可直接导入「正面/背面」两列）。
/// 返回写出的文件路径；`dir` 为空时写到应用数据目录的 exports/。
#[tauri::command]
fn export_cards(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    lang: String,
    dir: Option<String>,
) -> Result<String, String> {
    let packs = state.packs.lock().map_err(err)?;
    let pack = packs
        .get(&lang)
        .ok_or_else(|| format!("未加载的语言包: {lang}"))?;
    let store = state.store.lock().map_err(err)?;
    let rows = store.export_rows(&[lang.as_str()]).map_err(err)?;

    let index: HashMap<&str, &ealearn_core::entry::Entry> = pack
        .words
        .iter()
        .chain(pack.sentences.iter())
        .map(|e| (e.id.as_str(), e))
        .collect();

    let mut csv = String::from("word,gloss,kind,stability,difficulty,reps,lapses,due,last_review\n");
    for r in rows {
        let entry = index.get(r.entry_id.as_str()).copied();
        let word = entry.map(|e| e.surface.clone()).unwrap_or_else(|| r.entry_id.clone());
        let gloss = entry.and_then(|e| e.gloss.clone()).unwrap_or_default();
        csv.push_str(&format!(
            "{},{},{},{:.2},{:.2},{},{},{},{}\n",
            csv_cell(&word),
            csv_cell(&gloss),
            csv_cell(&r.kind),
            r.stability,
            r.difficulty,
            r.reps,
            r.lapses,
            ymd(r.due),
            if r.last_review > 0 { ymd(r.last_review) } else { String::new() }
        ));
    }

    let base = dir
        .filter(|d| !d.trim().is_empty())
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| app.path().app_data_dir().unwrap_or_default().join("exports"));
    std::fs::create_dir_all(&base).map_err(err)?;
    let path = base.join(format!("ealearn-{lang}-{}.csv", ymd(now())));
    std::fs::write(&path, csv).map_err(err)?;
    Ok(path.to_string_lossy().into_owned())
}

fn csv_cell(s: &str) -> String {
    if s.contains([',', '"', '\n']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

fn ymd(ts: i64) -> String {
    // 只用于导出文件的日期列，按 UTC 近似即可。
    let days = ts.div_euclid(86_400);
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}-{m:02}-{d:02}")
}

/// 把整库备份到应用数据目录的 exports/，返回文件路径。
#[tauri::command]
fn backup_db(app: tauri::AppHandle, state: State<'_, AppState>) -> Result<String, String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(err)?
        .join("exports");
    let path = dir.join(format!("ealearn-{}.db", ymd(now())));
    let store = state.store.lock().map_err(err)?;
    store.backup_to(&path).map_err(err)?;
    Ok(path.to_string_lossy().into_owned())
}

pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let data_dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&data_dir)?;

            let store = Store::open(data_dir.join("ealearn.db")).map_err(boxed)?;
            let map = build_packs(app.handle(), &store).map_err(boxed)?;

            app.manage(AppState {
                store: Mutex::new(store),
                packs: Mutex::new(map),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            list_packs,
            start_session,
            get_article,
            submit_answer,
            add_card,
            stats,
            due_cards,
            list_dialogs,
            get_dialog,
            lookup,
            define,
            get_setting,
            set_setting,
            llm_providers,
            llm_models,
            get_llm_settings,
            set_llm_settings,
            chat_send,
            chat_history,
            tts_voices,
            voice_catalog,
            install_voice,
            daily_accuracy,
            kind_accuracy,
            due_forecast,
            weak_entries,
            export_cards,
            backup_db,
            get_voice,
            set_voice,
            speak,
            list_cards,
            delete_custom_entry,
            delete_card,
            import_words,
            import_article,
            reset_data,
            list_books,
            import_book,
            remote_books,
            books_index_url,
            set_books_index_url,
            install_remote_book,
            record_focus,
            activity
        ])
        .run(tauri::generate_context!())
        .expect("Tauri 启动失败");
}

fn boxed(e: impl std::fmt::Display + 'static) -> Box<dyn std::error::Error> {
    Box::new(StoreError::Corner(e.to_string()))
}
