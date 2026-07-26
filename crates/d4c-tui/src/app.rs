use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
    Frame, Terminal,
};
use std::io;

use d4c_core::commands::CommandRegistry;
use d4c_core::plan::Plan;
use d4c_core::router::ModelRouter;
use crate::input::InputState;

pub struct App {
    running: bool,
    messages: Vec<(String, String)>,
    input: InputState,
    commands: CommandRegistry,
    router: ModelRouter,
    current_model: String,
    active_plan: Option<Plan>,
    plan_view: PlanView,
    building: bool,
    build_step: usize,
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
        }
    }

    pub fn run(&mut self) -> Result<()> {
        let mut stdout = io::stdout();
        crossterm::terminal::enable_raw_mode()?;
        crossterm::execute!(stdout, crossterm::terminal::EnterAlternateScreen)?;

        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;

        self.messages.push((
            "system".into(),
            "Welcome to d4c. Type /help for commands, or start chatting.".into(),
        ));

        while self.running {
            terminal.draw(|f| self.draw(f))?;

            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press || key.kind == KeyEventKind::Repeat {
                    match key.code {
                        KeyCode::Char('c')
                            if key.modifiers.contains(crossterm::event::KeyModifiers::CONTROL) =>
                        {
                            self.running = false;
                        }
                        // Backspace: KeyCode, DEL (0x7F), BS (0x08), Ctrl+H
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
                            // skip other ctrl combos
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

        self.messages.push(("user".into(), input.clone()));

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
                                self.messages.push((
                                    "system".into(),
                                    format!("Starting build. {} steps to execute.", plan.steps.len()),
                                ));
                                self.execute_build_step();
                            }
                            Some(_) => {
                                self.messages.push((
                                    "error".into(),
                                    "Plan not approved yet. Use /plan first, then approve it.".into(),
                                ));
                            }
                            None => {
                                self.messages.push((
                                    "error".into(),
                                    "No plan to build. Use /plan <task> to create one.".into(),
                                ));
                            }
                        }
                    } else if output == "__BUILD_NEXT__" {
                        if self.building {
                            self.execute_build_step();
                        } else {
                            self.messages.push(("error".into(), "No build in progress.".into()));
                        }
                    } else if output == "__BUILD_ABORT__" {
                        if self.building {
                            self.building = false;
                            self.plan_view = PlanView::Hidden;
                            self.messages.push(("system".into(), "Build paused.".into()));
                        }
                    } else if output.starts_with("__PLAN_START__") {
                        let json = &output["__PLAN_START__".len()..];
                        match serde_json::from_str::<Plan>(json) {
                            Ok(plan) => {
                                self.active_plan = Some(plan);
                                self.plan_view = PlanView::Questions;
                                self.messages.push((
                                    "system".into(),
                                    "Plan created! Answer the questions below to refine it.".into(),
                                ));
                            }
                            Err(e) => {
                                self.messages
                                    .push(("error".into(), format!("Failed to parse plan: {}", e)));
                            }
                        }
                    } else if input.trim() == "/clear" {
                        self.messages.clear();
                        self.messages
                            .push(("system".into(), "Context cleared.".into()));
                    } else {
                        self.messages.push(("system".into(), output));
                    }
                }
                Err(e) => {
                    self.messages
                        .push(("error".into(), format!("Error: {}", e)));
                }
            }
        } else {
            let decision = self.router.route(&input);
            self.current_model = decision.selected_model.clone();
            let response = self.mock_provider_respond(&input, &decision);
            self.messages.push(("assistant".into(), response));
        }
    }

    fn handle_plan_input(&mut self, input: String) {
        if let Some(ref mut plan) = self.active_plan {
            match self.plan_view {
                PlanView::Questions => {
                    let current_q = plan.questions.iter_mut().find(|q| q.answer.is_none());
                    if let Some(q) = current_q {
                        q.answer = Some(input.clone());
                        self.messages
                            .push(("user".into(), format!("Q{}: {}", q.id, input)));

                        let all_done = plan.questions.iter().all(|q| q.answer.is_some());
                        if all_done {
                            self.plan_view = PlanView::Assumptions;
                            self.messages.push((
                                "system".into(),
                                "All questions answered. Review the assumptions below.\nType the assumption number to toggle, or 'done' to proceed.".into(),
                            ));
                        }
                    }
                }
                PlanView::Assumptions => {
                    if input.trim().eq_ignore_ascii_case("done") {
                        plan.assumptions.iter_mut().for_each(|a| a.accepted = true);
                        plan.status = d4c_core::plan::PlanStatus::Draft;
                        self.plan_view = PlanView::PlanReview;
                        self.messages.push((
                            "system".into(),
                            "All assumptions accepted. Review the plan below.\nType 'approve' to accept, 'reject <reason>' to redo.".into(),
                        ));
                    } else if let Ok(num) = input.trim().parse::<usize>() {
                        if let Some(a) = plan.assumptions.iter_mut().find(|a| a.id == num) {
                            a.accepted = !a.accepted;
                            let status = if a.accepted { "accepted" } else { "rejected" };
                            self.messages.push((
                                "system".into(),
                                format!("Assumption {} {}:", num, status),
                            ));
                            self.messages
                                .push(("system".into(), format!("  {}", a.statement)));
                        }
                    }
                }
                PlanView::PlanReview => {
                    if input.trim().eq_ignore_ascii_case("approve") {
                        plan.status = d4c_core::plan::PlanStatus::Approved;
                        self.plan_view = PlanView::Hidden;
                        self.messages.push((
                            "system".into(),
                            "Plan approved! Type /build to execute, or continue chatting.".into(),
                        ));
                    } else if input.trim().to_lowercase().starts_with("reject") {
                        let reason = input.trim()["reject".len()..].trim();
                        self.messages.push((
                            "system".into(),
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
                self.messages.push((
                    "system".into(),
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
                    self.messages.push((
                        "system".into(),
                        "Build completed! All steps executed.".into(),
                    ));
                } else {
                    self.messages.push((
                        "system".into(),
                        "Checkpoint: type /build continue to proceed, /build abort to pause.".into(),
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
        if self.plan_view != PlanView::Hidden {
            self.draw_plan_view(f);
            return;
        }

        let show_suggestions = self.input.content.starts_with('/');
        let constraints = if show_suggestions {
            vec![
                Constraint::Min(3),
                Constraint::Length(1),
                Constraint::Length(3),
                Constraint::Length(1),
            ]
        } else {
            vec![
                Constraint::Min(3),
                Constraint::Length(3),
                Constraint::Length(1),
            ]
        };

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints(constraints)
            .split(f.area());

        self.draw_messages(f, chunks[0]);
        if show_suggestions {
            self.draw_suggestions(f, chunks[1]);
            self.draw_input(f, chunks[2]);
            self.draw_status(f, chunks[3]);
        } else {
            self.draw_input(f, chunks[1]);
            self.draw_status(f, chunks[2]);
        }
    }

    fn draw_plan_view(&self, f: &mut Frame) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Percentage(60),
                Constraint::Percentage(40),
            ])
            .split(f.area());

        self.draw_messages(f, chunks[0]);
        self.draw_plan_panel(f, chunks[1]);
    }

    fn draw_plan_panel(&self, f: &mut Frame, area: Rect) {
        if let Some(ref plan) = self.active_plan {
            let mut lines: Vec<Line> = Vec::new();

            match self.plan_view {
                PlanView::Questions => {
                    lines.push(Line::from(Span::styled(
                        " Questions",
                        Style::default().fg(Color::Yellow).add_modifier(ratatui::style::Modifier::BOLD),
                    )));
                    lines.push(Line::from(""));
                    for q in &plan.questions {
                        let marker = if q.answer.is_some() { "[x]" } else { "[ ]" };
                        let style = if q.answer.is_some() {
                            Style::default().fg(Color::DarkGray)
                        } else {
                            Style::default().fg(Color::White)
                        };
                        lines.push(Line::from(Span::styled(
                            format!("{} Q{}: {}", marker, q.id, q.text),
                            style,
                        )));
                        if let Some(ref answer) = q.answer {
                            lines.push(Line::from(Span::styled(
                                format!("     -> {}", answer),
                                Style::default().fg(Color::Green),
                            )));
                        }
                        if !q.options.is_empty() && q.answer.is_none() {
                            for opt in &q.options {
                                lines.push(Line::from(Span::styled(
                                    format!("     - {}", opt),
                                    Style::default().fg(Color::DarkGray),
                                )));
                            }
                        }
                    }
                }
                PlanView::Assumptions => {
                    lines.push(Line::from(Span::styled(
                        " Assumptions (toggle with number, 'done' to accept all)",
                        Style::default().fg(Color::Yellow).add_modifier(ratatui::style::Modifier::BOLD),
                    )));
                    lines.push(Line::from(""));
                    for a in &plan.assumptions {
                        let marker = if a.accepted { "[x]" } else { "[ ]" };
                        let style = if a.accepted {
                            Style::default().fg(Color::Green)
                        } else {
                            Style::default().fg(Color::White)
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
                        Style::default().fg(Color::Yellow).add_modifier(ratatui::style::Modifier::BOLD),
                    )));
                    lines.push(Line::from(Span::styled(
                        format!("Task: {}", plan.task),
                        Style::default().fg(Color::Cyan),
                    )));
                    lines.push(Line::from(""));
                    for step in &plan.steps {
                        let marker = if step.completed { "[x]" } else { "[ ]" };
                        lines.push(Line::from(Span::styled(
                            format!("{}. {} {}", step.id, marker, step.description),
                            Style::default().fg(Color::White),
                        )));
                    }
                    lines.push(Line::from(""));
                    lines.push(Line::from(Span::styled(
                        "Type 'approve' or 'reject <reason>'",
                        Style::default().fg(Color::DarkGray),
                    )));
                }
                PlanView::Hidden => {}
            }

            let paragraph = Paragraph::new(lines)
                .block(Block::default().borders(Borders::ALL).title("Plan"))
                .wrap(Wrap { trim: false });
            f.render_widget(paragraph, area);
        }
    }

    fn draw_messages(&self, f: &mut Frame, area: Rect) {
        let lines: Vec<Line> = self
            .messages
            .iter()
            .map(|(role, content)| {
                let style = match role.as_str() {
                    "user" => Style::default().fg(Color::Cyan),
                    "assistant" => Style::default().fg(Color::Green),
                    "system" => Style::default().fg(Color::DarkGray),
                    "error" => Style::default().fg(Color::Red),
                    _ => Style::default().fg(Color::White),
                };
                Line::from(Span::styled(
                    format!("[{}] {}", role, content),
                    style,
                ))
            })
            .collect();

        let paragraph = Paragraph::new(lines)
            .block(Block::default().borders(Borders::ALL).title("D4C"))
            .wrap(Wrap { trim: false });
        f.render_widget(paragraph, area);
    }

    fn draw_input(&self, f: &mut Frame, area: Rect) {
        let title = if self.plan_view != PlanView::Hidden {
            "Plan Input (Esc to exit plan)"
        } else {
            "Input"
        };
        let input = Paragraph::new(self.input.content.as_str())
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(title)
                    .border_style(Style::default().fg(Color::Yellow)),
            );
        f.render_widget(input, area);
        f.set_cursor_position((
            area.x + 1 + self.input.cursor_x(),
            area.y + 1,
        ));
    }

    fn draw_suggestions(&self, f: &mut Frame, area: Rect) {
        if !self.input.content.starts_with('/') {
            return;
        }
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
                    Style::default().fg(Color::Cyan),
                )];
                if i < sugs.len() - 1 {
                    parts.push(Span::styled("  ", Style::default().fg(Color::DarkGray)));
                }
                parts
            })
            .collect();
        let p = Paragraph::new(Line::from(spans))
            .block(Block::default().borders(Borders::ALL).border_style(Style::default().fg(Color::DarkGray)));
        f.render_widget(p, area);
    }

    fn draw_status(&self, f: &mut Frame, area: Rect) {
        let plan_status = if let Some(ref plan) = self.active_plan {
            format!(" | plan: {:?}", plan.status)
        } else {
            String::new()
        };
        let status = Paragraph::new(Line::from(vec![
            Span::styled(" d4c ", Style::default().fg(Color::Black).bg(Color::Blue)),
            Span::raw(format!(
                " | model: {}{} | /help for commands",
                self.current_model, plan_status
            )),
        ]));
        f.render_widget(status, area);
    }
}
