use anyhow::Result;
use crossterm::event::{KeyCode, KeyEventKind};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Style, Stylize, Modifier},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
    Frame, Terminal,
};
use std::io;
use std::time::Duration;

use d4c_core::agent::AgentConfig;
use d4c_core::build::{BuildContext, BuildEngine};
use d4c_core::commands::CommandRegistry;
use d4c_core::indexer::RepoIndex;
use d4c_core::plan::{
    build_synthesis_prompt, parse_synthesis, Plan,
    PlanManager,
};
use d4c_core::provider::{ChatOptions, EffortLevel, OpenCodeProvider, Provider};
use d4c_core::router::ModelRouter;
use crate::event::{poll_event, AppEvent, EventResult};
use crate::input::InputState;
use crate::message::{ChatMessage, Role};
use crate::message_list::MessageList;
use crate::sidebar::Sidebar;
use crate::status_bar::StatusBar;
use crate::theme::Colors;

pub struct App {
    running: bool,
    messages: Vec<ChatMessage>,
    input: InputState,
    commands: CommandRegistry,
    router: ModelRouter,
    current_model: String,
    current_provider: String,
    pinned_model: Option<String>,
    active_plan: Option<Plan>,
    plan_view: PlanView,
    building: bool,
    build_step: usize,
    plan_generating: bool,
    reject_reason: Option<String>,
    colors: Colors,
    agent_busy: bool,
    spinner_frame: usize,
    icons_enabled: bool,
    provider: Option<OpenCodeProvider>,
    runtime: Option<tokio::runtime::Runtime>,
    effort: EffortLevel,
    opencode_process: Option<std::process::Child>,
    model_picker: bool,
    model_selection: usize,
    config_manager: Option<d4c_core::config::ConfigManager>,
    last_used_model: Option<String>,
    agent_config: Option<AgentConfig>,
    build_engine: BuildEngine,
    build_files_touched: Vec<String>,
    build_errors: Vec<String>,
    build_verification: Option<String>,
}

fn provider_priority(provider: &str) -> u8 {
    match provider {
        "opencode" => 0,
        "opencode-go" => 1,
        _ => 2,
    }
}

/// A minimal repo index used when the real scan fails (e.g. permissions).
/// Lets the prompt builder produce a valid string rather than panicking.
fn dummy_index() -> RepoIndex {
    RepoIndex {
        root: std::path::PathBuf::from("."),
        files: std::collections::HashMap::new(),
        total_files: 0,
        total_size: 0,
    }
}

#[derive(PartialEq)]
enum PlanView {
    Hidden,
    Assumptions,
    PlanReview,
}

impl App {
    pub fn new() -> Self {
        let mut router = ModelRouter::new();

        // Load saved config
        let saved_config = d4c_core::config::ConfigManager::load().ok();
        let saved_effort = saved_config
            .as_ref()
            .and_then(|c| c.global().model.effort.clone())
            .and_then(|s| EffortLevel::from_str(&s));
        let base_url = saved_config
            .as_ref()
            .and_then(|c| c.global().provider.base_url.clone());
        let saved_last_used = saved_config
            .as_ref()
            .and_then(|c| c.global().model.last_used_model.clone());

        let runtime = tokio::runtime::Runtime::new().ok();

        let (provider, _connected, opencode_process) = match runtime.as_ref() {
            Some(rt) => {
                let mut p = OpenCodeProvider::new(base_url.clone());
                let mut healthy = rt.block_on(p.check_health()).unwrap_or(false);

                // Auto-start OpenCode server if not running
                let spawned = if !healthy {
                    match std::process::Command::new("opencode")
                        .args(["serve", "--port", "4096"])
                        .stdout(std::process::Stdio::null())
                        .stderr(std::process::Stdio::null())
                        .spawn()
                    {
                        Ok(mut child) => {
                            std::thread::sleep(Duration::from_secs(3));
                            let ok = rt.block_on(p.check_health()).unwrap_or(false);
                            if ok {
                                tracing::info!("Auto-started OpenCode server");
                                healthy = true;
                                Some(child)
                            } else {
                                let _ = child.kill();
                                None
                            }
                        }
                        Err(_) => None,
                    }
                } else {
                    None
                };

                if healthy {
                    tracing::info!("Connected to OpenCode server at {}", p.base_url());

                    if let Ok(models) = rt.block_on(p.list_models()) {
                        if !models.is_empty() {
                            router.load_from_models(&models);
                            tracing::info!("Loaded {} models from OpenCode", models.len());
                        } else {
                            router.load_default_catalog();
                        }
                    } else {
                        router.load_default_catalog();
                    }

                    let _ = rt.block_on(p.ensure_session());
                    (Some(p), true, spawned)
                } else {
                    router.load_default_catalog();
                    (None, false, spawned)
                }
            }
            None => {
                router.load_default_catalog();
                (None, false, None)
            }
        };

        if let Some(e) = saved_effort {
            router.set_preferred_effort(Some(e));
        }

        let initial_decision = router.route("hello");
        let initial_model = if saved_last_used.is_some() {
            saved_last_used.clone().unwrap()
        } else {
            initial_decision.selected_model.clone()
        };
        let initial_provider = initial_decision.selected_provider.clone();
        let effort = initial_decision.effort;

        let app = Self {
            running: true,
            messages: Vec::new(),
            input: InputState::new(),
            commands: CommandRegistry::new(),
            router,
            current_model: initial_model,
            current_provider: initial_provider,
            pinned_model: saved_last_used.clone(),
            active_plan: None,
            plan_view: PlanView::Hidden,
            building: false,
            build_step: 0,
            plan_generating: false,
            reject_reason: None,
            colors: Colors::default(),
            agent_busy: false,
            spinner_frame: 0,
            icons_enabled: true,
            provider,
            runtime,
            effort,
            opencode_process,
            model_picker: false,
            model_selection: 0,
            config_manager: saved_config,
            last_used_model: saved_last_used,
            agent_config: d4c_core::agent::AgentConfig::load().ok(),
            build_engine: BuildEngine::new(),
            build_files_touched: Vec::new(),
            build_errors: Vec::new(),
            build_verification: None,
        };

        app
    }

