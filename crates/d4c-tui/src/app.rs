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
use std::time::{Duration, Instant};

use d4c_core::commands::CommandRegistry;
use d4c_core::plan::Plan;
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
    active_plan: Option<Plan>,
    plan_view: PlanView,
    building: bool,
    build_step: usize,
    colors: Colors,
    agent_busy: bool,
    spinner_frame: usize,
    icons_enabled: bool,
    start_time: Instant,
}

#[derive(PartialEq)]
enum PlanView {
    Hidden,
    Questions,
    Assumptions,
    PlanReview,
}

impl App {
    pub fn new() -> Self {
        let mut router = ModelRouter::new();
        router.load_default_catalog();
        let initial_model = router.route("hello").selected_model.clone();

        Self {
            running: true,
            messages: Vec::new(),
            input: InputState::new(),
            commands: CommandRegistry::new(),
            router,
            current_model: initial_model,
            active_plan: None,
            plan_view: PlanView::Hidden,
            building: false,
            build_step: 0,
            colors: Colors::default(),
            agent_busy: false,
            spinner_frame: 0,
            icons_enabled: true,
            start_time: Instant::now(),
        }
    }

    pub fn run(&mut self) -> Result<()> {
        let mut stdout = io::stdout();
        crossterm::terminal::enable_raw_mode()?;
        crossterm::execute!(stdout, crossterm::terminal::EnterAlternateScreen)?;

        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;

        self.messages.push(ChatMessage::new(
            Role::System,
            "Welcome to d4c. Type /help for commands, or start chatting.",
        ));

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
                            | KeyCode::Char('\x08') => self.input.delete_char(),
                            KeyCode::Char('h')
                                if key.modifiers.contains(crossterm::event::KeyModifiers::CONTROL) =>
                            {
                                self.input.delete_char();
                            }
                            KeyCode::Esc => {
                                if self.plan_view != PlanView::Hidden {
                                    self.plan_view = PlanView::Hidden;
                                }
                            }
                            KeyCode::Enter => {
                                let input = self.input.submit();
                                if !input.is_empty() {
                                    self.handle_input(input);
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
                            KeyCode::Char(_c)
                                if key.modifiers.contains(crossterm::event::KeyModifiers::CONTROL) =>
                            {
                            }
                            KeyCode::Char(c) => self.input.insert_char(c),
                            KeyCode::Left => self.input.move_left(),
                            KeyCode::Right => self.input.move_right(),
                            KeyCode::Up => self.input.scroll_up(),
                            KeyCode::Down => self.input.scroll_down(),
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
                        match &self.active_plan {
                            Some(plan) if plan.status == d4c_core::plan::PlanStatus::Approved => {
                                self.building = true;
                                self.build_step = 0;
                                self.plan_view = PlanView::PlanReview;
                                self.messages.push(ChatMessage::new(
                                    Role::System,
                                    format!(
                                        "Starting build. {} steps to execute.",
                                        plan.steps.len()
                                    ),
                                ));
                                self.execute_build_step();
                            }
                            Some(_) => {
                                self.messages.push(ChatMessage::new(
                                    Role::Error,
                                    "Plan not approved yet. Use /plan first, then approve it.",
                                ));
                            }
                            None => {
                                self.messages.push(ChatMessage::new(
                                    Role::Error,
                                    "No plan to build. Use /plan <task> to create one.",
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
                                Role::System,
                                "Build paused.",
                            ));
                        }
                    } else if output.starts_with("__PLAN_START__") {
                        let json = &output["__PLAN_START__".len()..];
                        match serde_json::from_str::<Plan>(json) {
                            Ok(plan) => {
                                self.active_plan = Some(plan);
                                self.plan_view = PlanView::Questions;
                                self.messages.push(ChatMessage::new(
                                    Role::System,
                                    "Plan created! Answer the questions below to refine it.",
                                ));
                            }
                            Err(e) => {
                                self.messages.push(ChatMessage::new(
                                    Role::Error,
                                    format!("Failed to parse plan: {}", e),
                                ));
                            }
                        }
                    } else if input.trim() == "/clear" {
                        self.messages.clear();
                        self.messages.push(ChatMessage::new(
                            Role::System,
                            "Context cleared.",
                        ));
                    } else {
                        self.messages.push(ChatMessage::new(Role::System, output));
                    }
                }
                Err(e) => {
                    self.messages
                        .push(ChatMessage::new(Role::Error, format!("Error: {}", e)));
                }
            }
        } else {
            let decision = self.router.route(&input);
            self.current_model = decision.selected_model.clone();
            let response = self.mock_provider_respond(&input, &decision);
            self.messages
                .push(ChatMessage::new(Role::Agent, response));
        }
    }

    fn handle_plan_input(&mut self, input: String) {
        if let Some(ref mut plan) = self.active_plan {
            match self.plan_view {
                PlanView::Questions => {
                    let current_q = plan.questions.iter_mut().find(|q| q.answer.is_none());
                    if let Some(q) = current_q {
                        q.answer = Some(input.clone());
                        self.messages.push(ChatMessage::new(
                            Role::User,
                            format!("Q{}: {}", q.id, input),
                        ));

                        let all_done = plan.questions.iter().all(|q| q.answer.is_some());
                        if all_done {
                            self.plan_view = PlanView::Assumptions;
                            self.messages.push(ChatMessage::new(
                                Role::System,
                                "All questions answered. Review the assumptions below.\nType the assumption number to toggle, or 'done' to proceed.",
                            ));
                        }
                    }
                }
                PlanView::Assumptions => {
                    if input.trim().eq_ignore_ascii_case("done") {
                        plan.assumptions.iter_mut().for_each(|a| a.accepted = true);
                        plan.status = d4c_core::plan::PlanStatus::Draft;
                        self.plan_view = PlanView::PlanReview;
                        self.messages.push(ChatMessage::new(
                            Role::System,
                            "All assumptions accepted. Review the plan below.\nType 'approve' to accept, 'reject <reason>' to redo.",
                        ));
                    } else if let Ok(num) = input.trim().parse::<usize>() {
                        if let Some(a) = plan.assumptions.iter_mut().find(|a| a.id == num) {
                            a.accepted = !a.accepted;
                            let status = if a.accepted { "accepted" } else { "rejected" };
                            self.messages.push(ChatMessage::new(
                                Role::System,
                                format!("Assumption {} {}:", num, status),
                            ));
                            self.messages
                                .push(ChatMessage::new(Role::System, format!("  {}", a.statement)));
                        }
                    }
                }
                PlanView::PlanReview => {
                    if input.trim().eq_ignore_ascii_case("approve") {
                        plan.status = d4c_core::plan::PlanStatus::Approved;
                        self.plan_view = PlanView::Hidden;
                        self.messages.push(ChatMessage::new(
                            Role::System,
                            "Plan approved! Type /build to execute, or continue chatting.",
                        ));
                    } else if input.trim().to_lowercase().starts_with("reject") {
                        let reason = input.trim()["reject".len()..].trim();
                        self.messages.push(ChatMessage::new(
                            Role::System,
                            format!("Plan rejected: {}. Re-running questionnaire...", reason),
                        ));
                        plan.status = d4c_core::plan::PlanStatus::Rejected;
                        plan.questions.iter_mut().for_each(|q| q.answer = None);
                        plan.assumptions.iter_mut().for_each(|a| a.accepted = false);
                        self.plan_view = PlanView::Questions;
                    }
                }
                PlanView::Hidden => {}
            }
        }
    }

    fn execute_build_step(&mut self) {
        if let Some(ref mut plan) = self.active_plan {
            if self.build_step < plan.steps.len() {
                let total = plan.steps.len();
                let description = plan.steps[self.build_step].description.clone();
                plan.steps[self.build_step].completed = true;
                self.messages.push(ChatMessage::new(
                    Role::Tool,
                    format!(
                        "Step {}/{}: {} [DONE]",
                        self.build_step + 1,
                        total,
                        description
                    ),
                ));
                self.build_step += 1;

                if self.build_step >= plan.steps.len() {
                    self.building = false;
                    plan.status = d4c_core::plan::PlanStatus::Completed;
                    self.plan_view = PlanView::Hidden;
                    self.messages.push(ChatMessage::new(
                        Role::System,
                        "Build completed! All steps executed.",
                    ));
                } else {
                    self.messages.push(ChatMessage::new(
                        Role::System,
                        "Checkpoint: type /build continue to proceed, /build abort to pause.",
                    ));
                }
            }
        }
    }

    fn mock_provider_respond(
        &self,
        input: &str,
        decision: &d4c_core::router::RoutingDecision,
    ) -> String {
        format!(
            "[routed to {} ({})]\nReceived: \"{}\"\n\n\
             This is a placeholder response. The OpenCode provider integration \
             will replace this with real model output.",
            decision.selected_model,
            decision.tier,
            input.chars().take(80).collect::<String>()
        )
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

        if self.plan_view != PlanView::Hidden {
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
                PlanView::Questions => {
                    lines.push(Line::from(Span::styled(
                        " Questions",
                        Style::default()
                            .fg(self.colors.accent_system)
                            .add_modifier(Modifier::BOLD),
                    )));
                    lines.push(Line::from(""));
                    for q in &plan.questions {
                        let answered = q.answer.is_some();
                        let marker = if answered { "[x]" } else { "[ ]" };
                        let style = if answered {
                            Style::default().fg(self.colors.text_muted)
                        } else {
                            Style::default().fg(self.colors.text)
                        };
                        lines.push(Line::from(Span::styled(
                            format!("{} Q{}: {}", marker, q.id, q.text),
                            style,
                        )));
                        if let Some(ref answer) = q.answer {
                            lines.push(Line::from(Span::styled(
                                format!("     -> {}", answer),
                                Style::default().fg(self.colors.accent_success),
                            )));
                        }
                        if !q.options.is_empty() && !answered {
                            for opt in &q.options {
                                lines.push(Line::from(Span::styled(
                                    format!("     - {}", opt),
                                    Style::default().fg(self.colors.text_muted),
                                )));
                            }
                        }
                    }
                }
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
        let title = if self.plan_view != PlanView::Hidden {
            " Plan Input (Esc to exit) "
        } else {
            ""
        };

        let prompt = Span::styled(
            " › ",
            Style::default().fg(self.colors.accent_user),
        );

        let mut spans: Vec<Span> = vec![prompt];

        if self.input.is_empty() && self.plan_view == PlanView::Hidden {
            let placeholder = Span::styled(
                "Type a message…  (/ for commands)",
                Style::default().fg(self.colors.text_muted),
            );
            spans.push(placeholder);
        } else {
            spans.push(Span::styled(
                self.input.content.as_str(),
                Style::default().fg(self.colors.text),
            ));
        }

        let border_style = match self.plan_view {
            PlanView::Hidden => Style::default().fg(self.colors.border_active),
            _ => Style::default().fg(self.colors.border),
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
        sb.connected = true;
        sb.agent_busy = self.agent_busy;
        sb.spinner_frame = self.spinner_frame;
        sb.model = self.current_model.clone();
        sb.version = "0.1.0".into();
        sb.icons_enabled = self.icons_enabled;
        sb.render(f, area, &self.colors);
    }

    fn draw_sidebar(&self, f: &mut Frame, area: Rect) {
        let mut sb = Sidebar::new();
        sb.model = self.current_model.clone();
        let d = self.start_time.elapsed();
        sb.elapsed = format!(
            "{:02}:{:02}:{:02}",
            d.as_secs() / 3600,
            (d.as_secs() / 60) % 60,
            d.as_secs() % 60
        );
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
