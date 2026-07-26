pub enum AppEvent {
    Tick,
    KeyInput(crossterm::event::KeyEvent),
    Resize(u16, u16),
    Message(String),
}
