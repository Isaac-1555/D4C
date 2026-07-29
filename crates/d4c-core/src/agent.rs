use anyhow::{Context, Result};
use std::path::PathBuf;

use crate::indexer::RepoIndex;
use crate::plan::PlanStep;
use crate::provider::Message;

#[derive(Debug, Clone)]
pub struct AgentConfig {
    pub plan_prompt: String,
    pub build_prompt: String,
    pub agent_dir: PathBuf,
}

impl AgentConfig {
    pub fn load() -> Result<Self> {
        let root = std::env::current_dir().context("failed to get cwd")?;
        let plan_path = root.join("plan-command.md");
        let build_path = root.join("build-command.md");
        let agent_dir = root.join(".agent");

        let plan_prompt = if plan_path.exists() {
            std::fs::read_to_string(&plan_path)
                .context("failed to read plan-command.md")?
        } else {
            String::new()
        };

        let build_prompt = if build_path.exists() {
            std::fs::read_to_string(&build_path)
                .context("failed to read build-command.md")?
        } else {
            String::new()
        };

        std::fs::create_dir_all(&agent_dir)
            .context("failed to create .agent/ directory")?;

        Ok(Self {
            plan_prompt,
            build_prompt,
            agent_dir,
        })
    }

    pub fn plan_file_path(&self) -> PathBuf {
        self.agent_dir.join("plan.md")
    }

    pub fn todo_file_path(&self) -> PathBuf {
        self.agent_dir.join("todo.md")
    }

    pub fn build_plan_system_messages(&self, task: &str, index: &RepoIndex) -> Vec<Message> {
        let mut messages = Vec::new();
        if !self.plan_prompt.is_empty() {
            messages.push(Message {
                role: "user".into(),
                content: format!(
                    "[SYSTEM INSTRUCTIONS]\n{}\n\nUse these instructions to generate a plan for the following task.\n",
                    self.plan_prompt
                ),
            });
        }

        let stats = index.stats();
        let mut langs: Vec<String> = stats
            .languages
            .iter()
            .map(|(k, v)| format!("{} ({} files)", k, v))
            .collect();
        langs.sort();
        let top_files: Vec<String> = index
            .files
            .keys()
            .take(50)
            .map(|p| p.to_string_lossy().to_string())
            .collect();

        messages.push(Message {
            role: "user".into(),
            content: format!(
                "Task: {}\n\nRepository:\n- Root: {}\n- Files: {}\n- Languages: {}\n- Sample paths: {}\n\nGenerate a plan with assumptions and implementation steps.",
                task,
                index.root.display(),
                stats.total_files,
                langs.join(", "),
                top_files.join(", "),
            ),
        });
        messages
    }

    pub fn build_build_system_messages(&self, step: &PlanStep, plan_summary: &str) -> Vec<Message> {
        let mut messages = Vec::new();
        if !self.build_prompt.is_empty() {
            messages.push(Message {
                role: "user".into(),
                content: format!(
                    "[SYSTEM INSTRUCTIONS]\n{}\n\nExecute this build step:\n",
                    self.build_prompt
                ),
            });
        }

        messages.push(Message {
            role: "user".into(),
            content: format!(
                "Plan: {}\n\nCurrent step ({}/?): {}\n\nImplement this step. Return shell commands and file edits needed.",
                plan_summary,
                step.id,
                step.description,
            ),
        });
        messages
    }
}
