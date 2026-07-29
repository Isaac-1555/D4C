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
    pub questions: Vec<Question>,
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
    Questionnaire,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Question {
    pub id: usize,
    pub text: String,
    pub kind: QuestionKind,
    pub options: Vec<String>,
    #[serde(default)]
    pub option_descriptions: Vec<String>,
    #[serde(default)]
    pub header: String,
    #[serde(default)]
    pub multiple: bool,
    #[serde(default = "default_true")]
    pub custom: bool,
    pub answer: Option<String>,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum QuestionKind {
    FreeText,
    SingleSelect,
    MultiSelect,
    YesNo,
}

impl Plan {
    /// Build a plan in the Questionnaire phase from a model-returned question list.
    pub fn new_questionnaire(task: &str, questions: Vec<Question>) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            task: task.into(),
            steps: Vec::new(),
            status: PlanStatus::Questionnaire,
            assumptions: Vec::new(),
            questions,
        }
    }
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
            questions: Vec::new(),
        }
    }

    pub fn generate_mock_plan(&self, task: &str, index: &RepoIndex) -> Plan {
        let stats = index.stats();
        let file_count = stats.total_files;

        let questions = vec![
            Question {
                id: 1,
                text: "What is the primary goal of this change?".into(),
                kind: QuestionKind::FreeText,
                options: Vec::new(),
                option_descriptions: Vec::new(),
                header: "Goal".into(),
                multiple: false,
                custom: true,
                answer: None,
            },
            Question {
                id: 2,
                text: "Which area of the codebase does this affect?".into(),
                kind: QuestionKind::SingleSelect,
                options: vec![
                    "Core logic".into(),
                    "TUI / UI layer".into(),
                    "Configuration".into(),
                    "Provider integration".into(),
                    "Multiple areas".into(),
                ],
                option_descriptions: Vec::new(),
                header: "Scope".into(),
                multiple: false,
                custom: true,
                answer: None,
            },
            Question {
                id: 3,
                text: "Should this change maintain backward compatibility?".into(),
                kind: QuestionKind::YesNo,
                options: Vec::new(),
                option_descriptions: Vec::new(),
                header: "Compat".into(),
                multiple: false,
                custom: false,
                answer: None,
            },
        ];

        let assumptions = vec![
            Assumption {
                id: 1,
                statement: format!(
                    "The codebase contains {} files to potentially modify",
                    file_count
                ),
                accepted: false,
                editable: true,
            },
            Assumption {
                id: 2,
                statement: "No breaking changes to public API".into(),
                accepted: false,
                editable: true,
            },
            Assumption {
                id: 3,
                statement: "Tests should be updated alongside implementation".into(),
                accepted: false,
                editable: true,
            },
        ];

        let steps = vec![
            PlanStep {
                id: 1,
                description: "Analyze existing code structure and identify affected files".into(),
                files: Vec::new(),
                completed: false,
            },
            PlanStep {
                id: 2,
                description: "Implement core changes".into(),
                files: Vec::new(),
                completed: false,
            },
            PlanStep {
                id: 3,
                description: "Update tests and documentation".into(),
                files: Vec::new(),
                completed: false,
            },
            PlanStep {
                id: 4,
                description: "Verify build passes and run tests".into(),
                files: Vec::new(),
                completed: false,
            },
        ];

        Plan {
            id: uuid::Uuid::new_v4().to_string(),
            task: task.into(),
            steps,
            status: PlanStatus::Questionnaire,
            assumptions,
            questions,
        }
    }
}

// ----- Prompt builders (model-driven planning) -----

pub fn build_question_prompt(task: &str, index: &RepoIndex) -> String {
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

    format!(
r#"You are the planning phase of a terminal coding agent. Given the user's task and a summary of the repository, decide whether to ask clarifying questions.

Rules:
1. If the task is a greeting, menial, single-word, or trivial (no real work to plan), return {{ "questions": [] }}. The UI will then route the user to a normal chat instead of planning.
2. If the task warrants planning, return 1-5 short, task-relevant questions that resolve real ambiguities about THIS task. Do NOT ask generic questions like "What is the primary goal?" or "Which area of the codebase does this affect?" unless the task text is genuinely ambiguous about them.
3. Each question has a "kind": "free_text", "single_select", "multi_select", or "yes_no".
4. For "single_select" and "multi_select", provide 2-6 concrete options tied to the task. Avoid "Multiple areas" filler.
5. Provide a short "header" (max 30 chars) summarizing the question for the UI.
6. Set "custom": true to allow typing a custom answer (default), or "custom": false to force a pick from options.

Return ONLY a JSON object matching this schema (no prose, no markdown fences):
{{
  "questions": [
    {{
      "text": "<full question>",
      "header": "<short label, max 30 chars>",
      "kind": "free_text" | "single_select" | "multi_select" | "yes_no",
      "options": ["...", "..."],
      "option_descriptions": ["...", "..."],
      "multiple": false,
      "custom": true
    }}
  ]
}}

Repository summary:
- Root: {}
- Total files: {}
- Languages: {}
- Sample paths: {}

User task:
{}

JSON:"#,
        index.root.display(),
        stats.total_files,
        langs.join(", "),
        top_files.join(", "),
        task,
    )
}

