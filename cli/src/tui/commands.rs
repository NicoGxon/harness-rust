use brain::{ReasoningEffort, Usage};
use std::path::Path;

/// Información no sensible de la sesión que se muestra en `/status` y `/config`.
#[derive(Debug, Clone)]
pub struct SessionInfo {
    pub provider: String,
    pub model: String,
    pub temperature: f64,
    pub reasoning_effort: Option<ReasoningEffort>,
    pub max_turns: u32,
    pub current_dir: String,
    pub config_path: String,
    pub prompt_path: String,
    pub verbose: bool,
}

impl SessionInfo {
    pub fn from_paths(
        provider: String,
        model: String,
        temperature: f64,
        reasoning_effort: Option<ReasoningEffort>,
        max_turns: u32,
        current_dir: String,
        config_path: &Path,
        prompt_path: &Path,
        verbose: bool,
    ) -> Self {
        Self {
            provider,
            model,
            temperature,
            reasoning_effort,
            max_turns,
            current_dir,
            config_path: config_path.display().to_string(),
            prompt_path: prompt_path.display().to_string(),
            verbose,
        }
    }

    pub fn update(
        &mut self,
        provider: String,
        model: String,
        temperature: f64,
        reasoning_effort: Option<ReasoningEffort>,
        verbose: bool,
        config_path: &Path,
        prompt_path: &Path,
    ) {
        self.provider = provider;
        self.model = model;
        self.temperature = temperature;
        self.reasoning_effort = reasoning_effort;
        self.verbose = verbose;
        self.config_path = config_path.display().to_string();
        self.prompt_path = prompt_path.display().to_string();
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Message(String),
    Help,
    Status,
    Tools,
    Config,
    ConfigShow,
    Clear,
    New,
    Exit,
    Unknown(String),
}

/// Interpreta una entrada de la TUI antes de decidir si se envía al LLM.
pub fn parse(input: &str) -> Command {
    let trimmed = input.trim();

    if !trimmed.starts_with('/') {
        return Command::Message(trimmed.to_string());
    }

    // Permite enviar literalmente un prompt que comienza por `/`.
    if trimmed.starts_with("//") {
        return Command::Message(trimmed[1..].to_string());
    }

    let parts: Vec<&str> = trimmed.split_whitespace().collect();
    let name = parts
        .first()
        .copied()
        .unwrap_or_default()
        .to_ascii_lowercase();
    let arguments = &parts[1..];

    match name.as_str() {
        "/help" | "/?" if arguments.is_empty() => Command::Help,
        "/status" if arguments.is_empty() => Command::Status,
        "/tools" if arguments.is_empty() => Command::Tools,
        "/config" if arguments.is_empty() => Command::Config,
        "/config" if arguments.len() == 1 && arguments[0].eq_ignore_ascii_case("show") => {
            Command::ConfigShow
        }
        "/clear" if arguments.is_empty() => Command::Clear,
        "/new" if arguments.is_empty() => Command::New,
        "/exit" | "/quit" if arguments.is_empty() => Command::Exit,
        _ => Command::Unknown(trimmed.to_string()),
    }
}

pub fn help_text() -> &'static str {
    "Comandos de Typhon:\n\
  /help, /?       Muestra esta ayuda\n\
  /status         Muestra el estado de la sesión\n\
  /tools          Lista las herramientas disponibles\n\
  /config         Abre el editor interactivo de configuración\n\
  /config show    Muestra la configuración actual\n\
  /clear          Limpia la pantalla\n\
  /new            Inicia una conversación nueva\n\
  /exit, /quit    Sale de Typhon\n\
  //texto         Envía literalmente un prompt que empieza por /"
}

