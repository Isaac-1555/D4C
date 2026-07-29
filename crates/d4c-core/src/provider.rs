use anyhow::{Context, Result};
use async_trait::async_trait;
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use std::fmt;
use tokio::sync::mpsc;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tool {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatResponse {
    pub content: String,
    pub tool_calls: Vec<ToolCall>,
    pub usage: TokenUsage,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: serde_json::Value,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default)]
pub struct TokenUsage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    pub id: String,
    pub name: String,
    pub provider_id: String,
    pub context_window: u32,
    pub supports_tools: bool,
    pub supports_streaming: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderCapabilities {
    pub supports_streaming: bool,
    pub supports_tools: bool,
    pub supports_vision: bool,
}

/// Live chunks emitted by the streaming chat pipeline.
/// The TUI event loop polls these non-blockingly so the UI
/// never freezes while the LLM is producing output.
#[derive(Debug, Clone)]
pub enum StreamChunk {
    /// Lifecycle state change ("Thinking…", "Generating response…").
    Status(String),
    /// Incremental assistant text. May arrive in one shot for fast
    /// models or token-by-token for streamed ones.
    Delta(String),
    /// The turn has ended (final chunk). Carries token accounting.
    Done { tokens: TokenUsage },
    /// Generation failed or was interrupted.
    Error(String),
    /// Stream closed (always emitted last; after this the channel
    /// is empty and `recv()` returns `None`).
    Finished,
}

#[async_trait]
pub trait Provider: Send + Sync {
    async fn chat(&self, messages: &[Message], tools: &[Tool], options: &ChatOptions) -> Result<ChatResponse>;
    async fn list_models(&self) -> Result<Vec<ModelInfo>>;
    fn name(&self) -> &str;
    fn capabilities(&self) -> ProviderCapabilities;
}

// ---- Effort Level ----

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum EffortLevel {
    #[default]
    Low,
    Medium,
    High,
}

impl fmt::Display for EffortLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EffortLevel::Low => write!(f, "low"),
            EffortLevel::Medium => write!(f, "medium"),
            EffortLevel::High => write!(f, "high"),
        }
    }
}

impl EffortLevel {
    pub fn from_str(s: &str) -> Option<Self> {
        match s.trim().to_lowercase().as_str() {
            "low" => Some(Self::Low),
            "medium" | "med" => Some(Self::Medium),
            "high" => Some(Self::High),
            _ => None,
        }
    }

    pub fn from_model_name(name: &str) -> Self {
        let lower = name.to_lowercase();
        if lower.contains("reasoning")
            || lower.contains("big")
            || lower.contains("large")
            || lower.contains("sonnet")
            || lower.contains("opus")
        {
            Self::High
        } else if lower.contains("mini")
            || lower.contains("small")
            || lower.contains("fast")
            || lower.contains("haiku")
            || lower.contains("flash")
        {
            Self::Low
        } else {
            Self::Medium
        }
    }

    pub fn variants() -> &'static [EffortLevel] {
        &[EffortLevel::Low, EffortLevel::Medium, EffortLevel::High]
    }
}

// ---- Chat Options ----

#[derive(Debug, Clone, Default)]
pub struct ChatOptions {
    pub provider_id: Option<String>,
    pub model_id: Option<String>,
}

// ---- OpenCode V2 Provider ----
//
// Talks to the opencode server over the V2 HTTP API:
//   POST /api/session                        create session
//   POST /api/session/{id}/prompt           admit user prompt (non-blocking)
//   GET  /api/session/{id}/event?after=seq  SSE stream of generation events
//   POST /api/session/{id}/interrupt         cancel in-flight generation
//   GET  /api/model                          list available models
//   GET  /global/health                      liveness check
//
// The blocking `chat()` trait method is implemented on top of
// `chat_stream()` — it drains the stream to completion. The TUI
// uses `chat_stream()` directly so it can render incremental
// status updates and stay responsive while the model thinks.

