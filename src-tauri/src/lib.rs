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

    Ok(store
        .cards(&lang)
        .map_err(err)?
        .into_iter()
        .map(|c| {
            let entry = pack.entry(&c.entry_id);
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

    let card_ids: Vec<String> = stored.iter().map(|(id, _)| id.clone()).collect();
    store
        .bulk_add_cards(lang, &card_ids, CardKind::Word, at)
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
    name: String,
    url: String,
    #[serde(default)]
    count: usize,
    #[serde(default)]
    license: String,
    #[serde(default)]
    note: String,
}

#[derive(serde::Deserialize)]
struct BooksIndex {
    #[serde(default)]
    books: Vec<RemoteBook>,
}

/// 在线单词书索引。索引只是一个 JSON 文件，加书不用发版。
#[tauri::command]
async fn remote_books(
    state: State<'_, AppState>,
    index_url: Option<String>,
) -> Result<Vec<RemoteBook>, String> {
    let url = match index_url.filter(|u| !u.trim().is_empty()) {
        Some(u) => u,
        None => {
            let store = state.store.lock().map_err(err)?;
            store
                .get_setting(KEY_BOOKS_INDEX)
                .map_err(err)?
                .filter(|u| !u.trim().is_empty())
                .unwrap_or_else(|| DEFAULT_BOOKS_INDEX.to_string())
        }
    };

    let resp = reqwest::Client::new()
        .get(&url)
        .timeout(std::time::Duration::from_secs(20))
        .send()
        .await
        .map_err(|e| format!("拉取索引失败: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("索引返回 {}", resp.status()));
    }
    let index: BooksIndex = resp.json().await.map_err(|e| format!("解析索引失败: {e}"))?;
    Ok(index.books)
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
    state: State<'_, AppState>,
    lang: String,
    name: String,
    url: String,
) -> Result<ImportResult, String> {
    let resp = reqwest::Client::new()
        .get(&url)
        .timeout(std::time::Duration::from_secs(60))
        .send()
        .await
        .map_err(|e| format!("下载失败: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("下载返回 {}", resp.status()));
    }
    let text = resp.text().await.map_err(|e| format!("读取失败: {e}"))?;
    do_import(&state, &lang, &text, &name)
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

pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let data_dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&data_dir)?;

            let store = Store::open(data_dir.join("ealearn.db")).map_err(boxed)?;
            let packs = load_packs(&packs_dir(app.handle()));

            let at = now();
            let mut map = HashMap::new();
            for mut pack in packs {
                let lang = pack.manifest.lang.clone();
                let display = pack.manifest.display.clone();
                let version = pack.manifest.version.clone();
                let hash = pack.manifest.content_hash.clone();
                store
                    .upsert_pack(&lang, &display, &version, hash.as_deref(), true, at)
                    .map_err(boxed)?;

                if let Some(profile) = ealearn_lang::profile(&lang) {
                    for c in store.custom_entries(&lang).map_err(boxed)? {
                        let mut entry = ealearn_core::entry::Entry::new(&c.id, &c.surface);
                        entry.gloss = c.gloss.clone();
                        entry.tags = c.tags.clone();
                        entry.tokens = profile.tokenize(&c.surface);
                        pack.words.push(entry);
                    }
                }
                map.insert(lang, pack);
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
            get_setting,
            set_setting,
            llm_providers,
            llm_models,
            get_llm_settings,
            set_llm_settings,
            chat_send,
            chat_history,
            tts_voices,
            get_voice,
            set_voice,
            speak,
            list_cards,
            delete_custom_entry,
            delete_card,
            import_words,
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
