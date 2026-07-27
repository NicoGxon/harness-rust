use brain::{ModeloLLM, ProviderCredential, ReasoningEffort};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Estructura de configuración persistible en `config.toml`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ConfigFile {
    pub provider: ModeloLLM,
    pub model: String,
    pub temperature: f64,
    #[serde(default)]
    pub reasoning_effort: Option<ReasoningEffort>,
    #[serde(default)]
    pub verbose: bool,
    #[serde(default = "default_prompt_file")]
    pub prompt_file: String,
}

fn default_prompt_file() -> String {
    "system_prompt.md".to_string()
}

/// Configuración resuelta y validada para una sesión de Typhon.
#[derive(Debug, Clone)]
pub struct TyphonConfig {
    pub provider: ModeloLLM,
    pub model: String,
    pub credential: ProviderCredential,
    pub temperature: f64,
    pub reasoning_effort: Option<ReasoningEffort>,
    pub preamble: String,
    pub config_path: PathBuf,
    pub prompt_path: PathBuf,
    #[allow(dead_code)]
    pub verbose: bool,
}