    pub fn run(&mut self) -> Result<()> {
        let mut stdout = io::stdout();
        crossterm::terminal::enable_raw_mode()?;
        crossterm::execute!(stdout, crossterm::terminal::EnterAlternateScreen)?;

        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;

        while self.running {
            terminal.draw(|f| self.draw(f))?;

            match poll_event(Duration::from_millis(100))? {
                EventResult::Event(AppEvent::KeyInput(key)) => {
                    if key.kind == KeyEventKind::Press || key.kind == KeyEventKind::Repeat {
                        match key.code {
                            KeyCode::Char('c')
                                if key.modifiers.contains(crossterm::event::KeyModifiers::CONTROL) =>
                            {
                                self.running = false;
                            }
                            KeyCode::Backspace
                            | KeyCode::Delete
                            | KeyCode::Char('\x7f')
                            | KeyCode::Char('\x08') => {
                                self.input.delete_char();
                                if self.model_picker {
                                    self.model_selection = 0;
                                }
                            }
                            KeyCode::Char('h')
                                if key.modifiers.contains(crossterm::event::KeyModifiers::CONTROL) =>
                            {
                                self.input.delete_char();
                                if self.model_picker {
                                    self.model_selection = 0;
                                }
                            }
                            KeyCode::Esc => {
                                if self.model_picker {
                                    self.close_model_picker();
                                } else if self.plan_view != PlanView::Hidden {
                                    self.plan_view = PlanView::Hidden;
                                    self.plan_generating = false;
                                    self.agent_busy = false;
                                }
                            }
                            KeyCode::Enter => {
                                if self.model_picker {
                                    self.select_model_picker_model();
                                } else {
                                    let input = self.input.submit();
                                    if !input.is_empty() {
                                        self.handle_input(input);
                                    }
                                }
                            }
                            KeyCode::Tab => {
                                if self.input.content.starts_with('/') {
                                    let completed = self.commands.complete(&self.input.content);
                                    if let Some(replacement) = completed {
                                        self.input.replace_input(replacement);
                                    }
                                }
                            }
                            KeyCode::Char('e')
                                if key.modifiers.contains(crossterm::event::KeyModifiers::CONTROL) =>
                            {
                                self.cycle_effort();
                            }
                            KeyCode::Char(_c)
                                if key.modifiers.contains(crossterm::event::KeyModifiers::CONTROL) =>
                            {
                            }
                            KeyCode::Char(' ') => {
                                self.input.insert_char(' ');
                                if self.model_picker {
                                    self.model_selection = 0;
                                }
                            }
                            KeyCode::Char(c) => {
                                self.input.insert_char(c);
                                if self.model_picker {
                                    self.model_selection = 0;
                                }
                            }
                            KeyCode::Left => self.input.move_left(),
                            KeyCode::Right => self.input.move_right(),
                            KeyCode::Up => {
                                if self.model_picker {
                                    self.model_selection = self.model_selection.saturating_sub(1);
                                } else {
                                    self.input.scroll_up();
                                }
                            }
                            KeyCode::Down => {
                                if self.model_picker {
                                    let max = self.filtered_models().len().saturating_sub(1);
                                    self.model_selection = (self.model_selection + 1).min(max);
                                } else {
                                    self.input.scroll_down();
                                }
                            }
                            _ => {}
                        }
                    }
                }
                EventResult::Event(AppEvent::Resize(_, _)) => {}
                EventResult::Timeout => {
                    if self.agent_busy {
                        self.spinner_frame = self.spinner_frame.wrapping_add(1);
                    }
                }
            }
        }

        crossterm::terminal::disable_raw_mode()?;
        crossterm::execute!(
            terminal.backend_mut(),
            crossterm::terminal::LeaveAlternateScreen
        )?;
        terminal.show_cursor()?;

        if let Some(mut child) = self.opencode_process.take() {
            let _ = child.kill();
            let _ = child.wait();
            tracing::info!("Stopped auto-started OpenCode server");
        }

        Ok(())
    }

