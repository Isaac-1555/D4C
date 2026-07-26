use anyhow::Result;
use tracing_subscriber::EnvFilter;

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive("d4c=debug".parse()?))
        .init();

    tracing::info!("d4c starting");

    d4c_tui::run()?;

    Ok(())
}
