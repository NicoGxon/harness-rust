use crate::events::{AgentEvent, UserInput};
use crate::hooks::{ExecutionHook, ExecutionMetrics};
use brain::{BrainWrapper, StreamChunk, Usage};
use futures::StreamExt;
use tokio::sync::mpsc;

/// Motor de ejecución del agente, completamente desacoplado de la UI.
/// Se comunica exclusivamente a través de canales mpsc.
pub struct AgentRunner {
    brain: BrainWrapper,
    max_turns: u32,
    hooks: Vec<Box<dyn ExecutionHook>>,
}

impl AgentRunner {
    pub fn new(brain: BrainWrapper, max_turns: u32, hooks: Vec<Box<dyn ExecutionHook>>) -> Self {
        Self {
            brain,
            max_turns,
            hooks,
        }
    }

    /// Ejecuta el bucle del agente.
    /// Lee mensajes de `input_rx` y emite eventos a `event_tx`.
    /// Soporta cancelación en tiempo real enviando `UserInput::Cancel`.
    pub async fn run(
        self,
        mut input_rx: mpsc::UnboundedReceiver<UserInput>,
        event_tx: mpsc::UnboundedSender<AgentEvent>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        while let Some(input) = input_rx.recv().await {
            match input {
                UserInput::Message(prompt) => {
                    self.process_prompt(&prompt, &event_tx, &mut input_rx).await;
                }
                UserInput::Cancel => {}
                UserInput::Exit => break,
            }
        }

        Ok(())
    }

    async fn process_prompt(
        &self,
        prompt: &str,
        event_tx: &mpsc::UnboundedSender<AgentEvent>,
        input_rx: &mut mpsc::UnboundedReceiver<UserInput>,
    ) {
        let _ = event_tx.send(AgentEvent::StreamStart);
        let start_time = std::time::Instant::now();
        match self.brain.prompt(prompt, self.max_turns).await {
            Ok(mut stream) => {
                let mut full_response = String::new();
                let mut token_usage = Usage::default();
                'stream_loop: loop {
                    tokio::select! {
                        cancel_msg = input_rx.recv() => {
                            match cancel_msg {
                                Some(UserInput::Cancel | UserInput::Exit) => {
                                    let _ = event_tx.send(AgentEvent::Cancelled);
                                    return;
                                }
                                Some(_) => {} // otros mensajes de input se ignoran acá
                                None => {
                                    // canal cerrado: no hay más cancelaciones posibles,
                                    // dejamos de pollear esta rama y seguimos consumiendo el stream
                                    std::future::pending::<()>().await;
                                }
                            }
                        }
                        chunk_opt = stream.next() => {
                            match chunk_opt {
                                Some(Ok(StreamChunk::Text(text))) => {
                                    full_response.push_str(&text);
                                    let _ = event_tx.send(AgentEvent::Text(text));
                                }
                                Some(Ok(StreamChunk::Reasoning(reasoning))) => {
                                    let _ = event_tx.send(AgentEvent::Reasoning(reasoning));
                                }
                                Some(Ok(StreamChunk::Usage(usage))) => {
                                    token_usage = usage.clone();
                                    let _ = event_tx.send(AgentEvent::Usage(usage));
                                }
                                Some(Err(e)) => {
                                    let _ = event_tx.send(AgentEvent::Error(e.to_string()));
                                    break 'stream_loop;
                                }
                                None => break 'stream_loop, // Fin del stream
                            }
                        }
                    }
                }
                let duration = start_time.elapsed();
                // Notificar a los hooks
                let metrics = ExecutionMetrics {
                    prompt: prompt.to_string(),
                    duration,
                    response_len: full_response.len(),
                    usage: token_usage.clone(),
                    max_turns: self.max_turns,
                };
                for hook in &self.hooks {
                    hook.on_execution_completed(&metrics);
                }
                let _ = event_tx.send(AgentEvent::StreamEnd {
                    duration,
                    response_len: full_response.len(),
                    usage: token_usage,
                });
            }
            Err(e) => {
                let _ = event_tx.send(AgentEvent::Error(format!("Error al iniciar stream: {}", e)));
            }
        }
    }
}
