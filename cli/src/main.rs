use brain::{BrainWrapper, InfoModelo, ModeloLLM};
use rig::memory::InMemoryConversationMemory;
use std::env;
use std::io::{self, Write};

fn read_input(prompt: &str, default: Option<&str>) -> String {
    print!("{}", prompt);
    io::stdout().flush().ok();
    let mut input = String::new();
    io::stdin().read_line(&mut input).ok();
    let trimmed = input.trim().to_string();
    if trimmed.is_empty() && default.is_some() {
        default.unwrap().to_string()
    } else {
        trimmed
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Attempt to load .env file if it exists (keeps it helper-friendly)
    dotenv::dotenv().ok();

    println!("==================================================");
    println!("          CONFIGURACIÓN DE TYPHON IA              ");
    println!("==================================================");

    // 1. Select LLM Provider
    println!("Selecciona el proveedor de LLM:");
    println!("  1) Gemini");
    println!("  2) DeepSeek");
    let provider_opt = read_input("Selección [Por defecto: 2]: ", Some("2"));
    let provider = match provider_opt.as_str() {
        "1" => ModeloLLM::Gemini,
        _ => ModeloLLM::DeepSeek,
    };

    // 2. Fetch API Key (detect from env/dotenv or ask user)
    let env_key_name = match provider {
        ModeloLLM::Gemini => "GEMINI_API_KEY",
        ModeloLLM::DeepSeek => "DEEP_SEEK_API_KEY",
    };
    let env_key = env::var(env_key_name).ok();

    let api_key = match env_key {
        Some(key) if !key.is_empty() => key,
        _ => {
            let mut manual_key = String::new();
            while manual_key.is_empty() {
                manual_key = read_input("Ingresa tu API Key: ", None);
            }
            manual_key
        }
    };

    // 3. Fetch Models dynamically and choose
    println!("\nConsultando modelos disponibles en la API...");
    let modelos = match provider.listar_modelos(&api_key).await {
        Ok(m) if !m.is_empty() => m,
        _ => {
            println!("No se pudieron listar los modelos automáticamente.");
            println!("Usando modelos de respaldo preconfigurados...");
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
    };

    println!("\nModelos disponibles:");
    for (i, m) in modelos.iter().enumerate() {
        println!("  {}) {} (por {})", i + 1, m.id, m.owned_by);
    }
    let model_opt = read_input(
        &format!(
            "Selecciona el modelo [1-{}][Por defecto: 1]: ",
            modelos.len()
        ),
        Some("1"),
    );
    let model_idx: usize = model_opt.parse::<usize>().unwrap_or(1) - 1;
    let selected_model = modelos.get(model_idx).unwrap_or(&modelos[0]);

    // 4. Configure Temperature
    let temp_opt = read_input(
        "\nTemperatura (0.0 a 2.0) [Por defecto: 0.7]: ",
        Some("0.7"),
    );
    let temperature = temp_opt.parse::<f64>().unwrap_or(0.7);

    // 5. Configure Preamble/System Prompt
    let default_preamble =
        "Eres Typhon, un asistente de IA potente y servicial de pair programming en Rust.";
    let preamble = read_input(
        &format!("\nPreamble [Por defecto: \"{}\"]:\n> ", default_preamble),
        Some(default_preamble),
    );

    // 6. Configure Extended Details
    let details_opt = read_input("\n¿Deseas ver detalles extendidos de ejecución (tiempo, tamaño)? [s/N]: ", Some("n"));
    let extend_details = details_opt.to_lowercase() == "s" || details_opt.to_lowercase() == "si";

    println!("\n==================================================");
    println!("             INICIANDO TYPHON IA                  ");
    println!("==================================================");
    println!("Proveedor:          {:?}", provider);
    println!("Modelo:             {}", selected_model.id);
    println!("Temperatura:        {}", temperature);
    println!("Detalles Extendidos: {}", if extend_details { "Activado" } else { "Desactivado" });
    println!("==================================================");

    // Create the conversation memory
    let memory = InMemoryConversationMemory::new();

    // Construct the BrainWrapper
    let brain = BrainWrapper::new(
        provider,
        &selected_model.id,
        &preamble,
        &api_key,
        temperature,
        memory,
    );

    println!("Typhon listo para chatear. Escribe tu mensaje y presiona Enter.");
    println!("Escribe 'exit' o 'quit' para salir.");
    println!("--------------------------------------------------");

    // Construct hooks based on CLI configuration
    let mut hooks: Vec<Box<dyn agent_core::ExecutionHook>> = Vec::new();
    if extend_details {
        hooks.push(Box::new(agent_core::ConsoleTimeHook));
    }

    // Run the main interactive chat loop managed in agent-core with max_turns = 10
    agent_core::iniciar_loop(brain, 10, hooks).await?;

    Ok(())
}
