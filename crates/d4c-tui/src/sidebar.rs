use ratatui::{
    layout::Rect,
    style::{Style, Stylize},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
    Frame,
};

use crate::theme::Colors;

pub struct Sidebar {
    pub model: String,
    pub effort: Option<String>,
    pub tokens_used: Option<usize>,
    pub tokens_max: Option<usize>,
    pub branch: String,
    pub cwd: String,
    pub files_changed: Option<usize>,
}

impl Sidebar {
    pub fn new() -> Self {
        Self {
            model: String::new(),
            effort: None,
            tokens_used: None,
            tokens_max: None,
            branch: String::new(),
            cwd: String::new(),
            files_changed: None,
        }
    }

    pub fn render(&self, f: &mut Frame, area: Rect, colors: &Colors) {
        let label_style = Style::default().fg(colors.text_muted);
        let value_style = Style::default().fg(colors.text);

        let block = Block::default()
            .title(" session ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(colors.border))
            .bg(colors.surface);

        let inner = block.inner(area);
        f.render_widget(block, area);

        let mut session_rows: Vec<Line> = Vec::new();
        session_rows.push(Line::from(vec![
            Span::styled("model  ", label_style),
            Span::styled(&self.model, value_style),
        ]));
        session_rows.push(Line::from(vec![
            Span::styled("effort ", label_style),
            Span::styled(
                self.effort.as_deref().unwrap_or("—"),
                value_style,
            ),
        ]));
        session_rows.push(Line::from(vec![
            Span::styled("tokens ", label_style),
            Span::styled(
                match (self.tokens_used, self.tokens_max) {
                    (Some(u), Some(m)) => format!("{} / {}", u, m),
                    (Some(u), None) => format!("{} / —", u),
                    (None, _) => "— / —".into(),
                },
                value_style,
            ),
        ]));
        session_rows.push(Line::from(vec![
            Span::styled("branch ", label_style),
            Span::styled(&self.branch, value_style),
        ]));
        session_rows.push(Line::from(vec![
            Span::styled("cwd    ", label_style),
            Span::styled(&self.cwd, value_style),
        ]));
        session_rows.push(Line::from(vec![
            Span::styled("files  ", label_style),
            Span::styled(
                self.files_changed
                    .map(|n| format!("{} changed", n))
                    .unwrap_or_else(|| "—".into()),
                value_style,
            ),
        ]));

        let session_para = Paragraph::new(session_rows).wrap(Wrap { trim: false });
        f.render_widget(session_para, inner);
    }
}
