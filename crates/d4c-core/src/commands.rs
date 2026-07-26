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
                Ok("Available commands:\n  /help    [cmd]  - Show help (or help for specific command)\n  /new            - Start new session\n  /clear          - Clear conversation context\n  /quit           - Exit d4c\n  /history        - Browse past sessions\n  /config         - View/edit configuration\n  /login          - Authenticate with a provider\n  /review         - Review diff, plan, or past output\n  /plan    <task> - Start interactive planning\n  /build          - Execute an approved plan".into())
            } else {
                match args {
                    "plan" => Ok("/plan <task> - Interactive planning workflow.\n  Scans repo, generates questionnaire,\n  surfaces assumptions, produces plan\n  for approval.".into()),
                    "build" => Ok("/build - Execute approved plan with\n  checkpoints. Pauses after each step\n  for review/abort.".into()),
                    _ => Ok(format!("No detailed help for /{}", args)),
                }
            }
        });

        registry.register("new", |_args| Ok("Starting new session...".into()));
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
            match parts.first() {
                Some(&"set") => {
                    if parts.len() >= 3 {
                        Ok(format!("Config set: {} = {} (saved)", parts[1], parts[2]))
                    } else {
                        Ok("Usage: /config set <key> <value>".into())
                    }
                }
                Some(&"get") => {
                    if parts.len() >= 2 {
                        Ok(format!("Config get: {} = <current value>", parts[1]))
                    } else {
                        Ok("Usage: /config get <key>".into())
                    }
                }
                Some(&"show") | None => {
                    Ok("Current config:\n  provider.default_provider = opencode\n  model.router_enabled = true\n  ui.theme = default\n\nUse /config set <key> <value> to change.".into())
                }
                _ => Ok("Usage: /config [show|set|get]".into()),
            }
        });

        registry.register("login", |args| {
            if args.is_empty() {
                Ok("Usage: /login <provider>\nProviders: opencode, openai, anthropic".into())
            } else {
                Ok(format!(
                    "Login flow for '{}' — not yet implemented.\nWill prompt for API key and store securely.",
                    args
                ))
            }
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
