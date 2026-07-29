use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

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
    let start = content
        .find('{')
        .context("no JSON object found in response")?;
    let end = content
        .rfind('}')
        .context("no closing '}' found in response")?;
    if end < start {
        anyhow::bail!("malformed JSON: '}}' before '{{'");
    }
    Ok(content[start..=end].to_string())
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