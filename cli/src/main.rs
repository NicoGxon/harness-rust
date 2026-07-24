mod config;
mod tui;
mod ui;

use anyhow::Result;
use brain::BrainWrapper;
use rig::memory::InMemoryConversationMemory;

#[tokio::main]
async fn main() -> Result<()> {
    dotenv::dotenv().ok();

    let args: Vec<String> = std::env::args().collect();
    let force_reconfig = args
        .iter()
        .any(|arg| arg == "--reconfig" || arg == "--config" || arg == "-c");

    let config = config::resolver_config(force_reconfig).await?;

    let current_dir = std::env::current_dir()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| ".".to_string());

    ui::imprimir_banner(&format!("{}", config.provider), &config.model, &current_dir);

    let memory = InMemoryConversationMemory::new();
    let brain = BrainWrapper::new(
        config.provider,
        &config.model,
        &config.preamble,
        &config.api_key,
        config.temperature,
        memory,
    );

    let runner = agent_core::AgentRunner::new(brain, 10, Vec::new());
    let provider_str = format!("{:?}", config.provider);

    tui::run_tui(runner, config.model, provider_str)
        .await
        .map_err(|e| anyhow::anyhow!(e))?;

    Ok(())
}