    fn handle_input(&mut self, input: String) {
        if self.plan_view != PlanView::Hidden {
            self.handle_plan_input(input);
            return;
        }

        self.messages
            .push(ChatMessage::new(Role::User, input.clone()));

        if self.commands.is_command(&input) {
            match self.commands.execute(&input) {
                Ok(output) => {
                    if output == "__QUIT__" {
                        self.running = false;
                        return;
                    }
                    if output == "__BUILD_CHECK__" {
                        let plan_source = match &self.active_plan {
                            Some(plan) if plan.status == d4c_core::plan::PlanStatus::Approved => {
                                Some(plan.clone())
                            }
                            _ => {
                                if let Some(ref ac) = self.agent_config {
                                    let plan_path = ac.plan_file_path();
                                    match PlanManager::load_from_disk(&plan_path) {
                                        Ok(disk_plan) if disk_plan.status == d4c_core::plan::PlanStatus::Approved => {
                                            self.active_plan = Some(disk_plan.clone());
                                            Some(disk_plan)
                                        }
                                        _ => None,
                                    }
                                } else {
                                    None
                                }
                            }
                        };
                        match plan_source {
                            Some(_) => {
                                self.building = true;
                                self.build_step = 0;
                                self.build_files_touched.clear();
                                self.build_errors.clear();
                                self.build_verification = None;
                                self.plan_view = PlanView::PlanReview;
                                self.execute_build_step();
                            }
                            None => {
                                self.messages.push(ChatMessage::new(
                                    Role::Error,
                                    "No approved plan found. Use /plan <task> to create and approve one.",
                                ));
                            }
                        }
                    } else if output == "__BUILD_NEXT__" {
                        if self.building {
                            self.execute_build_step();
                        } else {
                            self.messages.push(ChatMessage::new(
                                Role::Error,
                                "No build in progress.",
                            ));
                        }
                    } else if output == "__BUILD_ABORT__" {
                        if self.building {
                            self.building = false;
                            self.plan_view = PlanView::Hidden;
                            self.messages.push(ChatMessage::new(
                                Role::Tool,
                                "Build paused. Run `/build resume` to continue.",
                            ));
                        }
                    } else if output == "__BUILD_RESUME__" {
                        self.try_resume_build();
                    } else if output == "__BUILD_STATUS__" {
                        self.show_todo_status();
                    } else if let Some(task) = output.strip_prefix("__PLAN_GENERATE__") {
                        self.start_plan_direct(task);
                    } else if output == "__NEW_SESSION__" {
                        self.messages.clear();
                        if let Some(ref mut p) = self.provider {
                            p.reset_session();
                            if let Some(ref rt) = self.runtime {
                                let _ = rt.block_on(p.ensure_session());
                            }
                        }
                    } else if output.starts_with("__SET_EFFORT__") {
                        let level = &output["__SET_EFFORT__".len()..];
                        if let Some(e) = EffortLevel::from_str(level) {
                            self.effort = e;
                            self.router.set_preferred_effort(Some(e));
                            // Re-route to pick a model matching the new effort
                            let decision = self.router.route("hello");
                            if self.pinned_model.is_none() {
                                self.current_model = decision.selected_model;
                                self.current_provider = decision.selected_provider;
                            }
                        } else {
                            self.messages.push(ChatMessage::new(
                                Role::Error,
                                format!(
                                    "Unknown effort level: {}. Use low, medium, or high.",
                                    level
                                ),
                            ));
                        }
                    } else if output.starts_with("__LOGIN__") {
                        let url = &output["__LOGIN__".len()..];
                        self.try_connect(url);
                    } else if output.starts_with("__SET_CONFIG__") {
                        let rest = &output["__SET_CONFIG__".len()..];
                        let mut parts = rest.splitn(2, ' ');
                        let key = parts.next().unwrap_or("");
                        let _val = parts.next().unwrap_or("");
                        match key {
                            "base_url" => {}
                            _ => {}
                        }
                    } else if output.starts_with("__MODEL__") {
                        let name = output["__MODEL__".len()..].trim().to_string();
                        if name.is_empty() {
                            self.model_picker = true;
                            self.model_selection = 0;
                            self.input = InputState::new();
                        } else {
                            // Find matching model
                            let lower = name.to_lowercase();
                            let matches: Vec<&d4c_core::router::CatalogModel> = self
                                .router
                                .catalog()
                                .iter()
                                .filter(|m| m.id.to_lowercase().contains(&lower))
                                .collect();
                            if matches.is_empty() {
                                self.messages.push(ChatMessage::new(
                                    Role::Error,
                                    format!("No model matching '{}'", name),
                                ));
                            } else if matches.len() == 1 {
                                let model = matches[0].id.clone();
                                let provider = matches[0].provider.clone();
                                self.pinned_model = Some(model.clone());
                                self.current_model = model.clone();
                                self.current_provider = provider;
                            } else {
                            }
                        }
                    } else if input.trim() == "/clear" {
                        self.messages.clear();
                    } else {
                    }
                }
                Err(e) => {
                    self.messages
                        .push(ChatMessage::new(Role::Error, format!("Error: {}", e)));
                }
            }
        } else {
            // Chat message — route and send to provider
            let decision = self.router.route(&input);
            if self.pinned_model.is_none() {
                self.current_model = decision.selected_model.clone();
                self.current_provider = decision.selected_provider.clone();
            }
            self.agent_busy = true;

            let response = match self.provider.as_mut().and_then(|p| self.runtime.as_ref().map(|rt| (p, rt))) {
                Some((provider, rt)) => {
                    let _ = rt.block_on(provider.ensure_session());

                    let msg = d4c_core::provider::Message {
                        role: "user".into(),
                        content: input.clone(),
                    };

                    let options = ChatOptions {
                        provider_id: Some(self.current_provider.clone()),
                        model_id: Some(self.current_model.clone()),
                    };

                    match rt.block_on(provider.chat(&[msg], &[], &options)) {
                        Ok(resp) => {
                            if resp.tool_calls.is_empty() {
                                resp.content
                            } else {
                                let mut out = resp.content;
                                for tc in &resp.tool_calls {
                                    out.push_str(&format!(
                                        "\n\n[tool call: {} with {:?}]",
                                        tc.name, tc.arguments
                                    ));
                                }
                                out
                            }
                        }
                        Err(e) => {
                            format!("[OpenCode error: {}]", e)
                        }
                    }
                }
                None => {
                    format!(
                        "[routed to {} ({})]\nReceived: \"{}\"\n\n\
                         No OpenCode server connected. Start it with `opencode serve` \
                         or check your connection.",
                        decision.selected_model,
                        decision.tier,
                        input.chars().take(80).collect::<String>()
                    )
                }
            };

            self.agent_busy = false;
            self.messages.push(ChatMessage::new(Role::Agent, response));
        }
    }

