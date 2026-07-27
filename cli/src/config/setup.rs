use anyhow::{Context, Result, bail};
use brain::{
    BrainWrapper, ModeloLLM, ProviderCredential, ReasoningEffort,
    provedores::chatgpt::ProvedorChatGPT,
};
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

    let credential = resolver_credencial(config_file.provider, &config_dir).await?;

    Ok(TyphonConfig {
        provider: config_file.provider,
        model: config_file.model,
        credential,
        temperature: config_file.temperature,
        reasoning_effort: config_file.reasoning_effort,
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
        let credential = resolver_credencial(provider, config_dir).await?;
        let model = resolver_modelo(provider, &credential).await?;
        let reasoning_effort = resolver_nivel_razonamiento(provider, None)?;

        let config_file = ConfigFile {
            provider,
            model,
            temperature: 0.7,
            reasoning_effort,
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
    let mut working_credential = config.credential.clone();
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
                "Nivel de razonamiento ({})",
                formato_nivel_razonamiento(working.provider, working.reasoning_effort)
            ),
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
                let (provider, model, credential) = resolver_proveedor_y_modelo(
                    working.provider,
                    &working.model,
                    &working_credential,
                    config,
                )
                .await?;
                working.provider = provider;
                working.model = model;
                working_credential = credential;
                if !proveedor_admite_razonamiento(provider) {
                    working.reasoning_effort = None;
                }
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
                working.reasoning_effort =
                    resolver_nivel_razonamiento(working.provider, working.reasoning_effort)?;
            }
            3 => {
                working.verbose = Confirm::new()
                    .with_prompt("¿Activar mensajes detallados?")
                    .default(working.verbose)
                    .interact()?;
            }
            4 => {
                if let Some(edited) = Editor::new().edit(&prompt_content)? {
                    prompt_content = edited;
                }
            }
            5 => {
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
            6 => {
                fs::write(&prompt_path, &prompt_content)
                    .with_context(|| format!("Error al guardar {}", prompt_path.display()))?;
                guardar_config(&config_path, &working)?;
                return Ok(Some(TyphonConfig {
                    provider: working.provider,
                    model: working.model,
                    credential: working_credential,
                    temperature: working.temperature,
                    reasoning_effort: working.reasoning_effort,
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

fn proveedor_admite_razonamiento(provider: ModeloLLM) -> bool {
    matches!(provider, ModeloLLM::OpenAI | ModeloLLM::ChatGPT)
}

fn formato_nivel_razonamiento(
    provider: ModeloLLM,
    reasoning_effort: Option<ReasoningEffort>,
) -> String {
    if !proveedor_admite_razonamiento(provider) {
        return "no aplica".to_string();
    }
    reasoning_effort
        .map(|effort| effort.to_string())
        .unwrap_or_else(|| "automático".to_string())
}

fn resolver_nivel_razonamiento(
    provider: ModeloLLM,
    current: Option<ReasoningEffort>,
) -> Result<Option<ReasoningEffort>> {
    if !proveedor_admite_razonamiento(provider) {
        return Ok(None);
    }

    let mut items = vec!["Automático (predeterminado)".to_string()];
    items.extend(
        ReasoningEffort::ALL
            .into_iter()
            .map(|effort| effort.to_string()),
    );
    let default = current
        .and_then(|effort| ReasoningEffort::ALL.iter().position(|item| *item == effort))
        .map(|index| index + 1)
        .unwrap_or(0);
    let selection = Select::new()
        .with_prompt("Nivel de razonamiento")
        .items(&items)
        .default(default)
        .interact()?;

    Ok((selection > 0).then(|| ReasoningEffort::ALL[selection - 1]))
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
    current_credential: &ProviderCredential,
    config: &TyphonConfig,
) -> Result<(ModeloLLM, String, ProviderCredential)> {
    let config_dir = config
        .config_path
        .parent()
        .context("La configuración no tiene un directorio padre")?;
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
    let credential = if provider == current_provider {
        current_credential.clone()
    } else if provider == config.provider {
        config.credential.clone()
    } else {
        resolver_credencial(provider, config_dir).await?
    };
    let model_for_default = if provider == current_provider {
        current_model
    } else {
        ""
    };
    let model = resolver_modelo_con_actual(provider, &credential, model_for_default).await?;
    Ok((provider, model, credential))
}

async fn resolver_modelo_con_actual(
    provider: ModeloLLM,
    credential: &ProviderCredential,
    current_model: &str,
) -> Result<String> {
    let modelos = match provider.listar_modelos(credential).await {
        Ok(modelos) => modelos,
        Err(error) if provider == ModeloLLM::ChatGPT => {
            eprintln!(
                "  [!] No se pudo listar modelos de ChatGPT ({error}). Puedes introducir el slug manualmente."
            );
            return resolver_modelo_manual(current_model);
        }
        Err(error) => {
            return Err(anyhow::anyhow!(
                "Error al consultar modelos de {provider}: {error}"
            ));
        }
    };
    if modelos.is_empty() {
        if provider == ModeloLLM::ChatGPT {
            return resolver_modelo_manual(current_model);
        }
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
    let providers = BrainWrapper::listar_provedores();
    let items: Vec<String> = providers.iter().map(ToString::to_string).collect();
    let selection = Select::new()
        .with_prompt("Selecciona el proveedor de LLM")
        .items(&items)
        .default(0)
        .interact()?;

    Ok(providers[selection])
}

async fn resolver_credencial(provider: ModeloLLM, config_dir: &Path) -> Result<ProviderCredential> {
    if provider == ModeloLLM::ChatGPT {
        let auth_file = config_dir.join("chatgpt").join("auth.json");
        fs::create_dir_all(
            auth_file
                .parent()
                .context("No se pudo crear el directorio de sesión ChatGPT")?,
        )?;
        proteger_sesion_chatgpt(auth_file.parent().unwrap_or(config_dir));
        println!("  [i] Iniciando sesión de ChatGPT...");
        ProvedorChatGPT::autorizar(&auth_file)
            .await
            .map_err(|error| {
                anyhow::anyhow!("No se pudo autorizar la suscripción de ChatGPT: {error}")
            })?;
        proteger_sesion_chatgpt_file(&auth_file);
        return Ok(ProviderCredential::ChatGptOAuth { auth_file });
    }

    let env_key_name = match provider {
        ModeloLLM::Gemini => "GEMINI_API_KEY",
        ModeloLLM::DeepSeek => "DEEP_SEEK_API_KEY",
        ModeloLLM::OpenAI => "OPENAI_API_KEY",
        ModeloLLM::ChatGPT => unreachable!(),
    };

    // Intentar desde .env primero
    if let Ok(key) = env::var(env_key_name)
        && !key.is_empty()
    {
        return Ok(ProviderCredential::ApiKey(key));
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

    Ok(ProviderCredential::ApiKey(key.trim().to_string()))
}

#[cfg(unix)]
fn proteger_sesion_chatgpt(directory: &Path) {
    use std::os::unix::fs::PermissionsExt;
    let _ = fs::set_permissions(directory, fs::Permissions::from_mode(0o700));
}

#[cfg(unix)]
fn proteger_sesion_chatgpt_file(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    if path.exists() {
        let _ = fs::set_permissions(path, fs::Permissions::from_mode(0o600));
    }
}

async fn resolver_modelo(provider: ModeloLLM, credential: &ProviderCredential) -> Result<String> {
    let spinner = ProgressBar::new_spinner();
    spinner.set_style(
        ProgressStyle::with_template("  {spinner:.cyan} {msg}")
            .unwrap()
            .tick_chars("⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏"),
    );
    spinner.set_message("Consultando modelos disponibles...");
    spinner.enable_steady_tick(Duration::from_millis(80));

    let modelos = match provider.listar_modelos(credential).await {
        Ok(m) if !m.is_empty() => {
            spinner.finish_and_clear();
            m
        }
        Ok(_) if provider == ModeloLLM::ChatGPT => {
            spinner.finish_and_clear();
            eprintln!("  [!] ChatGPT no devolvió modelos. Puedes introducir el slug manualmente.");
            return resolver_modelo_manual("");
        }
        Ok(_) => {
            spinner.finish_and_clear();
            bail!("No se encontraron modelos disponibles para {provider}.");
        }
        Err(_) if provider == ModeloLLM::ChatGPT => {
            spinner.finish_and_clear();
            eprintln!(
                "  [!] No se pudo listar modelos de ChatGPT. Puedes introducir el slug manualmente."
            );
            return resolver_modelo_manual("");
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

fn resolver_modelo_manual(actual: &str) -> Result<String> {
    let modelo: String = Input::new()
        .with_prompt("Identificador del modelo")
        .with_initial_text(actual)
        .validate_with(|value: &String| {
            if value.trim().is_empty() {
                Err("El identificador del modelo no puede estar vacío")
            } else {
                Ok(())
            }
        })
        .interact_text()?;
    Ok(modelo.trim().to_string())
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
            reasoning_effort: None,
            verbose: false,
            prompt_file: "prompt.md".to_string(),
        };
        let serialized = toml::to_string(&original).unwrap();
        let restored: ConfigFile = toml::from_str(&serialized).unwrap();

        assert_eq!(restored, original);
        assert!(!serialized.contains("api_key"));
    }

    #[test]
    fn config_file_roundtrips_reasoning_effort() {
        let original = ConfigFile {
            provider: ModeloLLM::OpenAI,
            model: "gpt-5".to_string(),
            temperature: 0.7,
            reasoning_effort: Some(ReasoningEffort::High),
            verbose: true,
            prompt_file: "system_prompt.md".to_string(),
        };
        let serialized = toml::to_string(&original).unwrap();
        assert!(serialized.contains("reasoning_effort = \"high\""));
        let restored: ConfigFile = toml::from_str(&serialized).unwrap();
        assert_eq!(restored.reasoning_effort, Some(ReasoningEffort::High));
    }
}
