use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum PermissionLevel {
    Ask,
    AllowOnce,
    AllowAlways,
    Deny,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum PermissionAction {
    FileWrite,
    FileRead,
    ShellExec,
    NetworkAccess,
}

impl std::fmt::Display for PermissionAction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PermissionAction::FileWrite => write!(f, "file write"),
            PermissionAction::FileRead => write!(f, "file read"),
            PermissionAction::ShellExec => write!(f, "shell exec"),
            PermissionAction::NetworkAccess => write!(f, "network access"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionRequest {
    pub action: PermissionAction,
    pub target: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionRule {
    pub action: PermissionAction,
    pub pattern: String,
    pub level: PermissionLevel,
}

pub struct PermissionManager {
    rules: Vec<PermissionRule>,
    session_overrides: HashMap<String, PermissionLevel>,
    rules_path: Option<PathBuf>,
}

impl PermissionManager {
    pub fn new() -> Self {
        let rules_path = std::env::current_dir()
            .ok()
            .map(|d| d.join(".d4c").join("permissions.json"));

        let mut manager = Self {
            rules: Vec::new(),
            session_overrides: HashMap::new(),
            rules_path,
        };

        if let Some(path) = &manager.rules_path {
            if path.exists() {
                if let Ok(content) = std::fs::read_to_string(path) {
                    if let Ok(rules) = serde_json::from_str(&content) {
                        manager.rules = rules;
                    }
                }
            }
        }

        manager
    }

    pub fn check_permission(&self, request: &PermissionRequest) -> PermissionLevel {
        let key = format!("{:?}:{}", request.action, request.target);

        if let Some(level) = self.session_overrides.get(&key) {
            return *level;
        }

        for rule in &self.rules {
            if rule.action == request.action && matches_pattern(&rule.pattern, &request.target) {
                return rule.level;
            }
        }

        PermissionLevel::Ask
    }

    pub fn grant_session(&mut self, request: &PermissionRequest, level: PermissionLevel) {
        let key = format!("{:?}:{}", request.action, request.target);
        self.session_overrides.insert(key, level);
    }

    pub fn save_rule(&mut self, rule: PermissionRule) -> Result<()> {
        self.rules.push(rule);
        if let Some(path) = &self.rules_path {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let content = serde_json::to_string_pretty(&self.rules)?;
            std::fs::write(path, content)?;
        }
        Ok(())
    }

    pub fn list_rules(&self) -> &[PermissionRule] {
        &self.rules
    }
}

fn matches_pattern(pattern: &str, target: &str) -> bool {
    if pattern == "*" {
        return true;
    }
    if pattern.ends_with("/**") {
        let prefix = &pattern[..pattern.len() - 3];
        return target.starts_with(prefix);
    }
    if pattern.contains('*') {
        let parts: Vec<&str> = pattern.split('*').collect();
        let mut remaining = target;
        for part in parts {
            if let Some(pos) = remaining.find(part) {
                remaining = &remaining[pos + part.len()..];
            } else {
                return false;
            }
        }
        return true;
    }
    pattern == target
}
