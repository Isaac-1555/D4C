use ratatui::{
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::{Paragraph, Wrap},
    Frame,
};

use crate::theme::Colors;
use crate::message::{ChatMessage, Role};

const GLYPH_OK: &str = "✓";
const GLYPH_ERR: &str = "✗";
const GLYPH_TOOL: &str = "⚙";
const GLYPH_GUTTER: &str = "▍";

const GLYPH_ERR_ASCII: &str = "[!!]";
const GLYPH_TOOL_ASCII: &str = "[i]";
const GLYPH_GUTTER_ASCII: &str = "│";

pub struct MessageList<'a> {
    pub messages: &'a [ChatMessage],
    pub colors: &'a Colors,
    pub icons_enabled: bool,
}

impl<'a> MessageList<'a> {
    pub fn new(messages: &'a [ChatMessage], colors: &'a Colors) -> Self {
        Self {
            messages,
            colors,
            icons_enabled: true,
        }
    }

    fn gutter(&self) -> &'static str {
        if self.icons_enabled {
            GLYPH_GUTTER
        } else {
            GLYPH_GUTTER_ASCII
        }
    }

    fn role_color(&self, role: &Role) -> ratatui::style::Color {
        match role {
            Role::User => self.colors.accent_user,
            Role::Agent => self.colors.accent_agent,
            Role::Tool => self.colors.accent_system,
            Role::Error => self.colors.accent_error,
        }
    }

    fn wrap_text(text: &str, max_width: usize) -> Vec<String> {
        if max_width < 2 {
            return vec![text.to_string()];
        }

        let mut lines = Vec::new();
        let mut current = String::new();

        for word in text.split_inclusive(char::is_whitespace) {
            let word_trimmed = word.trim_end();
            let would_be = if current.is_empty() {
                word_trimmed.len()
            } else {
                current.len() + word.len()
            };

            if would_be > max_width && !current.is_empty() {
                lines.push(current.trim_end().to_string());
                current = String::new();
            }
            current.push_str(word);
        }
        if !current.is_empty() {
            lines.push(current.trim_end().to_string());
        }

        if lines.is_empty() {
            lines.push(String::new());
        }

        lines
    }

    fn is_action_message(msg: &ChatMessage) -> bool {
        msg.role == Role::Tool
    }

    fn action_glyph(&self, msg: &ChatMessage) -> &'static str {
        let c = msg.content.trim();
        if c.starts_with("✓") {
            return GLYPH_OK;
        }
        let using_icons = self.icons_enabled;
        if c.starts_with("✗") || msg.role == Role::Error || c.starts_with("Error") {
            return if using_icons { GLYPH_ERR } else { GLYPH_ERR_ASCII };
        }
        if msg.role == Role::Tool {
            return if using_icons { GLYPH_TOOL } else { GLYPH_TOOL_ASCII };
        }
        if c.starts_with("Step") || c.starts_with("Starting") || c.starts_with("Build") || c.starts_with("Checkpoint") {
            return if using_icons { GLYPH_TOOL } else { GLYPH_TOOL_ASCII };
        }
        if using_icons { GLYPH_TOOL } else { GLYPH_TOOL_ASCII }
    }

    pub fn render(&self, f: &mut Frame, area: Rect) {
        if area.width < 4 || area.height < 1 {
            return;
        }

        let content_width = area.width.saturating_sub(3) as usize;
        let gutter = self.gutter();

        let mut lines: Vec<Line<'static>> = Vec::new();

        for msg in self.messages {
            let role_color = self.role_color(&msg.role);
            let is_action = Self::is_action_message(msg);

            let display_content = if is_action {
                let trimmed = msg.content.trim();
                let content = if trimmed.starts_with("✓") || trimmed.starts_with("✗") {
                    trimmed[3..].trim()
                } else {
                    &msg.content
                };
                content.to_string()
            } else {
                msg.content.clone()
            };

            let content_lines = Self::wrap_text(&display_content, content_width.saturating_sub(2));

            for (i, content_line) in content_lines.iter().enumerate() {
                if i == 0 {
                    let content_prefix = if is_action {
                        let glyph = self.action_glyph(msg);
                        if content_line.is_empty() {
                            format!(" {}", glyph)
                        } else {
                            format!(" {} {}", glyph, content_line)
                        }
                    } else {
                        format!(" {}", content_line)
                    };

                    lines.push(Line::from(vec![
                        Span::styled(gutter, Style::default().fg(role_color)),
                        Span::styled(content_prefix, Style::default().fg(self.colors.text)),
                    ]));
                } else {
                    lines.push(Line::from(vec![
                        Span::styled(gutter, Style::default().fg(role_color)),
                        Span::styled(content_line.clone(), Style::default().fg(self.colors.text)),
                    ]));
                }
            }

            lines.push(Line::from(""));
        }

        if lines.is_empty() {
            return;
        }

        let paragraph = Paragraph::new(lines).wrap(Wrap { trim: false });
        f.render_widget(paragraph, area);
    }
}
