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
    session_usage: Usage,
}

impl AgentRunner {
    pub fn new(brain: BrainWrapper, max_turns: u32, hooks: Vec<Box<dyn ExecutionHook>>) -> Self {
        Self {
            brain,
            max_turns,
            hooks,
            session_usage: Usage::default(),
        }
    }

    /// Ejecuta el bucle del agente.
    /// Lee mensajes de `input_rx` y emite eventos a `event_tx`.
    /// Soporta cancelación en tiempo real enviando `UserInput::Cancel`.
    pub async fn run(
        mut self,
        mut input_rx: mpsc::UnboundedReceiver<UserInput>,
        event_tx: mpsc::UnboundedSender<AgentEvent>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        while let Some(input) = input_rx.recv().await {
            match input {
                UserInput::Message(prompt) => {
                    self.process_prompt(&prompt, &event_tx, &mut input_rx).await;
                }
                UserInput::Cancel => {}
                UserInput::ResetConversation => {
                    let message = match self.brain.clear_conversation().await {
                        Ok(()) => {
                            self.session_usage = Usage::default();
                            "Conversación reiniciada. La memoria del agente está vacía.".to_string()
                        }
                        Err(error) => format!("No se pudo reiniciar la conversación: {error}"),
                    };
                    let _ = event_tx.send(AgentEvent::SystemMessage(message));
                }
                UserInput::Reconfigure(settings) => {
                    if let Err(error) = self.brain.reconfigure(settings) {
                        let _ = event_tx.send(AgentEvent::Error(error.to_string()));
                    }
                }
                UserInput::Exit => break,
            }
        }

        Ok(())
    }

    async fn process_prompt(
        &mut self,
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
                                    add_usage(&mut token_usage, &usage);
                                    add_usage(&mut self.session_usage, &usage);
                                    let _ = event_tx.send(AgentEvent::Usage(usage));
                                    let _ = event_tx.send(AgentEvent::SessionUsage(
                                        self.session_usage,
                                    ));
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
                    usage: self.session_usage,
                });
            }
            Err(e) => {
                let _ = event_tx.send(AgentEvent::Error(format!("Error al iniciar stream: {}", e)));
            }
        }
    }
}

fn add_usage(accumulator: &mut Usage, usage: &Usage) {
    accumulator.input_tokens = accumulator.input_tokens.saturating_add(usage.input_tokens);
    accumulator.output_tokens = accumulator
        .output_tokens
        .saturating_add(usage.output_tokens);
    accumulator.cached_input_tokens = accumulator
        .cached_input_tokens
        .saturating_add(usage.cached_input_tokens);
    accumulator.cache_creation_input_tokens = accumulator
        .cache_creation_input_tokens
        .saturating_add(usage.cache_creation_input_tokens);
    accumulator.tool_use_prompt_tokens = accumulator
        .tool_use_prompt_tokens
        .saturating_add(usage.tool_use_prompt_tokens);
    accumulator.reasoning_tokens = accumulator
        .reasoning_tokens
        .saturating_add(usage.reasoning_tokens);

    let total = if usage.total_tokens > 0 {
        usage.total_tokens
    } else {
        usage.input_tokens.saturating_add(usage.output_tokens)
    };
    accumulator.total_tokens = accumulator.total_tokens.saturating_add(total);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accumulates_all_usage_fields_and_falls_back_to_input_plus_output() {
        let mut accumulated = Usage::default();
        let first = Usage {
            input_tokens: 10,
            output_tokens: 4,
            total_tokens: 0,
            cached_input_tokens: 2,
            cache_creation_input_tokens: 1,
            tool_use_prompt_tokens: 3,
            reasoning_tokens: 5,
        };
        let second = Usage {
            input_tokens: 20,
            output_tokens: 8,
            total_tokens: 35,
            cached_input_tokens: 4,
            cache_creation_input_tokens: 2,
            tool_use_prompt_tokens: 6,
            reasoning_tokens: 7,
        };

        add_usage(&mut accumulated, &first);
        add_usage(&mut accumulated, &second);

        assert_eq!(accumulated.input_tokens, 30);
        assert_eq!(accumulated.output_tokens, 12);
        assert_eq!(accumulated.total_tokens, 49);
        assert_eq!(accumulated.cached_input_tokens, 6);
        assert_eq!(accumulated.cache_creation_input_tokens, 3);
        assert_eq!(accumulated.tool_use_prompt_tokens, 9);
        assert_eq!(accumulated.reasoning_tokens, 12);
    }
}