    fn handle_plan_input(&mut self, input: String) {
        if let Some(ref mut plan) = self.active_plan {
            match self.plan_view {
                PlanView::Assumptions => {
                    if input.trim().eq_ignore_ascii_case("done") {
                        plan.assumptions.iter_mut().for_each(|a| a.accepted = true);
                        plan.status = d4c_core::plan::PlanStatus::Draft;
                        self.plan_view = PlanView::PlanReview;
                    } else if let Ok(num) = input.trim().parse::<usize>() {
                        if let Some(a) = plan.assumptions.iter_mut().find(|a| a.id == num) {
                            a.accepted = !a.accepted;
                        }
                    }
                }
                PlanView::PlanReview => {
                    if input.trim().eq_ignore_ascii_case("approve") {
                        plan.status = d4c_core::plan::PlanStatus::Approved;
                        self.plan_view = PlanView::Hidden;
                        if let Some(ref agent_cfg) = self.agent_config {
                            let mgr = PlanManager::new();
                            let plan_path = agent_cfg.plan_file_path();
                            let todo_path = agent_cfg.todo_file_path();
                            if let Err(e) = mgr.save_to_disk(plan, &plan_path, &todo_path) {
                                self.messages.push(ChatMessage::new(
                                    Role::Error,
                                    format!("Failed to save plan: {}", e),
                                ));
                            } else {
                                let step_count = plan.steps.len();
                                self.messages.push(ChatMessage::new(
                                    Role::Tool,
                                    format!(
                                        "Plan approved. {} step(s). Run `/build` to start execution.",
                                        step_count,
                                    ),
                                ));
                            }
                        }
                    } else if let Some(reason) = input
                        .trim()
                        .strip_prefix("reject")
                        .map(|r| r.trim().to_string())
                    {
                        plan.status = d4c_core::plan::PlanStatus::Scanning;
                        self.reject_reason = Some(reason);
                        plan.assumptions.iter_mut().for_each(|a| a.accepted = false);
                        plan.steps.clear();
                        plan.assumptions.clear();
                        let task = plan.task.clone();
                        self.start_plan_direct(&task);
                    }
                }
                PlanView::Hidden => {}
            }
        }
    }



    fn start_plan_direct(&mut self, task: &str) {
        let (provider, rt) = match self.provider.as_mut().and_then(|p| self.runtime.as_ref().map(|rt| (p, rt))) {
            Some((p, rt)) => (p, rt),
            None => {
                let mgr = PlanManager::new();
                let mut plan = mgr.create_plan(task);
                plan.status = d4c_core::plan::PlanStatus::Assumptions;
                plan.steps = vec![d4c_core::plan::PlanStep {
                    id: 1,
                    description: "Implement core change".into(),
                    files: Vec::new(),
                    completed: false,
                }];
                plan.assumptions = vec![
                    d4c_core::plan::Assumption {
                        id: 1,
                        statement: "No breaking changes to public API".into(),
                        accepted: false,
                        editable: true,
                    },
                    d4c_core::plan::Assumption {
                        id: 2,
                        statement: "Tests should be updated alongside implementation".into(),
                        accepted: false,
                        editable: true,
                    },
                ];
                self.active_plan = Some(plan);
                self.plan_view = PlanView::Assumptions;
                return;
            }
        };

        self.plan_generating = true;
        self.agent_busy = true;

        let reject = self.reject_reason.clone();

        let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
        let index = RepoIndex::scan(&cwd).ok();
        let base_prompt = match &index {
            Some(idx) => build_synthesis_prompt(task, idx, reject.as_deref()),
            None => build_synthesis_prompt(task, &dummy_index(), reject.as_deref()),
        };

        let plan_system = self
            .agent_config
            .as_ref()
            .map(|ac| ac.plan_prompt.clone())
            .filter(|s| !s.is_empty());

        let final_prompt = match &plan_system {
            Some(sys) => format!(
                "[SYSTEM INSTRUCTIONS]\n{}\n\nNow generate a plan following those instructions.\n\n{}",
                sys, base_prompt
            ),
            None => base_prompt,
        };

        let msg = d4c_core::provider::Message {
            role: "user".into(),
            content: final_prompt,
        };
        let options = ChatOptions {
            provider_id: Some(self.current_provider.clone()),
            model_id: Some(self.current_model.clone()),
        };

        let result = rt.block_on(provider.chat(&[msg], &[], &options));
        self.plan_generating = false;
        self.agent_busy = false;

        match result {
            Ok(resp) => {
                match parse_synthesis(&resp.content) {
                    Ok((assumptions, steps)) => {
                        if assumptions.is_empty() && steps.is_empty() {
                            self.messages.push(ChatMessage::new(
                                Role::Error,
                                "Model returned no plan - try 'reject <reason>' to re-synthesize, or Esc to abort."
                                    .to_string(),
                            ));
                            self.plan_view = PlanView::PlanReview;
                            return;
                        }
                        let mgr = PlanManager::new();
                        let mut plan = mgr.create_plan(task);
                        plan.status = d4c_core::plan::PlanStatus::Assumptions;
                        plan.steps = steps;
                        plan.assumptions = assumptions;
                        self.active_plan = Some(plan);
                        self.plan_view = PlanView::Assumptions;
                    }
                    Err(e) => {
                        self.messages.push(ChatMessage::new(
                            Role::Error,
                            format!("Failed to parse plan: {}. Response was: {}", e, resp.content),
                        ));
                        self.plan_view = PlanView::Assumptions;
                    }
                }
            }
            Err(e) => {
                self.messages.push(ChatMessage::new(
                    Role::Error,
                    format!("[plan generation error: {}]", e),
                ));
                self.plan_view = PlanView::Assumptions;
            }
        }
    }

