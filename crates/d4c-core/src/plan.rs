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
    pub answer: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum QuestionKind {
    FreeText,
    SingleSelect,
    MultiSelect,
    YesNo,
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
                answer: None,
            },
            Question {
                id: 3,
                text: "Should this change maintain backward compatibility?".into(),
                kind: QuestionKind::YesNo,
                options: Vec::new(),
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
