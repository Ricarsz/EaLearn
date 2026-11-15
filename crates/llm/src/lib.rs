//! 兼容 OpenAI 协议的流式对话客户端。
//!
//! 三块纯逻辑与网络解耦，可以脱离 HTTP 单测：
//! - [`sse`]：增量 SSE 帧解析
//! - [`reply`]：从流式 JSON 里提前抽出 `reply` 字段的内容（边到边显示）
//! - [`turn`]：整段回复的结构化解析与标注校验
//!
//! 换供应商只改 [`LlmConfig::base_url`]，换成自建代理也一样——代理只需兼容
//! 同一套 `/chat/completions` 协议。

pub mod client;
pub mod prompt;
pub mod reply;
pub mod sse;
pub mod turn;

pub use client::{LlmClient, LlmConfig, LlmError};
pub use prompt::{ChatMessage, PromptContext};
pub use turn::{Annotation, Correction, TutorTurn};