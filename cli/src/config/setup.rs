use anyhow::{Context, Result, bail};
use brain::{BrainWrapper, ModeloLLM};
use console::style;
use dialoguer::{Confirm, Editor, Input, Select};
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

/// Abre el editor interactivo de configuración y retorna una nueva sesión si
/// el usuario guardó cambios.
pub async fn editar_configuracion(config: &TyphonConfig) -> Result<Option<TyphonConfig>> {
    let config_path = config.config_path.clone();
    let config_dir = config_path
        .parent()
        .context("El archivo de configuración no tiene un directorio padre")?
        .to_path_buf();
    let content = fs::read_to_string(&config_path)
        .with_context(|| format!("Error al leer {}", config_path.display()))?;
    let mut working: ConfigFile = toml::from_str(&content)
        .with_context(|| format!("Error al deserializar {}", config_path.display()))?;
    let mut working_api_key = config.api_key.clone();
    let mut prompt_path = resolver_prompt_path(&config_dir, &working.prompt_file);
    let mut prompt_content = fs::read_to_string(&prompt_path).with_context(|| {
        format!(
            "Error al leer el system prompt desde {}",
            prompt_path.display()
        )
    })?;

    loop {
        let items = [
            format!(
                "Proveedor y modelo ({} / {})",
                working.provider, working.model
            ),
            format!("Temperatura ({:.2})", working.temperature),
            format!(
                "Verbose ({})",
                if working.verbose {
                    "activado"
                } else {
                    "desactivado"
                }
            ),
            "Editar system prompt".to_string(),
            format!("Archivo del system prompt ({})", working.prompt_file),
            "Guardar y aplicar".to_string(),
            "Cancelar".to_string(),
        ];
        let selection = Select::new()
            .with_prompt("Configuración de Typhon")
            .items(&items)
            .default(0)
            .interact()?;

        match selection {
            0 => {
                let (provider, model, api_key) = resolver_proveedor_y_modelo(
                    working.provider,
                    &working.model,
                    &working_api_key,
                    config,
                )
                .await?;
                working.provider = provider;
                working.model = model;
                working_api_key = api_key;
            }
            1 => {
                let value: String = Input::new()
                    .with_prompt("Temperatura (0.0 - 2.0)")
                    .with_initial_text(format!("{:.2}", working.temperature))
                    .validate_with(|value: &String| match value.parse::<f64>() {
                        Ok(number) if (0.0..=2.0).contains(&number) => Ok(()),
                        _ => Err("Introduce un número entre 0.0 y 2.0"),
                    })
                    .interact_text()?;
                working.temperature = value
                    .parse()
                    .context("La temperatura introducida no es válida")?;
            }
            2 => {
                working.verbose = Confirm::new()
                    .with_prompt("¿Activar mensajes detallados?")
                    .default(working.verbose)
                    .interact()?;
            }
            3 => {
                if let Some(edited) = Editor::new().edit(&prompt_content)? {
                    prompt_content = edited;
                }
            }
            4 => {
                let new_file: String = Input::new()
                    .with_prompt("Ruta del system prompt")
                    .with_initial_text(&working.prompt_file)
                    .validate_with(|value: &String| {
                        if value.trim().is_empty() {
                            Err("La ruta no puede estar vacía")
                        } else {
                            Ok(())
                        }
                    })
                    .interact_text()?;
                let new_path = resolver_prompt_path(&config_dir, new_file.trim());
                if new_path != prompt_path {
                    if new_path.exists() {
                        prompt_content = fs::read_to_string(&new_path).with_context(|| {
                            format!(
                                "Error al leer el system prompt desde {}",
                                new_path.display()
                            )
                        })?;
                    }
                    working.prompt_file = new_file.trim().to_string();
                    prompt_path = new_path;
                }
            }
            5 => {
                fs::write(&prompt_path, &prompt_content)
                    .with_context(|| format!("Error al guardar {}", prompt_path.display()))?;
                guardar_config(&config_path, &working)?;
                return Ok(Some(TyphonConfig {
                    provider: working.provider,
                    model: working.model,
                    api_key: working_api_key,
                    temperature: working.temperature,
                    preamble: prompt_content,
                    config_path,
                    prompt_path,
                    verbose: working.verbose,
                }));
            }
            _ => return Ok(None),
        }
    }
}

fn resolver_prompt_path(config_dir: &Path, prompt_file: &str) -> PathBuf {
    if Path::new(prompt_file).is_absolute() {
        PathBuf::from(prompt_file)
    } else {
        config_dir.join(prompt_file)
    }
}

fn guardar_config(config_path: &Path, config_file: &ConfigFile) -> Result<()> {
    let toml_str =
        toml::to_string_pretty(config_file).context("Error al serializar la configuración")?;
    fs::write(config_path, toml_str)
        .with_context(|| format!("Error al guardar {}", config_path.display()))?;
    Ok(())
}

async fn resolver_proveedor_y_modelo(
    current_provider: ModeloLLM,
    current_model: &str,
    current_api_key: &str,
    config: &TyphonConfig,
) -> Result<(ModeloLLM, String, String)> {
    let providers: Vec<String> = BrainWrapper::listar_provedores()
        .iter()
        .map(ToString::to_string)
        .collect();
    let provider_selection = Select::new()
        .with_prompt("Selecciona el proveedor")
        .items(&providers)
        .default(
            BrainWrapper::listar_provedores()
                .iter()
                .position(|provider| *provider == current_provider)
                .unwrap_or(0),
        )
        .interact()?;
    let provider = BrainWrapper::listar_provedores()[provider_selection];
    let api_key = if provider == current_provider {
        current_api_key.to_string()
    } else if provider == config.provider {
        config.api_key.clone()
    } else {
        resolver_api_key(provider)?
    };
    let model = resolver_modelo_con_actual(provider, &api_key, current_model).await?;
    Ok((provider, model, api_key))
}

async fn resolver_modelo_con_actual(
    provider: ModeloLLM,
    api_key: &str,
    current_model: &str,
) -> Result<String> {
    let modelos = provider
        .listar_modelos(api_key)
        .await
        .with_context(|| format!("Error al consultar modelos de {provider}"))?;
    if modelos.is_empty() {
        bail!("No se encontraron modelos disponibles para {provider}.");
    }
    let items: Vec<String> = modelos
        .iter()
        .map(|model| format!("{} ({})", model.id, style(&model.owned_by).dim()))
        .collect();
    let default = modelos
        .iter()
        .position(|model| model.id == current_model)
        .unwrap_or(0);
    let selection = Select::new()
        .with_prompt("Selecciona el modelo")
        .items(&items)
        .default(default)
        .interact()?;
    Ok(modelos[selection].id.clone())
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_relative_and_absolute_prompt_paths() {
        let base = Path::new("/tmp/typhon");
        assert_eq!(
            resolver_prompt_path(base, "system_prompt.md"),
            PathBuf::from("/tmp/typhon/system_prompt.md")
        );
        assert_eq!(
            resolver_prompt_path(base, "/etc/typhon-prompt.md"),
            PathBuf::from("/etc/typhon-prompt.md")
        );
    }

    #[test]
    fn config_file_roundtrips_without_api_key() {
        let original = ConfigFile {
            provider: ModeloLLM::Gemini,
            model: "gemini-test".to_string(),
            temperature: 0.4,
            verbose: false,
            prompt_file: "prompt.md".to_string(),
        };
        let serialized = toml::to_string(&original).unwrap();
        let restored: ConfigFile = toml::from_str(&serialized).unwrap();

        assert_eq!(restored, original);
        assert!(!serialized.contains("api_key"));
    }
}
