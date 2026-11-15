use std::time::Duration;

use futures_util::StreamExt;
use serde_json::{Value, json};

use crate::prompt::ChatMessage;
use crate::reply::ReplyTracker;
use crate::sse::{SseEvent, SseParser};
use crate::turn::{TutorTurn, TurnError, parse_turn};

/// 任何兼容 OpenAI `/chat/completions` 的服务都能用：官方、兼容网关、自建代理、
/// 本地 ollama 都只是 `base_url` 不同。
#[derive(Debug, Clone)]
pub struct LlmConfig {
    pub base_url: String,
    pub api_key: String,
    pub model: String,
    pub temperature: f32,
    pub timeout_secs: u64,
}

impl LlmConfig {
    pub fn openai(api_key: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            base_url: "https://api.openai.com/v1".into(),
            api_key: api_key.into(),
            model: model.into(),
            temperature: 0.8,
            timeout_secs: 60,
        }
    }

    /// 本地 ollama 之类的无鉴权端点。
    pub fn local(base_url: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
            api_key: String::new(),
            model: model.into(),
            temperature: 0.8,
            timeout_secs: 120,
        }
    }

    fn endpoint(&self) -> String {
        format!("{}/chat/completions", self.base_url.trim_end_matches('/'))
    }
}

#[derive(Debug)]
pub enum LlmError {
    Http(String),
    Status(u16, String),
    Stream(String),
    Turn(TurnError),
    NotConfigured,
}

impl std::fmt::Display for LlmError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LlmError::Http(e) => write!(f, "请求失败: {e}"),
            LlmError::Status(code, body) => write!(f, "服务返回 {code}: {body}"),
            LlmError::Stream(e) => write!(f, "流式读取失败: {e}"),
            LlmError::Turn(e) => write!(f, "{e}"),
            LlmError::NotConfigured => write!(f, "还没有配置 API key"),
        }
    }
}

impl std::error::Error for LlmError {}

pub struct LlmClient {
    http: reqwest::Client,
    cfg: LlmConfig,
}

impl LlmClient {
    pub fn new(cfg: LlmConfig) -> Result<Self, LlmError> {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(cfg.timeout_secs))
            .build()
            .map_err(|e| LlmError::Http(e.to_string()))?;
        Ok(Self { http, cfg })
    }

    pub fn config(&self) -> &LlmConfig {
        &self.cfg
    }

    /// 发起一轮流式对话。
    ///
    /// `on_delta` 每收到 `reply` 正文的新增片段就被调用一次——界面据此做打字效果，
    /// 不必等整段 JSON 生成完。返回的是解析并校验过的完整回合。
    pub async fn chat<F>(
        &self,
        messages: &[ChatMessage],
        mut on_delta: F,
    ) -> Result<TutorTurn, LlmError>
    where
        F: FnMut(&str) + Send,
    {
        if self.cfg.api_key.is_empty() && !self.cfg.base_url.contains("localhost") {
            return Err(LlmError::NotConfigured);
        }

        let body = json!({
            "model": self.cfg.model,
            "messages": messages,
            "temperature": self.cfg.temperature,
            "stream": true,
        });

        let mut req = self.http.post(self.cfg.endpoint()).json(&body);
        if !self.cfg.api_key.is_empty() {
            req = req.bearer_auth(&self.cfg.api_key);
        }

        let resp = req.send().await.map_err(|e| LlmError::Http(e.to_string()))?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(LlmError::Status(status.as_u16(), truncate(&body, 400)));
        }

        let mut stream = resp.bytes_stream();
        let mut parser = SseParser::new();
        let mut tracker = ReplyTracker::default();
        let mut accumulated = String::new();
        let mut finished = false;

        while let Some(chunk) = stream.next().await {
            let bytes = chunk.map_err(|e| LlmError::Stream(e.to_string()))?;
            let text = String::from_utf8_lossy(&bytes);
            for event in parser.push(&text) {
                match event {
                    SseEvent::Done => {
                        finished = true;
                        break;
                    }
                    SseEvent::Data(payload) => {
                        if let Some(delta) = delta_content(&payload) {
                            accumulated.push_str(&delta);
                            if let Some(new) = tracker.on_text(&accumulated) {
                                on_delta(&new);
                            }
                        }
                    }
                }
            }
            if finished {
                break;
            }
        }

        if !finished {
            for event in parser.finish() {
                if let SseEvent::Data(payload) = event {
                    if let Some(delta) = delta_content(&payload) {
                        accumulated.push_str(&delta);
                        if let Some(new) = tracker.on_text(&accumulated) {
                            on_delta(&new);
                        }
                    }
                }
            }
        }

        parse_turn(&accumulated).map_err(LlmError::Turn)
    }
}

