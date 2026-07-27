use anyhow::{Context, Result};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::fmt;

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

#[derive(Debug, Clone, Serialize, Deserialize)]
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

// ---- OpenCode Provider ----

pub struct OpenCodeProvider {
    base_url: String,
    client: reqwest::Client,
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
            session_id: None,
        }
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
        let url = format!("{}/session", self.base_url);
        let resp = self
            .client
            .post(&url)
            .json(&serde_json::json!({}))
            .send()
            .await
            .context("Failed to create OpenCode session")?;
        let data: serde_json::Value = resp
            .json()
            .await
            .context("Failed to parse session response")?;
        let sid = data["id"]
            .as_str()
            .or_else(|| data["data"]["id"].as_str())
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

    pub fn base_url(&self) -> &str {
        &self.base_url
    }
}

#[async_trait]
impl Provider for OpenCodeProvider {
    async fn chat(&self, messages: &[Message], _tools: &[Tool], options: &ChatOptions) -> Result<ChatResponse> {
        let session_id = self
            .session_id
            .clone()
            .context("No OpenCode session. Call ensure_session first.")?;

        let user_msg = messages
            .iter()
            .rev()
            .find(|m| m.role == "user")
            .map(|m| m.content.as_str())
            .unwrap_or("");

        let mut body = serde_json::json!({
            "parts": [{"type": "text", "text": user_msg}]
        });
        if let (Some(pid), Some(mid)) = (options.provider_id.as_ref(), options.model_id.as_ref()) {
            body["model"] = serde_json::json!({"providerID": pid, "modelID": mid});
        }

        let url = format!("{}/session/{}/message", self.base_url, session_id);
        let resp = self
            .client
            .post(&url)
            .json(&body)
            .send()
            .await
            .context("Failed to send message to OpenCode")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("OpenCode returned {}: {}", status, body);
        }

        let data: serde_json::Value = resp
            .json()
            .await
            .context("Failed to parse message response")?;

        let parts = data["parts"].as_array().cloned().unwrap_or_default();
        let content = parts
            .iter()
            .filter_map(|p| {
                if p["type"].as_str() == Some("text") {
                    p["text"].as_str()
                } else {
                    None
                }
            })
            .collect::<Vec<_>>()
            .join("\n");

        let content = if content.is_empty() {
            "[no text response from model]".into()
        } else {
            content
        };

        Ok(ChatResponse {
            content,
            tool_calls: vec![],
            usage: TokenUsage {
                prompt_tokens: 0,
                completion_tokens: 0,
            },
        })
    }

    async fn list_models(&self) -> Result<Vec<ModelInfo>> {
        let url = format!("{}/config/providers", self.base_url);
        let resp = self
            .client
            .get(&url)
            .send()
            .await
            .context("Failed to fetch providers from OpenCode")?;
        let data: serde_json::Value = resp
            .json()
            .await
            .context("Failed to parse providers response")?;

        let mut models = vec![];
        if let Some(providers) = data["providers"].as_array() {
            for provider in providers {
                let provider_id = provider["id"].as_str().unwrap_or("unknown").to_string();
                if let Some(model_map) = provider["models"].as_object() {
                    for (model_id, info) in model_map {
                        let name = info["name"].as_str().unwrap_or(model_id);
                        let caps = &info["capabilities"];
                        let supports_tools = caps["toolcall"].as_bool().unwrap_or(false);
                        let context_window = info["limit"]["context"].as_u64().unwrap_or(0) as u32;
                        models.push(ModelInfo {
                            id: model_id.clone(),
                            name: name.to_string(),
                            provider_id: provider_id.clone(),
                            context_window,
                            supports_tools,
                            supports_streaming: true,
                        });
                    }
                }
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
