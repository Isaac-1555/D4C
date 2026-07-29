use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use crate::indexer::RepoIndex;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Plan {
    pub id: String,
    pub task: String,
    pub steps: Vec<PlanStep>,
    pub status: PlanStatus,
    pub assumptions: Vec<Assumption>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanStep {
    pub id: usize,
    pub description: String,
    pub files: Vec<PathBuf>,
    pub completed: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum PlanStatus {
    Scanning,
    Assumptions,
    Draft,
    Approved,
    InProgress,
    Completed,
    Rejected,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Assumption {
    pub id: usize,
    pub statement: String,
    pub accepted: bool,
    pub editable: bool,
}

pub struct PlanManager;

impl PlanManager {
    pub fn new() -> Self {
        Self
    }

    pub fn create_plan(&self, task: &str) -> Plan {
        Plan {
            id: uuid::Uuid::new_v4().to_string(),
            task: task.into(),
            steps: Vec::new(),
            status: PlanStatus::Scanning,
            assumptions: Vec::new(),
        }
    }

    pub fn save_to_disk(&self, plan: &Plan, plan_path: &Path, todo_path: &Path) -> Result<()> {
        if let Some(parent) = plan_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        if let Some(parent) = todo_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let plan_json = serde_json::to_string_pretty(plan)?;
        let plan_md = self.plan_to_markdown(plan);
        std::fs::write(plan_path, plan_md)?;
        std::fs::write(
            plan_path.with_extension("json"),
            plan_json,
        )?;
        self.write_todo(plan, todo_path)?;
        Ok(())
    }

    pub fn load_from_disk(plan_path: &Path) -> Result<Plan> {
        let json_path = plan_path.with_extension("json");
        if json_path.exists() {
            let content = std::fs::read_to_string(&json_path)?;
            let plan: Plan = serde_json::from_str(&content)?;
            return Ok(plan);
        }
        anyhow::bail!("no saved plan found at {}", plan_path.display());
    }

    pub fn plan_to_markdown(&self, plan: &Plan) -> String {
        let status_line = match plan.status {
            PlanStatus::Scanning => "**Status:** Scanning\n",
            PlanStatus::Assumptions => "**Status:** Assumptions\n",
            PlanStatus::Draft => "**Status:** Draft\n",
            PlanStatus::Approved => "**Status:** Approved\n",
            PlanStatus::InProgress => "**Status:** In Progress\n",
            PlanStatus::Completed => "**Status:** Completed\n",
            PlanStatus::Rejected => "**Status:** Rejected\n",
        };

        let mut md = format!(
            "# Plan: {}\n\n**ID:** `{}`\n{}\n\n## Task\n\n{}\n\n",
            plan.task, plan.id, status_line, plan.task,
        );

        if !plan.assumptions.is_empty() {
            md.push_str("## Assumptions\n\n");
            for a in &plan.assumptions {
                let status = if a.accepted { "✅" } else { "❌" };
                md.push_str(&format!("- {} {}: {}\n", status, a.id, a.statement));
            }
            md.push('\n');
        }

        md.push_str("## Steps\n\n");
        for step in &plan.steps {
            let done = if step.completed { "[x]" } else { "[ ]" };
            md.push_str(&format!(
                "{} **Step {}:** {}\n",
                done, step.id, step.description,
            ));
            if !step.files.is_empty() {
                let files: Vec<String> = step
                    .files
                    .iter()
                    .map(|f| f.to_string_lossy().to_string())
                    .collect();
                md.push_str(&format!("   Files: {}\n", files.join(", ")));
            }
        }

        md
    }

    pub fn write_todo(&self, plan: &Plan, todo_path: &Path) -> Result<()> {
        if let Some(parent) = todo_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let mut content = format!("# TODO: {}\n\n", plan.task);
        for step in &plan.steps {
            let marker = if step.completed { "x" } else { " " };
            content.push_str(&format!("- [{}] Step {} — {}\n", marker, step.id, step.description));
        }

        std::fs::write(todo_path, content)?;
        Ok(())
    }

    pub fn update_todo_status(todo_path: &Path, step_id: usize, new_marker: &str) -> Result<()> {
        if !todo_path.exists() {
            anyhow::bail!("todo file not found at {}", todo_path.display());
        }
        let content = std::fs::read_to_string(todo_path)?;
        let marker_pattern = format!("Step {}", step_id);
        let mut updated = String::new();
        for line in content.lines() {
            if line.contains(&marker_pattern) && line.trim_start().starts_with("- [") {
                let new_line = if line.contains("- [~]") || line.contains("- [ ]") || line.contains("- [x]") {
                    let before = &line[..line.find("- [").unwrap() + 3];
                    let after = &line[line.find(']').unwrap() + 1..];
                    format!("{}{}{}", before, new_marker, after)
                } else {
                    line.to_string()
                };
                updated.push_str(&new_line);
                updated.push('\n');
            } else {
                updated.push_str(line);
                updated.push('\n');
            }
        }
        std::fs::write(todo_path, updated)?;
        Ok(())
    }

    pub fn mark_todo_in_progress(todo_path: &Path, step_id: usize) -> Result<()> {
        Self::update_todo_status(todo_path, step_id, "~")
    }

    pub fn mark_todo_done(todo_path: &Path, step_id: usize) -> Result<()> {
        Self::update_todo_status(todo_path, step_id, "x")
    }

    pub fn read_todo_plan_summary(todo_path: &Path) -> Result<String> {
        if !todo_path.exists() {
            return Ok(String::new());
        }
        let content = std::fs::read_to_string(todo_path)?;
        let mut summary = String::new();
        let mut total = 0;
        let mut done = 0;
        for line in content.lines() {
            if line.contains("- [") && line.contains("—") {
                total += 1;
                if line.contains("- [x]") {
                    done += 1;
                } else if line.contains("- [~]") {
                    summary.push_str(&format!("  IN PROGRESS: {}\n", line.trim()));
                }
            }
        }
        summary = format!("{}/{} steps completed\n{}", done, total, summary);
        Ok(summary)
    }
}

// ----- Prompt builders (model-driven planning) -----

pub fn build_synthesis_prompt(
    task: &str,
    index: &RepoIndex,
    reject_reason: Option<&str>,
) -> String {
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

    let reject_block = match reject_reason {
        Some(r) if !r.trim().is_empty() => format!(
            "The user previously rejected a plan with this reason: {}\n\
             Incorporate this feedback into the new assumptions and steps.\n\n",
            r
        ),
        _ => String::new(),
    };

    format!(
r#"You are the synthesis phase of a terminal coding agent. Given the user's task and repository summary, produce (a) a list of assumptions the model is making and (b) a step-by-step implementation plan.

Rules:
1. If the task is menial or trivial (no real work), return {{ "assumptions": [], "steps": [] }}.
2. Assumptions: 2-5 short statements the model will hold true unless the user rejects them.
3. Steps: 2-6 concrete, ordered implementation steps. Each step should be implementable in one focused pass.
4. Return ONLY a JSON object (no prose, no markdown fences):
{{
  "assumptions": [
    {{ "statement": "..." }}
  ],
  "steps": [
    {{ "description": "..." }}
  ]
}}

Repository summary:
- Root: {}
- Total files: {}
- Languages: {}
- Sample paths: {}

User task:
{}
{}JSON:"#,
        index.root.display(),
        stats.total_files,
        langs.join(", "),
        top_files.join(", "),
        task,
        reject_block,
    )
}

// ----- Parsers -----

pub fn parse_synthesis(content: &str) -> Result<(Vec<Assumption>, Vec<PlanStep>)> {
    let json = extract_json_object(content)?;
    #[derive(Deserialize)]
    struct AInfo {
        statement: String,
    }
    #[derive(Deserialize)]
    struct SInfo {
        description: String,
    }
    #[derive(Deserialize)]
    struct Outer {
        #[serde(default)]
        assumptions: Vec<AInfo>,
        #[serde(default)]
        steps: Vec<SInfo>,
    }
    let parsed: Outer = serde_json::from_str(&json)
        .with_context(|| format!("failed to parse synthesis JSON: {}", json))?;
    let assumptions = parsed
        .assumptions
        .into_iter()
        .enumerate()
        .map(|(i, a)| Assumption {
            id: i + 1,
            statement: a.statement,
            accepted: false,
            editable: true,
        })
        .collect();
    let steps = parsed
        .steps
        .into_iter()
        .enumerate()
        .map(|(i, s)| PlanStep {
            id: i + 1,
            description: s.description,
            files: Vec::new(),
            completed: false,
        })
        .collect();
    Ok((assumptions, steps))
}

fn extract_json_object(content: &str) -> Result<String> {
    // Models often wrap JSON in markdown fences even when told not to.
    // Strip ```json ... ``` (or plain ``` ... ```) blocks first, then
    // fall back to bare { ... } extraction so leading prose is tolerated.
    let trimmed = content.trim();

    // Strip a leading ```json or ``` fence if present.
    let stripped = if trimmed.starts_with("```") {
        let after_open = trimmed
            .strip_prefix("```json")
            .or_else(|| trimmed.strip_prefix("```"))
            .unwrap_or(trimmed);
        // Remove trailing fence if present.
        if let Some(close_idx) = after_open.rfind("```") {
            &after_open[..close_idx]
        } else {
            after_open
        }
    } else {
        trimmed
    }
    .trim();

    let start = stripped.find('{');
    let end = stripped.rfind('}');

    match (start, end) {
        (Some(s), Some(e)) if e >= s => Ok(stripped[s..=e].to_string()),
        _ => anyhow::bail!(
            "no JSON object found in response. \
             The model returned:\n\n{}",
            content
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_synthesis_basic() {
        let resp = r#"{"assumptions":[{"statement":"Use postgres"}],"steps":[{"description":"Add migration"},{"description":"Update tests"}]}"#;
        let (asms, steps) = parse_synthesis(resp).unwrap();
        assert_eq!(asms.len(), 1);
        assert_eq!(asms[0].statement, "Use postgres");
        assert_eq!(asms[0].id, 1);
        assert_eq!(steps.len(), 2);
        assert_eq!(steps[1].description, "Update tests");
        assert_eq!(steps[1].id, 2);
    }

    #[test]
    fn parse_synthesis_empty_menial() {
        let resp = r#"{"assumptions":[],"steps":[]}"#;
        let (asms, steps) = parse_synthesis(resp).unwrap();
        assert!(asms.is_empty());
        assert!(steps.is_empty());
    }

    #[test]
    fn extract_json_handles_fenced() {
        let content = "```json\n{\"a\":1}\n```";
        let j = extract_json_object(content).unwrap();
        assert_eq!(j, "{\"a\":1}");
    }

    #[test]
    fn extract_json_handles_prose() {
        let content = "Sure! Here:\n{\"a\":1}\nLet me know.";
        let j = extract_json_object(content).unwrap();
        assert_eq!(j, "{\"a\":1}");
    }

    #[test]
    fn extract_json_no_object_fails() {
        let content = "no json here";
        assert!(extract_json_object(content).is_err());
    }

}