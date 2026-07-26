use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutingDecision {
    pub task_summary: String,
    pub selected_model: String,
    pub tier: TaskTier,
    pub reason: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum TaskTier {
    Simple,
    Complex,
}

impl std::fmt::Display for TaskTier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TaskTier::Simple => write!(f, "Simple"),
            TaskTier::Complex => write!(f, "Complex"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CatalogModel {
    pub id: String,
    pub tier: TaskTier,
    pub supports_tools: bool,
}

pub struct ModelRouter {
    catalog: Vec<CatalogModel>,
}

impl ModelRouter {
    pub fn new() -> Self {
        Self {
            catalog: Vec::new(),
        }
    }

    pub fn load_default_catalog(&mut self) {
        self.catalog = vec![
            CatalogModel {
                id: "big-pickle".into(),
                tier: TaskTier::Complex,
                supports_tools: true,
            },
            CatalogModel {
                id: "gpt-4o-mini".into(),
                tier: TaskTier::Simple,
                supports_tools: true,
            },
            CatalogModel {
                id: "gpt-4o".into(),
                tier: TaskTier::Complex,
                supports_tools: true,
            },
        ];
    }

    pub fn route(&self, task: &str) -> RoutingDecision {
        let tier = classify_task(task);
        let model = self
            .catalog
            .iter()
            .find(|m| m.tier == tier && m.supports_tools)
            .or_else(|| self.catalog.first())
            .map(|m| m.id.clone())
            .unwrap_or_else(|| "default".into());

        RoutingDecision {
            task_summary: task.chars().take(100).collect(),
            selected_model: model.clone(),
            tier,
            reason: format!("Classified as {} task", tier),
        }
    }

    pub fn catalog(&self) -> &[CatalogModel] {
        &self.catalog
    }
}

fn classify_task(task: &str) -> TaskTier {
    let task_lower = task.to_lowercase();
    let complex_signals = [
        "refactor",
        "migrate",
        "architecture",
        "redesign",
        "multi-file",
        "across",
        "multiple",
        "complex",
        "breaking change",
        "schema",
        "database",
        "api redesign",
        "system",
        "implement",
        "create",
        "add feature",
        "build",
    ];
    let simple_signals = [
        "read", "show", "list", "find", "what is", "rename", "fix typo", "comment", "format",
        "grep", "search", "explain",
    ];

    let complex_score = complex_signals
        .iter()
        .filter(|s| task_lower.contains(*s))
        .count();
    let simple_score = simple_signals
        .iter()
        .filter(|s| task_lower.contains(*s))
        .count();

    if complex_score > simple_score {
        TaskTier::Complex
    } else {
        TaskTier::Simple
    }
}
