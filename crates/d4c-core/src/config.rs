use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlobalConfig {
    pub provider: ProviderConfig,
    pub model: ModelPreferences,
    pub ui: UiConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    pub default_provider: String,
    pub api_key: Option<String>,
    pub base_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelPreferences {
    pub default_model: Option<String>,
    pub router_enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiConfig {
    pub theme: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectConfig {
    pub mcp_servers: Vec<McpServerConfig>,
    pub model_override: Option<String>,
    pub permissions: PermissionConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerConfig {
    pub name: String,
    pub command: String,
    pub args: Vec<String>,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionConfig {
    pub file_write: PermissionLevel,
    pub shell_exec: PermissionLevel,
    pub network: PermissionLevel,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum PermissionLevel {
    Ask,
    AllowAlways,
    Deny,
}

impl Default for GlobalConfig {
    fn default() -> Self {
        Self {
            provider: ProviderConfig {
                default_provider: "opencode".into(),
                api_key: None,
                base_url: None,
            },
            model: ModelPreferences {
                default_model: None,
                router_enabled: true,
            },
            ui: UiConfig {
                theme: "default".into(),
            },
        }
    }
}

impl Default for ProjectConfig {
    fn default() -> Self {
        Self {
            mcp_servers: Vec::new(),
            model_override: None,
            permissions: PermissionConfig {
                file_write: PermissionLevel::Ask,
                shell_exec: PermissionLevel::Ask,
                network: PermissionLevel::Ask,
            },
        }
    }
}

pub struct ConfigManager {
    global: GlobalConfig,
    project: Option<ProjectConfig>,
    global_path: PathBuf,
    #[allow(dead_code)]
    project_path: Option<PathBuf>,
}

impl ConfigManager {
    pub fn load() -> Result<Self> {
        let global_dir = dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("d4c");
        let global_path = global_dir.join("config.toml");

        let global = if global_path.exists() {
            let content = std::fs::read_to_string(&global_path)?;
            toml::from_str(&content)?
        } else {
            let config = GlobalConfig::default();
            std::fs::create_dir_all(&global_dir)?;
            let content = toml::to_string_pretty(&config)?;
            std::fs::write(&global_path, content)?;
            config
        };

        let project_path = PathBuf::from(".d4c/config.toml");
        let project = if project_path.exists() {
            let content = std::fs::read_to_string(&project_path)?;
            Some(toml::from_str(&content)?)
        } else {
            None
        };

        Ok(Self {
            global,
            project,
            global_path,
            project_path: if project_path.exists() {
                Some(project_path)
            } else {
                None
            },
        })
    }

    pub fn global(&self) -> &GlobalConfig {
        &self.global
    }

    pub fn project(&self) -> Option<&ProjectConfig> {
        self.project.as_ref()
    }

    pub fn merged_permissions(&self) -> PermissionConfig {
        self.project
            .as_ref()
            .map(|p| p.permissions.clone())
            .unwrap_or(PermissionConfig {
                file_write: PermissionLevel::Ask,
                shell_exec: PermissionLevel::Ask,
                network: PermissionLevel::Ask,
            })
    }

    pub fn save_global(&mut self, config: GlobalConfig) -> Result<()> {
        let content = toml::to_string_pretty(&config)?;
        std::fs::write(&self.global_path, content)?;
        self.global = config;
        Ok(())
    }
}