pub fn build_synthesis_prompt(
    task: &str,
    index: &RepoIndex,
    answers: &[(Question, String)],
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

    let mut qa_block = String::new();
    for (q, a) in answers {
        qa_block.push_str(&format!("- Q: {}\n  A: {}\n", q.text, a));
    }
    if qa_block.is_empty() {
        qa_block.push_str("(no questions were asked - task was unambiguous)\n");
    }

    let reject_block = match reject_reason {
        Some(r) if !r.trim().is_empty() => format!(
            "The user previously rejected a plan with this reason: {}\n\
             Incorporate this feedback into the new assumptions and steps.\n\n",
            r
        ),
        _ => String::new(),
    };

    format!(
r#"You are the synthesis phase of a terminal coding agent. Given the user's task, repository summary, and answers to clarifying questions, produce (a) a list of assumptions the model is making and (b) a step-by-step implementation plan.

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

Answers:
{}
{}JSON:"#,
        index.root.display(),
        stats.total_files,
        langs.join(", "),
        top_files.join(", "),
        task,
        qa_block,
        reject_block,
    )
}

// ----- Parsers -----

pub fn parse_questions(content: &str) -> Result<Vec<Question>> {
    let json = extract_json_object(content)?;
    #[derive(Deserialize)]
    struct QInfo {
        text: String,
        #[serde(default)]
        header: String,
        #[serde(default)]
        kind: String,
        #[serde(default)]
        options: Vec<String>,
        #[serde(default)]
        option_descriptions: Vec<String>,
        #[serde(default)]
        multiple: bool,
        #[serde(default = "default_true")]
        custom: bool,
    }
    #[derive(Deserialize)]
    struct Outer {
        #[serde(default)]
        questions: Vec<QInfo>,
    }
    let parsed: Outer = serde_json::from_str(&json)
        .with_context(|| format!("failed to parse questions JSON: {}", json))?;
    let questions = parsed
        .questions
        .into_iter()
        .enumerate()
        .map(|(i, q)| Question {
            id: i + 1,
            text: q.text,
            kind: match q.kind.to_lowercase().as_str() {
                "single_select" | "single" | "select" => QuestionKind::SingleSelect,
                "multi_select" | "multi" => QuestionKind::MultiSelect,
                "yes_no" | "yesno" | "yn" => QuestionKind::YesNo,
                _ => QuestionKind::FreeText,
            },
            options: q.options,
            option_descriptions: q.option_descriptions,
            header: q.header,
            multiple: q.multiple,
            custom: q.custom,
            answer: None,
        })
        .collect();
    Ok(questions)
}

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
    fn parse_questions_basic() {
        let resp = r#"{"questions":[{"text":"Which DB?","header":"db","kind":"single_select","options":["postgres","sqlite"],"option_descriptions":["p","s"],"multiple":false,"custom":true}]}"#;
        let qs = parse_questions(resp).unwrap();
        assert_eq!(qs.len(), 1);
        assert_eq!(qs[0].text, "Which DB?");
        assert_eq!(qs[0].kind, QuestionKind::SingleSelect);
        assert_eq!(qs[0].options.len(), 2);
        assert_eq!(qs[0].header, "db");
        assert!(qs[0].custom);
    }

    #[test]
    fn parse_questions_empty_menial() {
        let resp = r#"{"questions":[]}"#;
        let qs = parse_questions(resp).unwrap();
        assert!(qs.is_empty());
    }

    #[test]
    fn parse_questions_with_prose_and_fences() {
        let resp = "Here are my questions:\n```json\n{\"questions\":[]}\n```\n";
        let qs = parse_questions(resp).unwrap();
        assert!(qs.is_empty());
    }

    #[test]
    fn parse_questions_kind_aliases() {
        let resp = r#"{"questions":[
            {"text":"a","kind":"yesno"},
            {"text":"b","kind":"multi"},
            {"text":"c","kind":"select"},
            {"text":"d","kind":"free_text"}
        ]}"#;
        let qs = parse_questions(resp).unwrap();
        assert_eq!(qs.len(), 4);
        assert_eq!(qs[0].kind, QuestionKind::YesNo);
        assert_eq!(qs[1].kind, QuestionKind::MultiSelect);
        assert_eq!(qs[2].kind, QuestionKind::SingleSelect);
        assert_eq!(qs[3].kind, QuestionKind::FreeText);
    }

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

    #[test]
    fn question_serde_roundtrip_with_defaults() {
        let q = Question {
            id: 1,
            text: "x".into(),
            kind: QuestionKind::FreeText,
            options: Vec::new(),
            option_descriptions: Vec::new(),
            header: String::new(),
            multiple: false,
            custom: true,
            answer: None,
        };
        let s = serde_json::to_string(&q).unwrap();
        // Missing defaults should still deserialize
        let stripped = s.replace("\"custom\":true,", "");
        let back: Question = serde_json::from_str(&stripped).unwrap();
        assert!(back.custom); // default_true
        assert!(back.option_descriptions.is_empty());
        assert!(back.header.is_empty());
        assert!(!back.multiple);
    }
}