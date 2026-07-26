use anyhow::Result;
use app::App;

pub mod app;
pub mod event;
pub mod input;
pub mod message;
pub mod message_list;
pub mod sidebar;
pub mod status_bar;
pub mod theme;

pub fn run() -> Result<()> {
    let mut app = App::new();
    app.run()
}
