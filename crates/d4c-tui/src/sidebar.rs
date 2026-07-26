use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Style, Stylize, Modifier},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
    Frame,
};

use crate::theme::Colors;

const GUTTER: &str = "▍";

pub struct Sidebar {
    pub model: String,
    pub tokens_used: Option<usize>,
    pub tokens_max: Option<usize>,
    pub elapsed: String,
    pub branch: String,
    pub cwd: String,
    pub files_changed: Option<usize>,
}

impl Sidebar {
    pub fn new() -> Self {
        Self {
            model: String::new(),
            tokens_used: None,
            tokens_max: None,
            elapsed: "00:00:00".into(),
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

        let chunks = Layout::vertical([Constraint::Fill(1), Constraint::Length(1)]).split(inner);

        let mut session_rows: Vec<Line> = Vec::new();
        session_rows.push(Line::from(vec![
            Span::styled("model  ", label_style),
            Span::styled(&self.model, value_style),
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
            Span::styled("time   ", label_style),
            Span::styled(&self.elapsed, value_style),
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
        f.render_widget(session_para, chunks[0]);

        let legend: Vec<Span> = [
            ("you", colors.accent_user),
            ("agent", colors.accent_agent),
            ("tool", colors.accent_system),
            ("system", colors.accent_system),
        ]
        .iter()
        .flat_map(|(label, color)| {
            vec![
                Span::styled(GUTTER, Style::default().fg(*color)),
                Span::styled(
                    format!("{} ", label),
                    Style::default().fg(*color).add_modifier(Modifier::BOLD),
                ),
            ]
        })
        .collect();

        let legend_para = Paragraph::new(Line::from(legend));
        f.render_widget(legend_para, chunks[1]);
    }
}
