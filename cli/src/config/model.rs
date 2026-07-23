use brain::ModeloLLM;

/// Configuración resuelta y validada para una sesión de Typhon.
pub struct TyphonConfig {
    pub provider: ModeloLLM,
    pub model: String,
    pub api_key: String,
    pub temperature: f64,
    pub preamble: String,
    pub verbose: bool,
}