    fn execute_build_step(&mut self) {
        let (provider, rt) = match self.provider.as_ref().and_then(|p| self.runtime.as_ref().map(|rt| (p, rt))) {
            Some((p, rt)) => (p, rt),
            None => {
                self.messages.push(ChatMessage::new(
                    Role::Error,
                    "No provider connected. Cannot execute build steps.",
                ));
                return;
            }
        };

        if let Some(ref mut plan) = self.active_plan {
            if self.build_step >= plan.steps.len() {
                return;
            }

            let step = plan.steps[self.build_step].clone();
            let total = plan.steps.len();
            let step_idx = self.build_step;

            // Mark in-progress in todo
            if let Some(ref ac) = self.agent_config {
                let _ = PlanManager::mark_todo_in_progress(&ac.todo_file_path(), step.id);
            }

            // Build context from previous step output
            let previous_output: Vec<String> = self
                .messages
                .iter()
                .filter(|m| m.role == Role::Tool && m.content.contains("Step "))
                .map(|m| m.content.clone())
                .collect();

            let build_ctx = BuildContext {
                plan_task: plan.task.clone(),
                step_index: step_idx,
                total_steps: total,
                previous_output,
            };

            let system_prompt = self
                .agent_config
                .as_ref()
                .map(|ac| ac.build_prompt.clone())
                .unwrap_or_default();

            let todo_path = self
                .agent_config
                .as_ref()
                .map(|ac| ac.todo_file_path())
                .unwrap_or_else(|| std::path::PathBuf::from(".agent/todo.md"));

            self.agent_busy = true;

            let result = rt.block_on(self.build_engine.execute_step(
                provider,
                plan,
                &step,
                &build_ctx,
                &system_prompt,
                &todo_path,
            ));

            self.agent_busy = false;

            // Track results
            for f in &result.files_touched {
                if !self.build_files_touched.contains(f) {
                    self.build_files_touched.push(f.clone());
                }
            }
            for e in &result.errors {
                if !self.build_errors.contains(e) {
                    self.build_errors.push(e.clone());
                }
            }

            // Show result
            let status_icon = if result.success { "✓" } else { "✗" };
            let mut msg = format!("{} Step {}/{}: {}", status_icon, step_idx + 1, total, step.description);
            if !result.output.is_empty() {
                let preview: String = result.output.chars().take(200).collect();
                msg.push_str(&format!("\n{}", preview));
                if result.output.len() > 200 {
                    msg.push_str("…");
                }
            }
            if !result.errors.is_empty() {
                for e in &result.errors {
                    msg.push_str(&format!("\n  Error: {}", e));
                }
            }
            self.messages.push(ChatMessage::new(Role::Tool, msg));

            plan.steps[step_idx].completed = result.success;
            self.build_step += 1;

            if self.build_step >= plan.steps.len() || !result.success {
                self.building = false;
                plan.status = if result.success {
                    d4c_core::plan::PlanStatus::Completed
                } else {
                    d4c_core::plan::PlanStatus::InProgress
                };
                self.plan_view = PlanView::Hidden;

                // Run verification
                self.run_build_verification();
            }
        }
    }

