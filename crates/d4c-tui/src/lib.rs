use anyhow::Result;
use app::App;

pub mod app;
pub mod event;
pub mod input;
pub mod ui;

pub fn run() -> Result<()> {
    let mut app = App::new();
    app.run()
}
