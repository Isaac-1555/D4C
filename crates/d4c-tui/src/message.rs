use chrono::{DateTime, Local};

#[derive(Debug, Clone, PartialEq)]
pub enum Role {
    User,
    Agent,
    Tool,
    System,
    Error,
}

impl Role {
    pub fn label(&self) -> &'static str {
        match self {
            Role::User => "you",
            Role::Agent => "agent",
            Role::Tool => "tool",
            Role::System => "system",
            Role::Error => "error",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "user" | "you" => Role::User,
            "assistant" | "agent" => Role::Agent,
            "tool" => Role::Tool,
            "system" => Role::System,
            "error" | "err" => Role::Error,
            _ => Role::System,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ChatMessage {
    pub role: Role,
    pub content: String,
    pub timestamp: DateTime<Local>,
}

impl ChatMessage {
    pub fn new(role: Role, content: impl Into<String>) -> Self {
        Self {
            role,
            content: content.into(),
            timestamp: Local::now(),
        }
    }

    pub fn new_from_role_str(role: &str, content: impl Into<String>) -> Self {
        Self::new(Role::from_str(role), content)
    }
}