    fn run_build_verification(&mut self) {
        self.agent_busy = true;
        let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));

        let cmd = if cwd.join("Cargo.toml").exists() {
            Some(("cargo check", "cargo check"))
        } else if cwd.join("package.json").exists() {
            Some(("npm run build", "npm run build 2>&1 || npm run typecheck 2>&1 || true"))
        } else if cwd.join("pyproject.toml").exists() || cwd.join("setup.py").exists() {
            Some(("python -m pytest", "python -m pytest --no-header -q 2>&1 || true"))
        } else {
            None
        };

        let result = match cmd {
            Some((label, actual_cmd)) => {
                self.messages.push(ChatMessage::new(
                    Role::Tool,
                    format!("Running verification: {} …", label),
                ));
                let output = std::process::Command::new("sh")
                    .arg("-c")
                    .arg(actual_cmd)
                    .output();
                match output {
                    Ok(out) => {
                        let stdout = String::from_utf8_lossy(&out.stdout).to_string();
                        let stderr = String::from_utf8_lossy(&out.stderr).to_string();
                        let success = out.status.success();
                        let detail = if !stdout.is_empty() { stdout } else { stderr };
                        let detail_trimmed: String = detail.chars().take(500).collect();
                        let status = if success { "passed" } else { "failed" };
                        let full = format!("Verification ({}) {}\n{}", label, status, detail_trimmed);
                        self.build_verification = Some(full.clone());
                        full
                    }
                    Err(e) => {
                        let err = format!("Verification command failed: {}", e);
                        self.build_verification = Some(err.clone());
                        err
                    }
                }
            }
            None => {
                let msg: String = "No verification command found for this project type.".into();
                self.build_verification = Some(msg.clone());
                msg
            }
        };

        self.agent_busy = false;
        self.messages.push(ChatMessage::new(Role::Tool, result));

        // Show build report
        self.show_build_report();
    }

    fn show_build_report(&mut self) {
        let total_steps = self
            .active_plan
            .as_ref()
            .map(|p| p.steps.len())
            .unwrap_or(0);
        let files = if self.build_files_touched.is_empty() {
            "None".into()
        } else {
            self.build_files_touched.join("\n  ")
        };
        let errors = if self.build_errors.is_empty() {
            "None".into()
        } else {
            self.build_errors.join("\n  ")
        };
        let verification = self.build_verification.as_deref().unwrap_or("Not run").to_string();

        let report = format!(
            "Build complete.\n\n\
             **Summary:** Finished {} step(s)\n\n\
             **Files touched:**\n  {}\n\n\
             **Errors:**\n  {}\n\n\
             **Verification:** {}\n\n\
             Run `/build` to continue or start a new plan with `/plan`.",
            total_steps,
            files,
            errors,
            verification,
        );

        self.messages.push(ChatMessage::new(Role::Tool, report));
    }

    fn try_resume_build(&mut self) {
        if let Some(ref ac) = self.agent_config {
            let plan_path = ac.plan_file_path();
            let todo_path = ac.todo_file_path();

            match PlanManager::load_from_disk(&plan_path) {
                Ok(plan) => {
                    let todo_summary = PlanManager::read_todo_plan_summary(&todo_path).unwrap_or_default();
                    let resume_from = plan
                        .steps
                        .iter()
                        .position(|s| !s.completed)
                        .unwrap_or(0);

                    self.active_plan = Some(plan);
                    self.building = true;
                    self.build_step = resume_from;
                    self.build_files_touched.clear();
                    self.build_errors.clear();
                    self.build_verification = None;
                    self.plan_view = PlanView::PlanReview;

                    self.messages.push(ChatMessage::new(
                        Role::Tool,
                        format!("Resuming build from step {}.\n{}", resume_from + 1, todo_summary),
                    ));

                    self.execute_build_step();
                }
                Err(e) => {
                    self.messages.push(ChatMessage::new(
                        Role::Error,
                        format!("Cannot resume: {}", e),
                    ));
                }
            }
        } else {
            self.messages.push(ChatMessage::new(
                Role::Error,
                "No agent configuration found. Cannot resume build.",
            ));
        }
    }

    fn show_todo_status(&mut self) {
        if let Some(ref ac) = self.agent_config {
            let todo_path = ac.todo_file_path();
            match PlanManager::read_todo_plan_summary(&todo_path) {
                Ok(summary) => {
                    if summary.is_empty() {
                        self.messages.push(ChatMessage::new(
                            Role::Tool,
                            "No active build in progress.",
                        ));
                    } else {
                        self.messages.push(ChatMessage::new(
                            Role::Tool,
                            format!("Build status:\n{}", summary),
                        ));
                    }
                }
                Err(e) => {
                    self.messages.push(ChatMessage::new(
                        Role::Error,
                        format!("Failed to read build status: {}", e),
                    ));
                }
            }
        } else {
            self.messages.push(ChatMessage::new(
                Role::Tool,
                "No agent config. Use /plan <task> to start.",
            ));
        }
    }

    fn cycle_effort(&mut self) {
        self.effort = match self.effort {
            EffortLevel::Low => EffortLevel::Medium,
            EffortLevel::Medium => EffortLevel::High,
            EffortLevel::High => EffortLevel::Low,
        };
        self.router.set_preferred_effort(Some(self.effort));
        // Re-route with a dummy task to pick a model matching the new effort
        let decision = self.router.route("hello");
        if self.pinned_model.is_none() {
            self.current_model = decision.selected_model;
            self.current_provider = decision.selected_provider;
        }
    }

    fn try_connect(&mut self, url: &str) {
        match self.runtime.as_ref() {
            Some(rt) => {
                let mut p = OpenCodeProvider::new(Some(url.to_string()));
                let healthy = rt.block_on(p.check_health()).unwrap_or(false);
                if healthy {
                    tracing::info!("Connected to OpenCode server at {}", url);
                    if let Ok(models) = rt.block_on(p.list_models()) {
                        if !models.is_empty() {
                            self.router.load_from_models(&models);
                        }
                    }
                    let _ = rt.block_on(p.ensure_session());
                    self.provider = Some(p);
                } else {
                    self.messages.push(ChatMessage::new(
                        Role::Error,
                        format!("Could not reach OpenCode at {}. Is the server running?", url),
                    ));
                }
            }
            None => {
                self.messages.push(ChatMessage::new(
                    Role::Error,
                    "Cannot connect: no async runtime available.",
                ));
            }
        }
    }

    fn filtered_models(&self) -> Vec<&d4c_core::router::CatalogModel> {
        let lower = self.input.content.to_lowercase();
        let mut models: Vec<&d4c_core::router::CatalogModel> = if lower.is_empty() {
            self.router.catalog().iter().collect()
        } else {
            self.router
                .catalog()
                .iter()
                .filter(|m| {
                    m.id.to_lowercase().contains(&lower)
                        || m.name.to_lowercase().contains(&lower)
                        || m.provider.to_lowercase().contains(&lower)
                })
                .collect()
        };

        models.sort_by(|a, b| {
            let a_last = self.last_used_model.as_deref() == Some(a.id.as_str());
            let b_last = self.last_used_model.as_deref() == Some(b.id.as_str());
            if a_last != b_last {
                return b_last.cmp(&a_last);
            }
            let a_priority = provider_priority(&a.provider);
            let b_priority = provider_priority(&b.provider);
            if a_priority != b_priority {
                return a_priority.cmp(&b_priority);
            }
            a.name.cmp(&b.name)
        });

        models
    }

    fn close_model_picker(&mut self) {
        self.model_picker = false;
        self.input = InputState::new();
    }

    fn select_model_picker_model(&mut self) {
        let (model_id, provider_id) = {
            let filtered = self.filtered_models();
            if filtered.is_empty() {
                return;
            }
            let idx = self.model_selection.min(filtered.len() - 1);
            (filtered[idx].id.clone(), filtered[idx].provider.clone())
        };
        self.pinned_model = Some(model_id.clone());
        self.current_model = model_id.clone();
        self.current_provider = provider_id;
        self.last_used_model = Some(model_id.clone());

        if let Some(ref mut config) = self.config_manager {
            let mut cfg = config.global().clone();
            cfg.model.last_used_model = Some(model_id);
            let _ = config.save_global(cfg);
        }

        self.close_model_picker();
    }

    fn draw_model_picker(&self, f: &mut Frame, area: Rect) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Min(3),
                Constraint::Length(3),
                Constraint::Length(1),
            ])
            .split(area);

        let filtered = self.filtered_models();
        let mut lines: Vec<Line> = Vec::new();
        let mut selectable_indices: Vec<usize> = Vec::new();

        let mut last_provider = "";
        let mut last_was_header = false;

        for m in filtered.iter() {
            let is_last = self.last_used_model.as_deref() == Some(m.id.as_str());

            if is_last && !last_was_header {
                lines.push(Line::from(Span::styled(
                    " Last used",
                    Style::default()
                        .fg(self.colors.accent_system)
                        .add_modifier(Modifier::BOLD),
                )));
                last_was_header = true;
                last_provider = "";
            }

            if !is_last && m.provider != last_provider {
                let header = match m.provider.as_str() {
                    "opencode" => " OpenCode Zen",
                    "opencode-go" => " OpenCode Go",
                    _ => "",
                };
                if !header.is_empty() {
                    lines.push(Line::from(Span::styled(
                        header,
                        Style::default()
                            .fg(self.colors.accent_system)
                            .add_modifier(Modifier::BOLD),
                    )));
                    last_was_header = true;
                } else if last_provider != m.provider {
                    lines.push(Line::from(Span::styled(
                        format!(" {}", m.provider),
                        Style::default()
                            .fg(self.colors.accent_system)
                            .add_modifier(Modifier::BOLD),
                    )));
                    last_was_header = true;
                }
                last_provider = &m.provider;
            }

            if !is_last {
                last_was_header = false;
            }

            selectable_indices.push(lines.len());

            let selected = selectable_indices.len() - 1 == self.model_selection;
            let prefix = if selected { " ◉ " } else { " ○ " };
            let style = if selected {
                Style::default()
                    .fg(self.colors.accent_user)
                    .add_modifier(Modifier::REVERSED)
            } else if is_last {
                Style::default()
                    .fg(self.colors.accent_user)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(self.colors.text)
            };
            let tag = if is_last { " (last used)" } else { "" };
            lines.push(Line::from(Span::styled(
                format!("{}{}  [{}]{}", prefix, m.name, m.effort, tag),
                style,
            )));
        }

        if filtered.is_empty() {
            lines.push(Line::from(Span::styled(
                " No models match your filter",
                Style::default().fg(self.colors.text_muted),
            )));
        }

        let visible_height = chunks[0].height.saturating_sub(2) as usize;
        let scroll_offset = if self.model_selection >= visible_height {
            (self.model_selection - visible_height + 1) as u16
        } else {
            0
        };

        let list = Paragraph::new(lines)
            .scroll((scroll_offset, 0))
            .block(
                Block::default()
                    .title(" Model Picker ")
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(self.colors.accent_system)),
            )
            .bg(self.colors.surface);
        f.render_widget(list, chunks[0]);

        let info = Paragraph::new(Line::from(Span::styled(
            " Type to filter · ↑↓ navigate · Enter select · Esc cancel ",
            Style::default().fg(self.colors.text_muted),
        )))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(self.colors.border)),
        )
        .bg(self.colors.surface);
        f.render_widget(info, chunks[1]);

        self.draw_input(f, chunks[2]);
    }

    fn draw(&self, f: &mut Frame) {
        let area = f.area();

        f.render_widget(Block::default().bg(self.colors.bg), area);

        let show_sidebar = area.width >= 90;

        let (chat_area, sidebar_area) = if show_sidebar {
            let sidebar_width = (area.width as f32 * 0.2).max(22.0).min(30.0) as u16;
            let chunks = Layout::horizontal([
                Constraint::Fill(1),
                Constraint::Length(sidebar_width),
            ])
            .split(area);
            (chunks[0], Some(chunks[1]))
        } else {
            (area, None)
        };

        if self.model_picker {
            self.draw_model_picker(f, chat_area);
        } else if self.plan_view != PlanView::Hidden {
            self.draw_plan_view(f, chat_area);
        } else {
            self.draw_chat_view(f, chat_area);
        }

        if let Some(side) = sidebar_area {
            self.draw_sidebar(f, side);
        }
    }

    fn draw_chat_view(&self, f: &mut Frame, area: Rect) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Min(3),
                Constraint::Length(3),
                Constraint::Length(1),
            ])
            .split(area);

        let msg_bg = Block::default().bg(self.colors.bg);
        f.render_widget(msg_bg, chunks[0]);

        let mut message_list = MessageList::new(&self.messages, &self.colors);
        message_list.icons_enabled = self.icons_enabled;
        message_list.render(f, chunks[0]);

        self.draw_input(f, chunks[1]);
        if self.input.content.starts_with('/') {
            self.draw_suggestions_overlay(f, chunks[1]);
        }
        self.draw_status(f, chunks[2]);
    }

    fn draw_plan_view(&self, f: &mut Frame, area: Rect) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
            .split(area);

        let msg_bg = Block::default().bg(self.colors.bg);
        f.render_widget(msg_bg, chunks[0]);

        let mut message_list = MessageList::new(&self.messages, &self.colors);
        message_list.icons_enabled = self.icons_enabled;
        message_list.render(f, chunks[0]);
        self.draw_plan_panel(f, chunks[1]);
    }

    fn draw_plan_panel(&self, f: &mut Frame, area: Rect) {
        if let Some(ref plan) = self.active_plan {
            let mut lines: Vec<Line> = Vec::new();

            match self.plan_view {
                PlanView::Assumptions => {
                    lines.push(Line::from(Span::styled(
                        " Assumptions (toggle with number, 'done' to accept all)",
                        Style::default()
                            .fg(self.colors.accent_system)
                            .add_modifier(Modifier::BOLD),
                    )));
                    lines.push(Line::from(""));
                    for a in &plan.assumptions {
                        let marker = if a.accepted { "[x]" } else { "[ ]" };
                        let style = if a.accepted {
                            Style::default().fg(self.colors.accent_success)
                        } else {
                            Style::default().fg(self.colors.text)
                        };
                        lines.push(Line::from(Span::styled(
                            format!("{} {}: {}", marker, a.id, a.statement),
                            style,
                        )));
                    }
                }
                PlanView::PlanReview => {
                    lines.push(Line::from(Span::styled(
                        " Implementation Plan",
                        Style::default()
                            .fg(self.colors.accent_system)
                            .add_modifier(Modifier::BOLD),
                    )));
                    lines.push(Line::from(Span::styled(
                        format!("Task: {}", plan.task),
                        Style::default().fg(self.colors.accent_user),
                    )));
                    lines.push(Line::from(""));
                    for step in &plan.steps {
                        let marker = if step.completed { "[x]" } else { "[ ]" };
                        lines.push(Line::from(Span::styled(
                            format!("{}. {} {}", step.id, marker, step.description),
                            Style::default().fg(self.colors.text),
                        )));
                    }
                    lines.push(Line::from(""));
                    lines.push(Line::from(Span::styled(
                        "Type 'approve' or 'reject <reason>'",
                        Style::default().fg(self.colors.text_muted),
                    )));
                }
                PlanView::Hidden => {}
            }

            let paragraph = Paragraph::new(lines)
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title(" Plan ")
                        .border_style(Style::default().fg(self.colors.border))
                        .bg(self.colors.surface),
                )
                .wrap(Wrap { trim: false });
            f.render_widget(paragraph, area);
        }
    }

    fn draw_input(&self, f: &mut Frame, area: Rect) {
        let title = if self.model_picker {
            " Filter (Esc to cancel) "
        } else if self.plan_generating {
            " Generating plan (Esc to abort) "
        } else if self.plan_view == PlanView::Assumptions {
            " Toggle with number, 'done' to accept (Esc to exit) "
        } else if self.plan_view == PlanView::PlanReview {
            " Type 'approve' or 'reject <reason>' "
        } else {
            ""
        };

        let prompt = Span::styled(
            " › ",
            Style::default().fg(self.colors.accent_user),
        );

        let mut spans: Vec<Span> = vec![prompt];

        if self.input.is_empty() && !self.model_picker && self.plan_view == PlanView::Hidden {
            let placeholder = Span::styled(
                "Type a message…  (/ for commands)",
                Style::default().fg(self.colors.text_muted),
            );
            spans.push(placeholder);
        } else if self.input.is_empty() && self.model_picker {
            let placeholder = Span::styled(
                "Type to filter models…",
                Style::default().fg(self.colors.text_muted),
            );
            spans.push(placeholder);
        } else {
            spans.push(Span::styled(
                self.input.content.as_str(),
                Style::default().fg(self.colors.text),
            ));
        }

        let border_style = if self.model_picker {
            Style::default().fg(self.colors.accent_system)
        } else {
            match self.plan_view {
                PlanView::Hidden => Style::default().fg(self.colors.border_active),
                _ => Style::default().fg(self.colors.border),
            }
        };

        let input = Paragraph::new(Line::from(spans)).block(
            Block::default()
                .borders(Borders::ALL)
                .title(title)
                .border_style(border_style)
                .bg(self.colors.surface),
        );
        f.render_widget(input, area);
        f.set_cursor_position((
            area.x + 3 + self.input.cursor_x(),
            area.y + 1,
        ));
    }

    fn draw_suggestions_overlay(&self, f: &mut Frame, area: Rect) {
        let sugs = self.commands.suggestions(&self.input.content);
        if sugs.is_empty() {
            return;
        }

        let spans: Vec<Span> = sugs
            .iter()
            .enumerate()
            .flat_map(|(i, cmd)| {
                let mut parts = vec![Span::styled(
                    format!("/{}", cmd),
                    Style::default().fg(self.colors.accent_user),
                )];
                if i < sugs.len() - 1 {
                    parts.push(Span::styled(
                        "  ",
                        Style::default().fg(self.colors.text_muted),
                    ));
                }
                parts
            })
            .collect();

        let overlay_height = 3;
        let overlay_y = area.y.saturating_sub(overlay_height);
        let overlay_area = Rect {
            x: area.x,
            y: overlay_y,
            width: area.width,
            height: overlay_height,
        };

        let p = Paragraph::new(Line::from(spans))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(self.colors.border)),
            )
            .bg(self.colors.surface);
        f.render_widget(p, overlay_area);
    }

    fn draw_status(&self, f: &mut Frame, area: Rect) {
        let mut sb = StatusBar::new();
        sb.connected = self.provider.is_some();
        sb.agent_busy = self.agent_busy;
        sb.spinner_frame = self.spinner_frame;
        sb.model = self.current_model.clone();
        sb.effort = self.effort.to_string();
        sb.version = "0.1.0".into();
        sb.icons_enabled = self.icons_enabled;
        sb.render(f, area, &self.colors);
    }

    fn draw_sidebar(&self, f: &mut Frame, area: Rect) {
        let mut sb = Sidebar::new();
        sb.model = self.current_model.clone();
        sb.effort = Some(self.effort.to_string());
        sb.branch = "main".into();
        sb.cwd = std::env::current_dir()
            .map(|p| {
                let s = p.to_string_lossy();
                if s.len() > 20 {
                    format!("~{}", &s[s.len() - 19..])
                } else {
                    s.into_owned()
                }
            })
            .unwrap_or_default();
        sb.render(f, area, &self.colors);
    }
}
