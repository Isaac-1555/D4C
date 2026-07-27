use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub messages: Vec<Message>,
    pub status: SessionStatus,
    pub active_plan: Option<Plan>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum SessionStatus {
    Active,
    Completed,
    Interrupted,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: Role,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum Role {
    User,
    Assistant,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Plan {
    pub id: String,
    pub steps: Vec<PlanStep>,
    pub status: PlanStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanStep {
    pub description: String,
    pub files: Vec<PathBuf>,
    pub completed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum PlanStatus {
    Draft,
    Approved,
    InProgress,
    Completed,
}

pub struct SessionManager {
    sessions_dir: PathBuf,
}

impl SessionManager {
    pub fn new() -> Result<Self> {
        let sessions_dir = dirs::data_local_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("d4c")
            .join("sessions");
        std::fs::create_dir_all(&sessions_dir)?;
        Ok(Self { sessions_dir })
    }

    pub fn save(&self, session: &Session) -> Result<()> {
        let path = self.sessions_dir.join(format!("{}.json", session.id));
        let content = serde_json::to_string_pretty(session)?;
        std::fs::write(path, content)?;
        Ok(())
    }

    pub fn load(&self, id: &str) -> Result<Session> {
        let path = self.sessions_dir.join(format!("{}.json", id));
        let content = std::fs::read_to_string(path)?;
        Ok(serde_json::from_str(&content)?)
    }

    pub fn list(&self) -> Result<Vec<Session>> {
        let mut sessions = Vec::new();
        for entry in std::fs::read_dir(&self.sessions_dir)? {
            let entry = entry?;
            if entry.path().extension().map_or(false, |e| e == "json") {
                let content = std::fs::read_to_string(entry.path())?;
                let session: Session = serde_json::from_str(&content)?;
                sessions.push(session);
            }
        }
        sessions.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        Ok(sessions)
    }

    pub fn incomplete_sessions(&self) -> Result<Vec<Session>> {
        Ok(self
            .list()?
            .into_iter()
            .filter(|s| s.status == SessionStatus::Interrupted)
            .collect())
    }

    pub fn new_session() -> Session {
        Session {
            id: uuid::Uuid::new_v4().to_string(),
            created_at: chrono::Utc::now(),
            messages: Vec::new(),
            status: SessionStatus::Active,
            active_plan: None,
        }
    }
}
