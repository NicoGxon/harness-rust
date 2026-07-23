use brain::Usage;
use std::time::Duration;

/// Eventos que el agente emite hacia la capa de UI.
#[derive(Debug, Clone)]
pub enum AgentEvent {
    /// El agente inicia el procesamiento de un prompt.
    StreamStart,
    /// Fragmento de texto generado por el modelo.
    Text(String),
    /// Fragmento de razonamiento generado por el modelo.
    Reasoning(String),
    /// Inicio de ejecución de una herramienta.
    ToolStart { name: String, input: String },
    /// Resultado de la ejecución de una herramienta.
    ToolResult { name: String, output: String },
    /// Información de uso de tokens reportada por el modelo.
    Usage(Usage),
    /// Error ocurrido durante el procesamiento.
    Error(String),
    /// El procesamiento fue cancelado por el usuario.
    Cancelled,
    /// El agente finalizó el procesamiento de un prompt.
    StreamEnd {
        duration: Duration,
        response_len: usize,
        usage: Usage,
    },
}

/// Mensajes que la capa de UI envía al agente.
#[derive(Debug, Clone)]
pub enum UserInput {
    /// Un mensaje/prompt del usuario.
    Message(String),
    /// Solicitud de cancelación del procesamiento actual.
    Cancel,
    /// El usuario solicita salir.
    Exit,
}
