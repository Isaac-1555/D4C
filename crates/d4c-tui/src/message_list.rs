use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
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

    /// Wrap a single line (no embedded newlines) to `max_width` columns.
    /// Returns a list of wrapped sub-lines.
    fn wrap_single_line(text: &str, max_width: usize) -> Vec<String> {
        if max_width < 2 {
            return vec![text.to_string()];
        }

        let mut lines = Vec::new();
        let mut current = String::new();

        for word in text.split_inclusive(char::is_whitespace) {
            let would_be = if current.is_empty() {
                word.trim_end().len()
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

    /// Split text on newlines first (preserving paragraph breaks), then
    /// wrap each line to `max_width`. This ensures the model's line breaks
    /// are respected instead of collapsing them into one reflowed paragraph.
    fn wrap_text(text: &str, max_width: usize) -> Vec<String> {
        let mut result = Vec::new();
        for line in text.lines() {
            for wrapped in Self::wrap_single_line(line, max_width) {
                result.push(wrapped);
            }
        }
        if result.is_empty() {
            result.push(String::new());
        }
        result
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

    /// Parse a single line of text into styled spans, handling inline
    /// markdown: `**bold**` and `` `code` ``.  Regular text is returned
    /// as a plain span.  Nested formatting is not supported (good enough
    /// for LLM responses which rarely nest).
    fn parse_markdown_spans(text: &str, colors: &Colors) -> Vec<Span<'static>> {
        let mut spans = Vec::new();
        let mut remaining = text;

        while !remaining.is_empty() {
            // Look for the nearest `**` or backtick
            let bold_pos = remaining.find("**");
            let code_pos = remaining.find('`');

            let pick = match (bold_pos, code_pos) {
                (Some(b), Some(c)) => {
                    if b <= c { Some(('b', b)) } else { Some(('c', c)) }
                }
                (Some(b), None) => Some(('b', b)),
                (None, Some(c)) => Some(('c', c)),
                (None, None) => None,
            };

            match pick {
                Some(('b', pos)) => {
                    // Need a matching closing **
                    if let Some(close) = remaining[pos + 2..].find("**") {
                        if pos > 0 {
                            spans.push(Span::styled(
                                remaining[..pos].to_string(),
                                Style::default().fg(colors.text),
                            ));
                        }
                        let bold_text = &remaining[pos + 2..pos + 2 + close];
                        spans.push(Span::styled(
                            bold_text.to_string(),
                            Style::default()
                                .fg(colors.accent_agent
                                )
                                .add_modifier(Modifier::BOLD),
                        ));
                        remaining = &remaining[pos + 2 + close + 2..];
                    } else {
                        // No closing ** — treat rest as plain
                        spans.push(Span::styled(
                            remaining.to_string(),
                            Style::default().fg(colors.text),
                        ));
                        break;
                    }
                }
                Some(('c', pos)) => {
                    if let Some(close) = remaining[pos + 1..].find('`') {
                        if pos > 0 {
                            spans.push(Span::styled(
                                remaining[..pos].to_string(),
                                Style::default().fg(colors.text),
                            ));
                        }
                        let code_text = &remaining[pos + 1..pos + 1 + close];
                        spans.push(Span::styled(
                            code_text.to_string(),
                            Style::default().fg(colors.accent_system),
                        ));
                        remaining = &remaining[pos + 1 + close + 1..];
                    } else {
                        spans.push(Span::styled(
                            remaining.to_string(),
                            Style::default().fg(colors.text),
                        ));
                        break;
                    }
                }
                None => {
                    spans.push(Span::styled(
                        remaining.to_string(),
                        Style::default().fg(colors.text),
                    ));
                    break;
                }
                // Unreachable — pick only ever yields 'b' or 'c'.
                _ => {
                    spans.push(Span::styled(
                        remaining.to_string(),
                        Style::default().fg(colors.text),
                    ));
                    break;
                }
            }
        }

        if spans.is_empty() {
            spans.push(Span::raw(""));
        }
        spans
    }

    /// Build the styled spans for a content line, optionally with markdown
    /// parsing (applied to Agent messages; Tool/Error stay plain).
    fn line_spans(&self, text: &str, use_markdown: bool) -> Vec<Span<'static>> {
        if use_markdown {
            // Check for markdown headers (# …)
            let trimmed = text.trim_start();
            if trimmed.starts_with("# ") || trimmed.starts_with("## ") || trimmed.starts_with("### ") {
                let header_text = trimmed.trim_start_matches('#').trim();
                return vec![
                    Span::styled(
                        header_text.to_string(),
                        Style::default()
                            .fg(self.colors.accent_agent)
                            .add_modifier(Modifier::BOLD),
                    ),
                ];
            }
            Self::parse_markdown_spans(text, &self.colors)
        } else {
            vec![Span::styled(
                text.to_string(),
                Style::default().fg(self.colors.text),
            )]
        }
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
            // Only Agent messages get markdown rendering; User, Tool,
            // Error stay plain text.
            let use_markdown = msg.role == Role::Agent;

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
                let gutter_span = Span::styled(gutter, Style::default().fg(role_color));

                if i == 0 && is_action {
                    let glyph = self.action_glyph(msg);
                    let prefix = if content_line.is_empty() {
                        format!(" {}", glyph)
                    } else {
                        format!(" {} ", glyph)
                    };
                    let mut line_spans = vec![
                        gutter_span,
                        Span::styled(prefix, Style::default().fg(self.colors.text)),
                    ];
                    // Remaining content after glyph prefix
                    if !content_line.is_empty() {
                        line_spans.extend(self.line_spans(content_line, use_markdown));
                    }
                    lines.push(Line::from(line_spans));
                } else {
                    let mut line_spans = vec![gutter_span];
                    // Indent continuation lines with a space
                    line_spans.push(Span::raw(" "));
                    line_spans.extend(self.line_spans(content_line, use_markdown));
                    lines.push(Line::from(line_spans));
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