pub fn status_text(info: &SessionInfo, session_usage: &Usage) -> String {
    format!(
        "Estado de Typhon:\n  Proveedor: {}\n  Modelo: {}\n  Temperatura: {:.2}\n  Razonamiento: {}\n  Máximo de turnos: {}\n  Directorio: {}\n  Consumo de la conversación: {}",
        info.provider,
        info.model,
        info.temperature,
        format_reasoning_effort(info.reasoning_effort),
        info.max_turns,
        info.current_dir,
        session_usage_text(session_usage)
    )
}

pub fn session_usage_text(usage: &Usage) -> String {
    if !usage.has_values() {
        return "Sin tokens reportados todavía".to_string();
    }

    format!(
        "Tokens: {} entrada / {} salida / {} total{}",
        usage.input_tokens,
        usage.output_tokens,
        usage.total_tokens,
        if usage.reasoning_tokens > 0 {
            format!(" / {} razonamiento", usage.reasoning_tokens)
        } else {
            String::new()
        }
    )
}

pub fn config_text(info: &SessionInfo) -> String {
    format!(
        "Configuración activa:\n  Archivo de configuración: {}\n  System prompt: {}\n  Proveedor: {}\n  Modelo: {}\n  Temperatura: {:.2}\n  Razonamiento: {}\n  Verbose: {}",
        info.config_path,
        info.prompt_path,
        info.provider,
        info.model,
        info.temperature,
        format_reasoning_effort(info.reasoning_effort),
        if info.verbose {
            "activado"
        } else {
            "desactivado"
        }
    )
}

fn format_reasoning_effort(effort: Option<ReasoningEffort>) -> String {
    effort
        .map(|value| value.to_string())
        .unwrap_or_else(|| "automático".to_string())
}

pub fn tools_text() -> &'static str {
    "Herramientas disponibles:\n  crear_archivo     Crea un archivo de texto en el disco\n  leer_archivo      Lee el contenido de un archivo\n  listar_directorio Lista archivos y subdirectorios"
}

pub fn unknown_text(command: &str) -> String {
    format!(
        "Comando desconocido: '{}'. Escribe /help para ver los comandos disponibles.",
        command
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_local_commands_without_case_sensitivity() {
        assert_eq!(parse(" /HELP "), Command::Help);
        assert_eq!(parse("/?"), Command::Help);
        assert_eq!(parse("/quit"), Command::Exit);
    }

    #[test]
    fn preserves_regular_messages() {
        assert_eq!(
            parse("explica este código"),
            Command::Message("explica este código".into())
        );
        assert_eq!(parse("//literal"), Command::Message("/literal".into()));
    }

    #[test]
    fn rejects_arguments_for_v1_commands() {
        assert_eq!(
            parse("/status ahora"),
            Command::Unknown("/status ahora".into())
        );
        assert_eq!(
            parse("/config otra-cosa"),
            Command::Unknown("/config otra-cosa".into())
        );
    }

    #[test]
    fn parses_interactive_and_read_only_config_commands() {
        assert_eq!(parse("/config"), Command::Config);
        assert_eq!(parse("/CONFIG SHOW"), Command::ConfigShow);
        assert_eq!(
            parse("/config show ahora"),
            Command::Unknown("/config show ahora".into())
        );
    }

    #[test]
    fn reports_unknown_commands() {
        assert_eq!(parse("/wat"), Command::Unknown("/wat".into()));
    }

    #[test]
    fn formats_session_usage_with_the_requested_breakdown() {
        assert_eq!(
            session_usage_text(&Usage {
                input_tokens: 120,
                output_tokens: 30,
                total_tokens: 150,
                ..Usage::default()
            }),
            "Tokens: 120 entrada / 30 salida / 150 total"
        );
        assert_eq!(
            session_usage_text(&Usage::default()),
            "Sin tokens reportados todavía"
        );
        assert_eq!(
            session_usage_text(&Usage {
                input_tokens: 10,
                output_tokens: 4,
                total_tokens: 14,
                reasoning_tokens: 7,
                ..Usage::default()
            }),
            "Tokens: 10 entrada / 4 salida / 14 total / 7 razonamiento"
        );
    }
}
