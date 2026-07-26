use anyhow::Result;
use std::collections::HashMap;
use std::path::PathBuf;

use crate::indexer::RepoIndex;
use crate::plan::PlanManager;
use crate::session::SessionManager;

pub struct CommandRegistry {
    handlers: HashMap<String, Box<dyn Fn(&str) -> Result<String> + Send + Sync>>,
}

impl CommandRegistry {
    pub fn new() -> Self {
        let mut registry = Self {
            handlers: HashMap::new(),
        };

        registry.register("help", |args| {
            if args.is_empty() {
                Ok("Available commands:\n  /help    [cmd]  - Show help (or help for specific command)\n  /new            - Start new session\n  /clear          - Clear conversation context\n  /quit           - Exit d4c\n  /model   [name] - List or pin a model\n  /history        - Browse past sessions\n  /config         - View/edit configuration\n  /login          - Connect to a provider\n  /review         - Review diff, plan, or past output\n  /plan    <task> - Start interactive planning\n  /build          - Execute an approved plan\n\nKeys: Ctrl+E  cycle effort (low/medium/high)".into())
            } else {
                match args {
                    "plan" => Ok("/plan <task> - Interactive planning workflow.\n  Scans repo, generates questionnaire,\n  surfaces assumptions, produces plan\n  for approval.".into()),
                    "build" => Ok("/build - Execute approved plan with\n  checkpoints. Pauses after each step\n  for review/abort.".into()),
                    _ => Ok(format!("No detailed help for /{}", args)),
                }
            }
        });

        registry.register("new", |_args| Ok("__NEW_SESSION__".into()));
        registry.register("clear", |_args| Ok("Context cleared.".into()));
        registry.register("quit", |_args| Ok("__QUIT__".into()));

        registry.register("history", |_args| match SessionManager::new() {
            Ok(mgr) => match mgr.list() {
                Ok(sessions) => {
                    if sessions.is_empty() {
                        Ok("No past sessions.".into())
                    } else {
                        let mut out = String::from("Past sessions:\n");
                        for s in sessions.iter().take(20) {
                            out.push_str(&format!(
                                "  {} [{}] - {} messages\n",
                                &s.id[..8],
                                s.created_at.format("%Y-%m-%d %H:%M"),
                                s.messages.len()
                            ));
                        }
                        Ok(out)
                    }
                }
                Err(e) => Ok(format!("Error listing sessions: {}", e)),
            },
            Err(e) => Ok(format!("Session manager error: {}", e)),
        });

        registry.register("config", |args| {
            let parts: Vec<&str> = args.splitn(3, ' ').collect();
            let mut cm = match crate::config::ConfigManager::load() {
                Ok(cm) => cm,
                Err(_) => return Ok("Failed to load config.".into()),
            };

            match parts.first() {
                Some(&"set") => {
                    if parts.len() >= 3 {
                        let key = parts[1];
                        let val = parts[2];
                        match key {
                            "effort" | "effort_level" => {
                                let lower = val.to_lowercase();
                                if matches!(lower.as_str(), "low" | "medium" | "med" | "high") {
                                    let normalized = match lower.as_str() {
                                        "med" => "medium",
                                        other => other,
                                    };
                                    // Persist to config
                                    let mut g = cm.global().clone();
                                    g.model.effort = Some(normalized.to_string());
                                    let _ = cm.save_global(g);
                                    Ok(format!("__SET_EFFORT__{}", normalized))
                                } else {
                                    Ok("Invalid effort level. Use: low, medium, or high.".into())
                                }
                            }
                            "base_url" | "url" => {
                                let mut g = cm.global().clone();
                                g.provider.base_url = Some(val.to_string());
                                match cm.save_global(g) {
                                    Ok(_) => Ok(format!(
                                        "__SET_CONFIG__base_url {}",
                                        val
                                    )),
                                    Err(_) => Ok("Failed to save config.".into()),
                                }
                            }
                            _ => Ok(format!("Unknown config key: {}. Known keys: effort, base_url", key)),
                        }
                    } else {
                        Ok("Usage: /config set <key> <value>\nKeys: effort (low|medium|high), base_url <url>".into())
                    }
                }
                Some(&"get") => {
                    if parts.len() >= 2 {
                        let val = match parts[1] {
                            "effort" | "effort_level" => cm.global().model.effort.clone().unwrap_or_else(|| "auto".into()),
                            "base_url" | "url" => cm.global().provider.base_url.clone().unwrap_or_else(|| "http://127.0.0.1:4096".into()),
                            "provider" | "default_provider" => cm.global().provider.default_provider.clone(),
                            _ => "<unknown>".into(),
                        };
                        Ok(format!("{} = {}", parts[1], val))
                    } else {
                        Ok("Usage: /config get <key>".into())
                    }
                }
                Some(&"show") | None => {
                    let effort = cm.global().model.effort.as_deref().unwrap_or("auto");
                    let base_url = cm.global().provider.base_url.as_deref().unwrap_or("http://127.0.0.1:4096");
                    Ok(format!(
                        "Current config:\n  provider     = {}\n  base_url     = {}\n  effort       = {}\n  router       = {}\n\n\
                         /config set effort   <low|medium|high>\n\
                         /config set base_url <url>\n\
                         /login opencode      (connect to OpenCode)",
                        cm.global().provider.default_provider,
                        base_url,
                        effort,
                        if cm.global().model.router_enabled { "enabled" } else { "disabled" },
                    ))
                }
                _ => Ok("Usage: /config [show|set|get]".into()),
            }
        });

        registry.register("login", |args| {
            let parts: Vec<&str> = args.splitn(2, ' ').collect();
            let provider = parts.first().copied().unwrap_or("");
            if provider.is_empty() {
                return Ok("Usage: /login <provider> [url]\n  /login opencode\n  /login opencode http://localhost:4096".into());
            }
            if provider != "opencode" {
                return Ok(format!("Provider '{}' not supported yet. Use 'opencode'.", provider));
            }
            let url = parts.get(1).copied().unwrap_or("http://127.0.0.1:4096");
            // Save URL to config, then signal app to connect
            if let Ok(mut cm) = crate::config::ConfigManager::load() {
                let mut g = cm.global().clone();
                g.provider.base_url = Some(url.to_string());
                let _ = cm.save_global(g);
            }
            Ok(format!("__LOGIN__{}", url))
        });

        registry.register("review", |_args| {
            Ok("No active plan or diff to review.\nUse /plan <task> to create a plan.".into())
        });

        registry.register("plan", |args| {
            if args.is_empty() {
                return Ok("Usage: /plan <task description>\n  Example: /plan Add dark mode toggle to settings".into());
            }
            let mgr = PlanManager::new();
            let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
            let plan = match RepoIndex::scan(&cwd) {
                Ok(index) => mgr.generate_mock_plan(args, &index),
                Err(_) => mgr.create_plan(args),
            };
            Ok(format!("__PLAN_START__{}", serde_json::to_string(&plan).unwrap_or_default()))
        });

        registry.register("model", |args| {
            Ok(format!("__MODEL__{}", args))
        });

        registry.register("build", |args| {
            if args.is_empty() {
                Ok("__BUILD_CHECK__".into())
            } else {
                match args.trim() {
                    "continue" | "next" => Ok("__BUILD_NEXT__".into()),
                    "abort" | "pause" => Ok("__BUILD_ABORT__".into()),
                    _ => Ok("Usage: /build [continue|abort]\n  No args: start build from approved plan\n  continue: proceed to next checkpoint\n  abort: pause execution".into()),
                }
            }
        });

        registry
    }

