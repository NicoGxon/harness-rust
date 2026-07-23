mod config;
mod tui;
mod ui;

use anyhow::Result;
use brain::BrainWrapper;
use rig::memory::InMemoryConversationMemory;

#[tokio::main]
async fn main() -> Result<()> {
    dotenv::dotenv().ok();

    ui::imprimir_banner();

    let config = config::resolver_config().await?;

    let memory = InMemoryConversationMemory::new();
    let brain = BrainWrapper::new(
        config.provider,
        &config.model,
        &config.preamble,
        &config.api_key,
        config.temperature,
        memory,
    );

    let mut hooks: Vec<Box<dyn agent_core::ExecutionHook>> = Vec::new();
    if config.verbose {
        hooks.push(Box::new(agent_core::ConsoleTimeHook));
    }

    let runner = agent_core::AgentRunner::new(brain, 10, hooks);
    let provider_str = format!("{:?}", config.provider);

    tui::run_tui(runner, config.model, provider_str)
        .await
        .map_err(|e| anyhow::anyhow!(e))?;

    Ok(())
}
