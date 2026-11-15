//! Tauri 壳层。这里只做三件事：把输入递进内核、把结果序列化回前端、
//! 管理状态。任何判分或调度逻辑都不写在这里——它属于 `ealearn-core`。

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use ealearn_core::dialog::Dialog;
use ealearn_core::entry::Token;
use ealearn_core::exercise::{Answer, Exercise, Grade};
use ealearn_core::pack::Pack;
use ealearn_core::plan::{self, Coverage};
use ealearn_core::srs::{CardState, Rating, Scheduler};
use ealearn_store::{CardKind, Stats, Store, StoreError};
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
) -> Result<Vec<Exercise>, String> {
    let packs = state.packs.lock().map_err(err)?;
    let pack = packs
        .get(&lang)
        .ok_or_else(|| format!("未加载的语言包: {lang}"))?;
    Ok(plan::plan(pack, limit.clamp(1, 40), seed))
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

/// 把 pack 里的素材读成 data URL。走这条而不是 asset 协议，省掉 scope 配置，
/// 也顺手挡住目录穿越。
#[tauri::command]
fn asset_data(
    state: State<'_, AppState>,
    lang: String,
    rel: String,
) -> Result<Option<String>, String> {
    use base64::Engine;

    let packs = state.packs.lock().map_err(err)?;
    let pack = packs
        .get(&lang)
        .ok_or_else(|| format!("未加载的语言包: {lang}"))?;

    let root = std::fs::canonicalize(&pack.root).map_err(err)?;
    let path = std::fs::canonicalize(pack.asset(&rel)).map_err(err)?;
    if !path.starts_with(&root) {
        return Err("素材路径越界".into());
    }
    let Ok(bytes) = std::fs::read(&path) else {
        return Ok(None);
    };

    let mime = match path.extension().and_then(|e| e.to_str()) {
        Some("webp") => "image/webp",
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("svg") => "image/svg+xml",
        Some("wav") => "audio/wav",
        Some("mp3") => "audio/mpeg",
        Some("ogg") => "audio/ogg",
        _ => "application/octet-stream",
    };
    Ok(Some(format!(
        "data:{mime};base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    )))
}

#[derive(Serialize)]
pub struct LlmSettingsView {
    base_url: String,
    model: String,
    has_key: bool,
    /// 只回显末 4 位，避免把 key 送到前端
    key_hint: String,
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

    let (cfg, history, slang, label) = {
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

pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let data_dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&data_dir)?;

            let store = Store::open(data_dir.join("ealearn.db")).map_err(boxed)?;
            let packs = load_packs(&packs_dir(app.handle()));

            let at = now();
            let mut map = HashMap::new();
            for pack in packs {
                let m = &pack.manifest;
                store
                    .upsert_pack(
                        &m.lang,
                        &m.display,
                        &m.version,
                        m.content_hash.as_deref(),
                        true,
                        at,
                    )
                    .map_err(boxed)?;
                map.insert(m.lang.clone(), pack);
            }

            app.manage(AppState {
                store: Mutex::new(store),
                packs: Mutex::new(map),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            list_packs,
            start_session,
            submit_answer,
            add_card,
            stats,
            due_cards,
            list_dialogs,
            get_dialog,
            lookup,
            define,
            asset_data,
            get_setting,
            set_setting,
            get_llm_settings,
            set_llm_settings,
            chat_send,
            chat_history
        ])
        .run(tauri::generate_context!())
        .expect("Tauri 启动失败");
}

fn boxed(e: impl std::fmt::Display + 'static) -> Box<dyn std::error::Error> {
    Box::new(StoreError::Corner(e.to_string()))
}