    pub fn register(
        &mut self,
        name: &str,
        handler: impl Fn(&str) -> Result<String> + Send + Sync + 'static,
    ) {
        self.handlers.insert(name.into(), Box::new(handler));
    }

    pub fn execute(&self, command: &str) -> Result<String> {
        let parts: Vec<&str> = command.trim_start_matches('/').splitn(2, ' ').collect();
        let cmd = parts[0];
        let args = parts.get(1).unwrap_or(&"");

        self.handlers
            .get(cmd)
            .map(|h| h(args))
            .unwrap_or_else(|| Ok(format!("Unknown command: /{}", cmd)))
    }

    pub fn is_command(&self, input: &str) -> bool {
        input.starts_with('/')
    }

    pub fn commands(&self) -> Vec<&str> {
        let mut names: Vec<&str> = self.handlers.keys().map(|s| s.as_str()).collect();
        names.sort();
        names
    }

    pub fn suggestions(&self, prefix: &str) -> Vec<&str> {
        let trimmed = prefix.trim_start_matches('/');
        if trimmed.is_empty() {
            return self.commands();
        }
        self.commands()
            .into_iter()
            .filter(|c| c.starts_with(trimmed))
            .collect()
    }

    pub fn complete(&self, prefix: &str) -> Option<String> {
        let sugs = self.suggestions(prefix);
        if sugs.len() == 1 {
            Some(format!("/{} ", sugs[0]))
        } else {
            None
        }
    }
}
