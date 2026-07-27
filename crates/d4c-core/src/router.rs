use crate::provider::EffortLevel;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutingDecision {
    pub task_summary: String,
    pub selected_model: String,
    pub selected_provider: String,
    pub tier: TaskTier,
    pub effort: EffortLevel,
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
    pub name: String,
    pub provider: String,
    pub tier: TaskTier,
    pub effort: EffortLevel,
    pub supports_tools: bool,
}

pub struct ModelRouter {
    catalog: Vec<CatalogModel>,
    preferred_effort: Option<EffortLevel>,
}

impl ModelRouter {
    pub fn new() -> Self {
        Self {
            catalog: Vec::new(),
            preferred_effort: None,
        }
    }

    pub fn load_default_catalog(&mut self) {
        self.catalog = vec![
            CatalogModel {
                id: "big-pickle".into(),
                name: "Big Pickle".into(),
                provider: "opencode".into(),
                tier: TaskTier::Complex,
                effort: EffortLevel::High,
                supports_tools: true,
            },
            CatalogModel {
                id: "gpt-4o-mini".into(),
                name: "GPT-4o Mini".into(),
                provider: "opencode".into(),
                tier: TaskTier::Simple,
                effort: EffortLevel::Low,
                supports_tools: true,
            },
            CatalogModel {
                id: "gpt-4o".into(),
                name: "GPT-4o".into(),
                provider: "opencode".into(),
                tier: TaskTier::Complex,
                effort: EffortLevel::Medium,
                supports_tools: true,
            },
        ];
    }

    pub fn load_from_models(&mut self, models: &[crate::provider::ModelInfo]) {
        self.catalog = models
            .iter()
            .map(|m| CatalogModel {
                id: m.id.clone(),
                name: m.name.clone(),
                provider: m.provider_id.clone(),
                tier: if m.supports_tools {
                    TaskTier::Complex
                } else {
                    TaskTier::Simple
                },
                effort: EffortLevel::from_model_name(&m.name),
                supports_tools: m.supports_tools,
            })
            .collect();
        if self.catalog.is_empty() {
            self.load_default_catalog();
        }
    }

    pub fn set_preferred_effort(&mut self, effort: Option<EffortLevel>) {
        self.preferred_effort = effort;
    }

    pub fn preferred_effort(&self) -> Option<EffortLevel> {
        self.preferred_effort
    }

    pub fn route(&self, task: &str) -> RoutingDecision {
        let tier = classify_task(task);
        let effort = self.preferred_effort.unwrap_or(match tier {
            TaskTier::Simple => EffortLevel::Low,
            TaskTier::Complex => EffortLevel::High,
        });

        let model = self
            .catalog
            .iter()
            .find(|m| m.tier == tier && m.effort == effort && m.supports_tools)
            .or_else(|| {
                self.catalog
                    .iter()
                    .find(|m| m.tier == tier && m.supports_tools)
            })
            .or_else(|| self.catalog.first());

        let selected_model = model.map(|m| m.id.clone()).unwrap_or_else(|| "default".into());
        let selected_provider = model.map(|m| m.provider.clone()).unwrap_or_else(|| "unknown".into());

        RoutingDecision {
            task_summary: task.chars().take(100).collect(),
            selected_model,
            selected_provider,
            tier,
            effort,
            reason: format!(
                "Classified as {} task, effort {}",
                tier,
                if self.preferred_effort.is_some() {
                    "override"
                } else {
                    "auto"
                }
            ),
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