pub struct OpenCodeProvider {
    base_url: String,
    client: reqwest::Client,
    /// Separate client without a request timeout for the long-lived
    /// SSE connection. The plain `client` is used for everything else
    /// (and keeps the 30s timeout to surface dead servers quickly).
    stream_client: reqwest::Client,
    session_id: Option<String>,
}

impl OpenCodeProvider {
    pub fn new(base_url: Option<String>) -> Self {
        Self {
            base_url: base_url.unwrap_or_else(|| "http://127.0.0.1:4096".into()),
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .unwrap_or_default(),
            stream_client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(600))
                .build()
                .unwrap_or_default(),
            session_id: None,
        }
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    pub async fn check_health(&self) -> Result<bool> {
        let url = format!("{}/global/health", self.base_url);
        match self.client.get(&url).send().await {
            Ok(resp) => Ok(resp.status().is_success()),
            Err(_) => Ok(false),
        }
    }

    pub async fn ensure_session(&mut self) -> Result<String> {
        if let Some(ref sid) = self.session_id {
            return Ok(sid.clone());
        }
        let url = format!("{}/api/session", self.base_url);
        let resp = self
            .client
            .post(&url)
            .json(&serde_json::json!({}))
            .send()
            .await
            .context("Failed to create OpenCode session")?;
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("OpenCode session create returned {}: {}", status, body);
        }
        let data: serde_json::Value = resp
            .json()
            .await
            .context("Failed to parse session response")?;
        let sid = data["data"]["id"]
            .as_str()
            .or_else(|| data["id"].as_str())
            .unwrap_or("")
            .to_string();
        if sid.is_empty() {
            anyhow::bail!("OpenCode returned empty session ID");
        }
        self.session_id = Some(sid.clone());
        tracing::info!("Created OpenCode session: {}", sid);
        Ok(sid)
    }

    pub fn reset_session(&mut self) {
        self.session_id = None;
    }

    /// Cancel any in-flight generation for the current session.
    pub async fn interrupt(&self) -> Result<()> {
        let session_id = self
            .session_id
            .as_ref()
            .context("No session; cannot interrupt")?;
        let url = format!("{}/api/session/{}/interrupt", self.base_url, session_id);
        let resp = self.client.post(&url).send().await?;
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("interrupt returned {}: {}", status, body);
        }
        Ok(())
    }

    /// Non-blocking streaming chat against the primary session. Returns a
    /// channel receiver that the UI polls on its own cadence. Each
    /// `StreamChunk` describes one lifecycle event: a status update, an
    /// incremental text delta, a completion marker, an error, or the
    /// terminal `Finished` event.
    pub async fn chat_stream(
        &self,
        messages: &[Message],
        options: &ChatOptions,
    ) -> Result<mpsc::Receiver<StreamChunk>> {
        let session_id = self
            .session_id
            .clone()
            .context("No OpenCode session. Call ensure_session first.")?;
        self.stream_for_session(&session_id, messages, options).await
    }

    /// One-shot chat in a throwaway session. Creates a fresh session,
    /// sends a single prompt, drains the stream to completion, and
    /// returns the final text. The temp session is abandoned (server GC
    /// reaps it). The primary `session_id` is untouched, so one-shot
    /// prompts (like /plan synthesis) don't pollute the main chat
    /// transcript with "return only JSON" instructions.
    pub async fn chat_one_shot(
        &self,
        messages: &[Message],
        options: &ChatOptions,
    ) -> Result<ChatResponse> {
        let temp_sid = self.create_session().await?;
        let mut rx = self.stream_for_session(&temp_sid, messages, options).await?;
        let mut full_text = String::new();
        let mut tokens = TokenUsage {
            prompt_tokens: 0,
            completion_tokens: 0,
        };
        while let Some(chunk) = rx.recv().await {
            match chunk {
                StreamChunk::Delta(t) => full_text.push_str(&t),
                StreamChunk::Done { tokens: tk } => tokens = tk,
                StreamChunk::Error(e) => anyhow::bail!(e),
                StreamChunk::Finished => break,
                StreamChunk::Status(_) => {}
            }
        }
        let content = if full_text.is_empty() {
            "[no text response from model]".into()
        } else {
            full_text
        };
        Ok(ChatResponse {
            content,
            tool_calls: vec![],
            usage: tokens,
        })
    }

    /// Create a session without storing it on `self`. Used by
    /// `chat_one_shot()` to isolate one-off prompts from the primary
    /// chat session.
    async fn create_session(&self) -> Result<String> {
        let url = format!("{}/api/session", self.base_url);
        let resp = self
            .client
            .post(&url)
            .json(&serde_json::json!({}))
            .send()
            .await
            .context("Failed to create temp session")?;
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("temp session create returned {}: {}", status, body);
        }
        let data: serde_json::Value = resp
            .json()
            .await
            .context("Failed to parse temp session response")?;
        let sid = data["data"]["id"]
            .as_str()
            .or_else(|| data["id"].as_str())
            .unwrap_or("")
            .to_string();
        if sid.is_empty() {
            anyhow::bail!("temp session returned empty ID");
        }
        Ok(sid)
    }

    /// Core streaming logic parameterized over a session id. Shared by
    /// `chat_stream()` (primary session) and `chat_one_shot()` (temp
    /// session). Sends a prompt and spawns a task that parses the SSE
    /// event stream into `StreamChunk`s.
    async fn stream_for_session(
        &self,
        session_id: &str,
        messages: &[Message],
        options: &ChatOptions,
    ) -> Result<mpsc::Receiver<StreamChunk>> {
        // Pull the last user message — opencode's prompt endpoint takes
        // a single text part (not full conversation history). The server
        // keeps the running transcript on its side.
        let user_msg = messages
            .iter()
            .rev()
            .find(|m| m.role == "user")
            .map(|m| m.content.as_str())
            .unwrap_or("");

        let mut prompt = serde_json::json!({ "text": user_msg });
        if let (Some(pid), Some(mid)) = (options.provider_id.as_ref(), options.model_id.as_ref())
        {
            prompt["model"] = serde_json::json!({ "providerID": pid, "modelID": mid });
        }

        let prompt_url = format!("{}/api/session/{}/prompt", self.base_url, session_id);
        let resp = self
            .client
            .post(&prompt_url)
            .json(&serde_json::json!({ "prompt": prompt }))
            .send()
            .await
            .context("Failed to send prompt to OpenCode")?;
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("OpenCode prompt returned {}: {}", status, body);
        }
        let resp_data: serde_json::Value = resp
            .json()
            .await
            .context("Failed to parse prompt admission response")?;
        let admitted_seq = resp_data["data"]["admittedSeq"]
            .as_u64()
            .unwrap_or(0)
            .saturating_sub(1);

        let event_url = format!(
            "{}/api/session/{}/event?after={}",
            self.base_url, session_id, admitted_seq
        );
        let (tx, rx) = mpsc::channel::<StreamChunk>(64);
        let stream_client = self.stream_client.clone();
        let history_client = self.client.clone();
        let history_base = self.base_url.clone();
        let hist_sid = session_id.to_string();

        tokio::spawn(async move {
            let resp = match stream_client
                .get(&event_url)
                .header("Accept", "text/event-stream")
                .send()
                .await
            {
                Ok(r) => r,
                Err(e) => {
                    let _ = tx
                        .send(StreamChunk::Error(format!("SSE connect failed: {}", e)))
                        .await;
                    let _ = tx.send(StreamChunk::Finished).await;
                    return;
                }
            };

            let mut stream = resp.bytes_stream();
            let mut buf = String::new();
            let mut text_sent = false;

            while let Some(chunk_res) = stream.next().await {
                let chunk = match chunk_res {
                    Ok(c) => c,
                    Err(e) => {
                        let _ = tx
                            .send(StreamChunk::Error(format!("SSE read: {}", e)))
                            .await;
                        break;
                    }
                };
                // Normalize \r\n → \n so the \n\n delimiter search works
                // regardless of the server's line-ending convention.
                buf.push_str(&String::from_utf8_lossy(&chunk).replace("\r\n", "\n"));

                // SSE events are separated by a blank line ("\n\n").
                // Each event block may contain one or more `data:` lines
                // that we concatenate into a single JSON payload.
                while let Some(idx) = buf.find("\n\n") {
                    let event_block: String = buf[..idx].to_string();
                    buf.drain(..idx + 2);

                    let data: String = event_block
                        .lines()
                        .filter_map(|l| {
                            l.strip_prefix("data:").map(|s| s.strip_prefix(' ').unwrap_or(s))
                        })
                        .collect::<Vec<_>>()
                        .join("");
                    if data.is_empty() {
                        continue;
                    }

                    let parsed = match serde_json::from_str::<serde_json::Value>(&data) {
                        Ok(v) => v,
                        Err(_) => continue,
                    };

                    let event_type = parsed["type"].as_str().unwrap_or("");
                    let d = &parsed["data"];

                    match event_type {
                        "session.next.prompt.admitted" | "session.next.prompted" => {
                            let _ = tx.send(StreamChunk::Status("Thinking…".into())).await;
                        }
                        "session.next.step.started" => {
                            let _ = tx
                                .send(StreamChunk::Status("Generating response…".into()))
                                .await;
                        }
                        // Streaming models send incremental text deltas.
                        "session.next.text.delta" => {
                            let delta = d["text"].as_str().or_else(|| d["delta"].as_str());
                            if let Some(text) = delta {
                                if !text.is_empty() {
                                    text_sent = true;
                                    let _ = tx.send(StreamChunk::Delta(text.to_string())).await;
                                }
                            }
                        }
                        // Non-streaming models deliver the full text in one shot.
                        "session.next.text.ended" => {
                            // If we already streamed deltas, don't duplicate
                            // the full text from text.ended.
                            if !text_sent {
                                if let Some(text) = d["text"].as_str() {
                                    if !text.is_empty() {
                                        text_sent = true;
                                        let _ = tx.send(StreamChunk::Delta(text.to_string())).await;
                                    }
                                }
                            }
                        }
                        "session.next.step.ended" => {
                            let tokens = TokenUsage {
                                prompt_tokens: d["tokens"]["input"].as_u64().unwrap_or(0)
                                    as u32,
                                completion_tokens: d["tokens"]["output"].as_u64().unwrap_or(0)
                                    as u32,
                            };
                            // Safety net: if the SSE stream missed the text.ended
                            // event entirely (race on first message, network hiccup,
                            // etc.), fetch the response from the session history
                            // endpoint so we don't lose the model's output.
                            if !text_sent {
                                tracing::warn!("SSE stream ended without any text; fetching history");
                                let history_url = format!(
                                    "{}/api/session/{}/history?limit=20",
                                    history_base, hist_sid
                                );
                                if let Ok(resp) = history_client.get(&history_url).send().await {
                                    if let Ok(data) = resp.json::<serde_json::Value>().await {
                                        if let Some(events) = data["data"].as_array() {
                                            for evt in events.iter().rev() {
                                                if evt["type"].as_str()
                                                    == Some("session.next.text.ended")
                                                {
                                                    if let Some(t) = evt["data"]["text"].as_str() {
                                                        if !t.is_empty() {
                                                            let _ = tx
                                                                .send(StreamChunk::Delta(
                                                                    t.to_string(),
                                                                ))
                                                                .await;
                                                            break;
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                            let _ = tx.send(StreamChunk::Done { tokens }).await;
                            let _ = tx.send(StreamChunk::Finished).await;
                            return;
                        }
                        "session.next.step.failed" => {
                            let _ = tx
                                .send(StreamChunk::Error("Generation interrupted".into()))
                                .await;
                            let _ = tx.send(StreamChunk::Finished).await;
                            return;
                        }
                        _ => {}
                    }
                }
            }

            // Stream closed without an explicit `step.ended` (server shutdown,
            // HTTP timeout, etc.). Try the history fallback before giving up.
            if !text_sent {
                tracing::warn!("SSE stream closed early without text; fetching history");
                let history_url = format!(
                    "{}/api/session/{}/history?limit=20",
                    history_base, hist_sid
                );
                if let Ok(resp) = history_client.get(&history_url).send().await {
                    if let Ok(data) = resp.json::<serde_json::Value>().await {
                        if let Some(events) = data["data"].as_array() {
                            for evt in events.iter().rev() {
                                if evt["type"].as_str() == Some("session.next.text.ended") {
                                    if let Some(t) = evt["data"]["text"].as_str() {
                                        if !t.is_empty() {
                                            let _ = tx
                                                .send(StreamChunk::Delta(t.to_string()))
                                                .await;
                                            break;
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            let _ = tx.send(StreamChunk::Finished).await;
        });

        Ok(rx)
    }
}

#[async_trait]
impl Provider for OpenCodeProvider {
    /// Blocking convenience wrapper around `chat_stream()`. Used by
    /// `BuildEngine` and `/plan` generation, where the caller already
    /// runs inside an async runtime and just wants the final text.
    async fn chat(
        &self,
        messages: &[Message],
        _tools: &[Tool],
        options: &ChatOptions,
    ) -> Result<ChatResponse> {
        let mut rx = self.chat_stream(messages, options).await?;
        let mut full_text = String::new();
        let mut tokens = TokenUsage {
            prompt_tokens: 0,
            completion_tokens: 0,
        };

        while let Some(chunk) = rx.recv().await {
            match chunk {
                StreamChunk::Delta(t) => full_text.push_str(&t),
                StreamChunk::Done { tokens: tk } => tokens = tk,
                StreamChunk::Error(e) => anyhow::bail!(e),
                StreamChunk::Finished => break,
                StreamChunk::Status(_) => {}
            }
        }

        let content = if full_text.is_empty() {
            "[no text response from model]".into()
        } else {
            full_text
        };

        Ok(ChatResponse {
            content,
            tool_calls: vec![],
            usage: tokens,
        })
    }

    async fn list_models(&self) -> Result<Vec<ModelInfo>> {
        let url = format!("{}/api/model", self.base_url);
        let resp = self
            .client
            .get(&url)
            .send()
            .await
            .context("Failed to fetch models from OpenCode")?;
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("model list returned {}: {}", status, body);
        }
        let data: serde_json::Value = resp
            .json()
            .await
            .context("Failed to parse models response")?;

        let mut models = Vec::new();
        if let Some(arr) = data["data"].as_array() {
            for m in arr {
                let id = m["id"].as_str().unwrap_or("").to_string();
                if id.is_empty() {
                    continue;
                }
                let provider_id = m["providerID"].as_str().unwrap_or("unknown").to_string();
                let name = m["name"].as_str().unwrap_or(&id).to_string();
                let supports_tools = m["capabilities"]["tools"].as_bool().unwrap_or(false);
                let context_window = m["limit"]["context"].as_u64().unwrap_or(0) as u32;
                models.push(ModelInfo {
                    id,
                    name,
                    provider_id,
                    context_window,
                    supports_tools,
                    supports_streaming: true,
                });
            }
        }
        Ok(models)
    }

    fn name(&self) -> &str {
        "opencode"
    }

    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities {
            supports_streaming: true,
            supports_tools: true,
            supports_vision: false,
        }
    }
}