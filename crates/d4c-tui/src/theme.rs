use ratatui::style::Color;

#[derive(Debug, Clone)]
pub struct Colors {
    pub bg: Color,
    pub surface: Color,
    pub border: Color,
    pub border_active: Color,
    pub text: Color,
    pub text_muted: Color,
    pub accent_user: Color,
    pub accent_agent: Color,
    pub accent_system: Color,
    pub accent_error: Color,
    pub accent_success: Color,
}

impl Default for Colors {
    fn default() -> Self {
        Self {
            bg: Color::Rgb(0x12, 0x13, 0x1c),
            surface: Color::Rgb(0x1a, 0x1b, 0x26),
            border: Color::Rgb(0x2c, 0x2e, 0x42),
            border_active: Color::Rgb(0x56, 0x5f, 0x89),
            text: Color::Rgb(0xc8, 0xcc, 0xdb),
            text_muted: Color::Rgb(0x6b, 0x6e, 0x85),
            accent_user: Color::Rgb(0x7d, 0xd3, 0xfc),
            accent_agent: Color::Rgb(0xc7, 0x92, 0xea),
            accent_system: Color::Rgb(0xff, 0xb4, 0x54),
            accent_error: Color::Rgb(0xf7, 0x76, 0x8e),
            accent_success: Color::Rgb(0x9e, 0xce, 0x6a),
        }
    }
}
