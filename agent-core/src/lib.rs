pub mod hooks;
pub use hooks::{ConsoleTimeHook, ExecutionHook, ExecutionMetrics};

use brain::{BrainWrapper, StreamChunk, Usage};
use futures::StreamExt;
use std::io::{self, Write};

pub async fn iniciar_loop(
    brain: BrainWrapper,
    max_turns: u32,
    hooks: Vec<Box<dyn ExecutionHook>>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mut input = String::new();

    loop {
        print!("Typhon > ");
        io::stdout().flush()?;
        input.clear();
        io::stdin().read_line(&mut input)?;

        let prompt = input.trim();
        if prompt.is_empty() {
            continue;
        }

        if prompt == "exit" || prompt == "quit" {
            println!("Saliendo de Typhon. ¡Hasta luego!");
            break;
        }

        let start_time = std::time::Instant::now();
        match brain.prompt(prompt, max_turns).await {
            Ok(mut stream) => {
                let mut full_response = String::new();
                let mut token_usage = Usage::default();

                println!(); // Linebreak before response starts

                while let Some(chunk_res) = stream.next().await {
                    match chunk_res {
                        Ok(StreamChunk::Text(text)) => {
                            print!("{}", text);
                            io::stdout().flush()?;
                            full_response.push_str(&text);
                        }
                        Ok(StreamChunk::Reasoning(reasoning)) => {
                            print!("{}", reasoning);
                            io::stdout().flush()?;
                        }
                        Ok(StreamChunk::Usage(usage)) => {
                            token_usage = usage;
                        }
                        Err(e) => {
                            eprintln!("\nError en el stream: {}\n", e);
                        }
                    }
                }

                let duration = start_time.elapsed();
                println!();

                let metrics = ExecutionMetrics {
                    prompt: prompt.to_string(),
                    duration,
                    response_len: full_response.len(),
                    usage: token_usage,
                    max_turns,
                };

                for hook in &hooks {
                    hook.on_execution_completed(&metrics);
                }

                println!();
            }
            Err(e) => {
                eprintln!("\nError al iniciar stream: {}\n", e);
            }
        }
    }

    Ok(())
}

