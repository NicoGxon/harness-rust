use anyhow::Result;
use brain::{InfoModelo, ModeloLLM};
use console::style;
use dialoguer::{Confirm, Input, Select};
use indicatif::{ProgressBar, ProgressStyle};
use std::env;
use std::time::Duration;

use super::model::TyphonConfig;

/// Resuelve la configuración completa para una sesión de chat.
/// Prioridad: variables .env > prompt interactivo (dialoguer).
pub async fn resolver_config() -> Result<TyphonConfig> {
    let provider = resolver_proveedor()?;
    let api_key = resolver_api_key(provider)?;
    let model = resolver_modelo(provider, &api_key).await?;
    let temperature = resolver_temperatura()?;
    let preamble = resolver_preamble()?;
    let verbose = resolver_verbose()?;

    Ok(TyphonConfig {
        provider,
        model,
        api_key,
        temperature,
        preamble,
        verbose,
    })
}

fn resolver_proveedor() -> Result<ModeloLLM> {
    let items = &["DeepSeek", "Gemini"];
    let selection = Select::new()
        .with_prompt(format!(
            "{} Selecciona el proveedor de LLM",
            style("⚡").bold()
        ))
        .items(items)
        .default(0)
        .interact()?;

    Ok(match selection {
        1 => ModeloLLM::Gemini,
        _ => ModeloLLM::DeepSeek,
    })
}

fn resolver_api_key(provider: ModeloLLM) -> Result<String> {
    let env_key_name = match provider {
        ModeloLLM::Gemini => "GEMINI_API_KEY",
        ModeloLLM::DeepSeek => "DEEP_SEEK_API_KEY",
    };

    // Intentar desde .env primero
    if let Ok(key) = env::var(env_key_name) {
        if !key.is_empty() {
            println!(
                "  {} API key cargada desde {}",
                style("✔").green(),
                style(env_key_name).dim()
            );
            return Ok(key);
        }
    }

    // Si no hay en .env, pedir interactivamente
    let key: String = Input::new()
        .with_prompt(format!(
            "{} Ingresa tu API Key ({})",
            style("🔑").bold(),
            env_key_name
        ))
        .validate_with(|input: &String| {
            if input.trim().is_empty() {
                Err("La API key no puede estar vacía")
            } else {
                Ok(())
            }
        })
        .interact_text()?;

    Ok(key.trim().to_string())
}

async fn resolver_modelo(provider: ModeloLLM, api_key: &str) -> Result<String> {
    let spinner = ProgressBar::new_spinner();
    spinner.set_style(
        ProgressStyle::with_template("  {spinner:.cyan} {msg}")
            .unwrap()
            .tick_chars("⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏"),
    );
    spinner.set_message("Consultando modelos disponibles...");
    spinner.enable_steady_tick(Duration::from_millis(80));

    let modelos = match provider.listar_modelos(api_key).await {
        Ok(m) if !m.is_empty() => {
            spinner.finish_and_clear();
            m
        }
        _ => {
            spinner.finish_with_message(format!(
                "{}",
                style("No se pudieron listar los modelos. Usando respaldo.").yellow()
            ));
            modelos_fallback(provider)
        }
    };

    let items: Vec<String> = modelos
        .iter()
        .map(|m| format!("{} ({})", m.id, style(&m.owned_by).dim()))
        .collect();

    let selection = Select::new()
        .with_prompt(format!("{} Selecciona el modelo", style("🧠").bold()))
        .items(&items)
        .default(0)
        .interact()?;

    Ok(modelos[selection].id.clone())
}

fn resolver_temperatura() -> Result<f64> {
    let temp: f64 = Input::new()
        .with_prompt(format!(
            "{} Temperatura (0.0 - 2.0)",
            style("🌡️").bold()
        ))
        .default(0.7)
        .validate_with(|input: &f64| {
            if *input >= 0.0 && *input <= 2.0 {
                Ok(())
            } else {
                Err("Debe estar entre 0.0 y 2.0")
            }
        })
        .interact_text()?;

    Ok(temp)
}

fn resolver_preamble() -> Result<String> {
    let default = "Eres Typhon, un asistente de IA potente y servicial de pair programming en Rust.";

    let preamble: String = Input::new()
        .with_prompt(format!("{} System prompt", style("📝").bold()))
        .default(default.to_string())
        .interact_text()?;

    Ok(preamble)
}

fn resolver_verbose() -> Result<bool> {
    let verbose = Confirm::new()
        .with_prompt(format!(
            "{} ¿Mostrar métricas de ejecución?",
            style("📊").bold()
        ))
        .default(false)
        .interact()?;

    Ok(verbose)
}

fn modelos_fallback(provider: ModeloLLM) -> Vec<InfoModelo> {
    match provider {
        ModeloLLM::Gemini => vec![
            InfoModelo {
                id: "gemini-1.5-flash".into(),
                owned_by: "google".into(),
            },
            InfoModelo {
                id: "gemini-1.5-pro".into(),
                owned_by: "google".into(),
            },
        ],
        ModeloLLM::DeepSeek => vec![
            InfoModelo {
                id: "deepseek-chat".into(),
                owned_by: "deepseek".into(),
            },
            InfoModelo {
                id: "deepseek-reasoner".into(),
                owned_by: "deepseek".into(),
            },
        ],
    }
}
