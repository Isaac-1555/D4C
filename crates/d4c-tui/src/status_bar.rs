use ratatui::{
    layout::Rect,
    style::{Style, Stylize},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

use crate::theme::Colors;

const SPINNER_BRAILLE: &[char] = &['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];
const SPINNER_ASCII: &[char] = &['|', '/', '-', '\\', '|', '/', '-', '\\', '|', '/'];

const DOT_ON: &str = "●";
const DOT_ON_ASCII: &str = "o";

pub struct StatusBar {
    pub connected: bool,
    pub agent_busy: bool,
    pub spinner_frame: usize,
    pub streaming_status: String,
    pub model: String,
    pub effort: String,
    pub version: String,
    pub icons_enabled: bool,
}

impl StatusBar {
    pub fn new() -> Self {
        Self {
            connected: false,
            agent_busy: false,
            spinner_frame: 0,
            streaming_status: String::new(),
            model: String::new(),
            effort: String::new(),
            version: String::new(),
            icons_enabled: true,
        }
    }

    fn dot_char(&self) -> String {
        if self.agent_busy {
            let spinner = if self.icons_enabled {
                SPINNER_BRAILLE
            } else {
                SPINNER_ASCII
            };
            format!(" {} ", spinner[self.spinner_frame % spinner.len()])
        } else {
            let dot = if self.icons_enabled { DOT_ON } else { DOT_ON_ASCII };
            format!(" {} ", dot)
        }
    }

    fn dot_color(&self, colors: &Colors) -> ratatui::style::Color {
        if self.agent_busy {
            colors.accent_system
        } else if self.connected {
            colors.accent_success
        } else {
            colors.accent_error
        }
    }

    pub fn render(&self, f: &mut Frame, area: Rect, colors: &Colors) {
        let dot = self.dot_char();
        let dot_color = self.dot_color(colors);

        let dot_span = Span::styled(
            dot,
            Style::default().fg(dot_color).bg(colors.surface),
        );

        // Optional streaming status text ("Thinking…", "Generating…")
        // shown next to the spinner while the LLM is working.
        let status_span = if self.agent_busy && !self.streaming_status.is_empty() {
            format!(" {} ", self.streaming_status)
        } else {
            String::new()
        };
        let status_span = Span::styled(
            status_span,
            Style::default().fg(colors.accent_system).bg(colors.surface),
        );

        let effort_tag = if !self.effort.is_empty() {
            format!(" [{}]", self.effort)
        } else {
            String::new()
        };
        let model_span = Span::styled(
            format!(" {}{} ", self.model, effort_tag),
            Style::default().fg(colors.text).bg(colors.surface),
        );

        let version_tag = format!(" {} ", self.version);
        let version_span = Span::styled(
            version_tag,
            Style::default().fg(colors.text_muted).bg(colors.bg),
        );

        let dot_w = dot_span.width();
        let status_w = status_span.width();
        let model_w = model_span.width();
        let version_w = version_span.width();
        let avail = area.width as usize;

        if dot_w + status_w + model_w + 1 + version_w <= avail {
            let pad = avail.saturating_sub(dot_w + status_w + model_w + version_w);
            let line = Line::from(vec![
                dot_span,
                status_span,
                model_span,
                Span::styled(" ".repeat(pad), Style::default().bg(colors.bg)),
                version_span,
            ]);
            let paragraph = Paragraph::new(line).bg(colors.bg);
            f.render_widget(paragraph, area);
        } else {
            let line = Line::from(vec![dot_span, status_span, model_span]);
            let paragraph = Paragraph::new(line).bg(colors.bg);
            f.render_widget(paragraph, area);
        }
    }
}