impl LlmClient {
    /// 拉取模型列表（`GET {base}/models`）。国内厂商基本都兼容这个端点，
    /// 比在代码里硬编码模型名可靠得多。
    pub async fn list_models(
        base_url: &str,
        api_key: &str,
    ) -> Result<Vec<String>, LlmError> {
        let url = format!("{}/models", base_url.trim_end_matches('/'));
        let mut req = reqwest::Client::builder()
            .timeout(Duration::from_secs(20))
            .build()
            .map_err(|e| LlmError::Http(e.to_string()))?
            .get(url);
        if !api_key.is_empty() {
            req = req.bearer_auth(api_key);
        }
        let resp = req.send().await.map_err(|e| LlmError::Http(e.to_string()))?;
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        if !status.is_success() {
            return Err(LlmError::Status(status.as_u16(), truncate(&body, 300)));
        }
        let v: Value = serde_json::from_str(&body).map_err(|e| LlmError::Http(e.to_string()))?;
        let mut ids: Vec<String> = v
            .get("data")
            .and_then(|d| d.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|m| m.get("id").and_then(|i| i.as_str()).map(str::to_string))
                    .collect()
            })
            .unwrap_or_default();
        ids.sort();
        Ok(ids)
    }
}

/// 从一帧 `data:` 负载里取出 `choices[0].delta.content`。
pub fn delta_content(payload: &str) -> Option<String> {
    let v: Value = serde_json::from_str(payload).ok()?;
    let content = v.get("choices")?.get(0)?.get("delta")?.get("content")?;
    content.as_str().map(str::to_string)
}

fn truncate(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        return s.to_string();
    }
    s.chars().take(n).collect::<String>() + "…"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_delta_content() {
        let frame = r#"{"choices":[{"delta":{"content":"你好"}}]}"#;
        assert_eq!(delta_content(frame).as_deref(), Some("你好"));
    }

    #[test]
    fn ignores_frames_without_content() {
        assert_eq!(delta_content(r#"{"choices":[{"delta":{}}]}"#), None);
        assert_eq!(delta_content(r#"{"choices":[]}"#), None);
        assert_eq!(delta_content("not json"), None);
    }

    #[test]
    fn endpoint_join_is_idempotent() {
        let cfg = LlmConfig::openai("k", "m");
        assert_eq!(cfg.endpoint(), "https://api.openai.com/v1/chat/completions");
        let mut trailing = cfg.clone();
        trailing.base_url = "https://x.test/v1/".into();
        assert_eq!(trailing.endpoint(), "https://x.test/v1/chat/completions");
    }

    #[test]
    fn stream_simulation_produces_turn() {
        // 把一整段 SSE 切成任意碎片喂给解析器，应当与整段一次喂入等价
        let json = r#"{"reply":"She ghosted him.","translation":"她把他玩消失了。","annotations":[{"surface":"ghosted","kind":"slang","gloss":"玩消失"}],"difficulty":2}"#;
        let mut sse = String::new();
        for piece in json.as_bytes().chunks(7) {
            let text = String::from_utf8_lossy(piece).to_string();
            let frame = serde_json::json!({"choices":[{"delta":{"content": text}}]}).to_string();
            sse.push_str(&format!("data: {frame}\n\n"));
        }
        sse.push_str("data: [DONE]\n\n");

        let mut parser = SseParser::new();
        let mut tracker = ReplyTracker::default();
        let mut acc = String::new();
        for event in parser.push(&sse) {
            if let SseEvent::Data(p) = event {
                if let Some(d) = delta_content(&p) {
                    acc.push_str(&d);
                    let _ = tracker.on_text(&acc);
                }
            }
        }
        assert_eq!(tracker.text(), "She ghosted him.");

        let turn = parse_turn(&acc).unwrap();
        assert_eq!(turn.annotations.len(), 1);
        assert_eq!(turn.difficulty, Some(2));
    }
}