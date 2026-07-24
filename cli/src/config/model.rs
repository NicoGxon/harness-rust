use brain::ModeloLLM;
use serde::{Deserialize, Serialize};

/// Estructura de configuración persistible en `config.toml`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigFile {
    pub provider: ModeloLLM,
    pub model: String,
    pub temperature: f64,
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
    pub api_key: String,
    pub temperature: f64,
    pub preamble: String,
    #[allow(dead_code)]
    pub verbose: bool,
}
