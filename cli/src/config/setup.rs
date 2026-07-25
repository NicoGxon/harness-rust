use anyhow::{Context, Result, bail};
use brain::ModeloLLM;
use console::style;
use dialoguer::{Input, Select};
use indicatif::{ProgressBar, ProgressStyle};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use super::model::{ConfigFile, TyphonConfig};

const DEFAULT_SYSTEM_PROMPT: &str = r#"# System Prompt de Typhon

Eres Typhon, un asistente de IA experto en programación y desarrollo de software multilenguaje.
Tu objetivo es ayudar al usuario a escribir, refactorizar, depurar y diseñar código de alta calidad en cualquier lenguaje de programación o stack tecnológico.

## Directrices Principales
1. **Tono y Estilo**:
   - Sé directo, conciso y ve al grano.
   - Prioriza mostrar código funcional antes de dar explicaciones extensas.
   - Mantén las explicaciones teóricas breves y puntuales.

2. **Idioma**:
   - Responde siempre en español.
   - Los comentarios dentro del código generado también deben estar en español.

3. **Calidad de Código**:
   - Escribe código limpio, moderno, idiomático y seguro.
   - Aplica buenas prácticas de desarrollo, manejo adecuado de errores y estructura clara.
   - Si no estás seguro del lenguaje o framework específico, consulta o solicita aclaraciones necesarias.
"#;

/// Obtiene el directorio de configuración ~/.config/typhon
pub fn obtener_directorio_config() -> Result<PathBuf> {
    let config_dir = dirs::config_dir()
        .context("No se pudo determinar el directorio de configuración del usuario")?
        .join("typhon");
    Ok(config_dir)
}

/// Resuelve la configuración completa para una sesión de chat.
pub async fn resolver_config(force_reconfig: bool) -> Result<TyphonConfig> {
    let config_dir = obtener_directorio_config()?;
    fs::create_dir_all(&config_dir)
        .with_context(|| format!("Error al crear el directorio {}", config_dir.display()))?;

    let config_path = config_dir.join("config.toml");
    let (config_file, prompt_path) =
        cargar_o_crear_config(&config_dir, &config_path, force_reconfig).await?;
    let preamble = fs::read_to_string(&prompt_path).with_context(|| {
        format!(
            "Error al leer el system prompt desde {}",
            prompt_path.display()
        )
    })?;

    let api_key = resolver_api_key(config_file.provider)?;

    Ok(TyphonConfig {
        provider: config_file.provider,
        model: config_file.model,
        api_key,
        temperature: config_file.temperature,
        preamble,
        config_path,
        prompt_path,
        verbose: config_file.verbose,
    })
}

async fn cargar_o_crear_config(
    config_dir: &Path,
    config_path: &Path,
    force_reconfig: bool,
) -> Result<(ConfigFile, PathBuf)> {
    if force_reconfig || !config_path.exists() {
        println!("  [i] Iniciando configuración de proveedor y modelo..."); // Proceso directo de creación de configuración
        let provider = resolver_proveedor()?;
        let api_key_temp = resolver_api_key(provider)?;
        let model = resolver_modelo(provider, &api_key_temp).await?;

        let config_file = ConfigFile {
            provider,
            model,
            temperature: 0.7,
            verbose: true,
            prompt_file: "system_prompt.md".to_string(),
        };

        let toml_str = toml::to_string_pretty(&config_file)
            .context("Error al serializar la configuración inicial")?;
        fs::write(config_path, toml_str)
            .with_context(|| format!("Error al crear {}", config_path.display()))?;

        let prompt_path = config_dir.join(&config_file.prompt_file);
        if !prompt_path.exists() {
            fs::write(&prompt_path, DEFAULT_SYSTEM_PROMPT)
                .with_context(|| format!("Error al crear {}", prompt_path.display()))?;
        }

        println!(
            "  [✓] Configuración inicial guardada en {}",
            style(config_dir.display()).bold()
        );

        return Ok((config_file, prompt_path));
    }

    let content = fs::read_to_string(config_path)
        .with_context(|| format!("Error al leer {}", config_path.display()))?;
    let config_file: ConfigFile = toml::from_str(&content)
        .with_context(|| format!("Error al deserializar {}", config_path.display()))?;

    let prompt_path = if Path::new(&config_file.prompt_file).is_absolute() {
        PathBuf::from(&config_file.prompt_file)
    } else {
        config_dir.join(&config_file.prompt_file)
    };

    if !prompt_path.exists() {
        fs::write(&prompt_path, DEFAULT_SYSTEM_PROMPT)
            .with_context(|| format!("Error al crear {}", prompt_path.display()))?;
    }

    Ok((config_file, prompt_path))
}

fn resolver_proveedor() -> Result<ModeloLLM> {
    let items = &["DeepSeek", "Gemini"];
    let selection = Select::new()
        .with_prompt("Selecciona el proveedor de LLM")
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
    if let Ok(key) = env::var(env_key_name)
        && !key.is_empty()
    {
        return Ok(key);
    }

    // Si no hay en .env, pedir interactivamente
    let key: String = Input::new()
        .with_prompt(format!("Ingresa tu API Key ({})", env_key_name))
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
        Ok(_) => {
            spinner.finish_and_clear();
            bail!("No se encontraron modelos disponibles para {provider}.");
        }
        Err(e) => {
            spinner.finish_and_clear();
            bail!("Error al consultar los modelos del proveedor {provider}: {e}");
        }
    };

    let items: Vec<String> = modelos
        .iter()
        .map(|m| format!("{} ({})", m.id, style(&m.owned_by).dim()))
        .collect();

    let selection = Select::new()
        .with_prompt("Selecciona el modelo")
        .items(&items)
        .default(0)
        .interact()?;

    Ok(modelos[selection].id.clone())
}
