use crate::provedores::deepseek::ProvedorDeepSeek;
use crate::provedores::gemini::ProvedorGemini;
use futures::StreamExt;
use futures::stream::BoxStream;
use rig::agent::MultiTurnStreamItem;
use rig::memory::InMemoryConversationMemory;
use rig::streaming::{StreamedAssistantContent, StreamingPrompt};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModeloLLM {
    Gemini,
    DeepSeek,
}

pub enum ActiveAgent {
    Gemini(ProvedorGemini),
    DeepSeek(ProvedorDeepSeek),
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct InfoModelo {
    pub id: String,
    pub owned_by: String,
}

#[derive(Debug, Clone)]
pub enum StreamChunk {
    Text(String),
    Reasoning(String),
    Usage(rig::completion::Usage),
}

pub struct BrainWrapper {
    agent: ActiveAgent,
}

impl BrainWrapper {
    pub fn new(
        provedor: ModeloLLM,
        model: &str,
        preamble: &str,
        api_key: &str,
        temperature: f64,
        memory: InMemoryConversationMemory,
    ) -> Self {
        match provedor {
            ModeloLLM::Gemini => {
                let gemini_prov =
                    ProvedorGemini::new(preamble, memory, model, api_key, temperature);
                Self {
                    agent: ActiveAgent::Gemini(gemini_prov),
                }
            }
            ModeloLLM::DeepSeek => {
                let deepseek_prov =
                    ProvedorDeepSeek::new(preamble, memory, model, api_key, temperature);
                Self {
                    agent: ActiveAgent::DeepSeek(deepseek_prov),
                }
            }
        }
    }

    pub async fn prompt(
        &self,
        prompt: &str,
        max_turns: u32,
    ) -> Result<
        BoxStream<'_, Result<StreamChunk, Box<dyn std::error::Error + Send + Sync>>>,
        Box<dyn std::error::Error + Send + Sync>,
    > {
        match &self.agent {
            ActiveAgent::Gemini(gemini_prov) => {
                let stream = gemini_prov
                    .agent
                    .stream_prompt(prompt)
                    .max_turns(max_turns as usize)
                    .await;

                let mapped = stream.map(|item_res| match item_res {
                    Ok(MultiTurnStreamItem::StreamAssistantItem(content)) => match content {
                        StreamedAssistantContent::Text(text) => Ok(StreamChunk::Text(text.text)),
                        StreamedAssistantContent::ReasoningDelta { reasoning, .. } => {
                            Ok(StreamChunk::Reasoning(reasoning))
                        }
                        StreamedAssistantContent::Reasoning(r) => {
                            Ok(StreamChunk::Reasoning(r.display_text()))
                        }
                        _ => Ok(StreamChunk::Text("".to_string())),
                    },
                    Ok(MultiTurnStreamItem::CompletionCall(call)) => {
                        Ok(StreamChunk::Usage(call.usage))
                    }
                    Ok(MultiTurnStreamItem::FinalResponse(resp)) => {
                        Ok(StreamChunk::Usage(resp.usage))
                    }
                    Err(e) => Err(Box::new(e) as Box<dyn std::error::Error + Send + Sync>),
                    _ => Ok(StreamChunk::Text("".to_string())),
                });

                Ok(mapped.boxed())
            }
            ActiveAgent::DeepSeek(deepseek_prov) => {
                let stream = deepseek_prov
                    .agent
                    .stream_prompt(prompt)
                    .max_turns(max_turns as usize)
                    .await;

                let mapped = stream.map(|item_res| match item_res {
                    Ok(MultiTurnStreamItem::StreamAssistantItem(content)) => match content {
                        StreamedAssistantContent::Text(text) => Ok(StreamChunk::Text(text.text)),
                        StreamedAssistantContent::ReasoningDelta { reasoning, .. } => {
                            Ok(StreamChunk::Reasoning(reasoning))
                        }
                        StreamedAssistantContent::Reasoning(r) => {
                            Ok(StreamChunk::Reasoning(r.display_text()))
                        }
                        _ => Ok(StreamChunk::Text("".to_string())),
                    },
                    Ok(MultiTurnStreamItem::CompletionCall(call)) => {
                        Ok(StreamChunk::Usage(call.usage))
                    }
                    Ok(MultiTurnStreamItem::FinalResponse(resp)) => {
                        Ok(StreamChunk::Usage(resp.usage))
                    }
                    Err(e) => Err(Box::new(e) as Box<dyn std::error::Error + Send + Sync>),
                    _ => Ok(StreamChunk::Text("".to_string())),
                });

                Ok(mapped.boxed())
            }
        }
    }

    pub fn listar_provedores() -> &'static [ModeloLLM] {
        &[ModeloLLM::Gemini, ModeloLLM::DeepSeek]
    }
}

impl ModeloLLM {
    pub async fn listar_modelos(
        &self,
        api_key: &str,
    ) -> Result<Vec<InfoModelo>, Box<dyn std::error::Error + Send + Sync>> {
        match self {
            ModeloLLM::DeepSeek => {
                let models = ProvedorDeepSeek::listar_modelos(api_key, None).await?;
                Ok(models
                    .into_iter()
                    .map(|m| InfoModelo {
                        id: m.id,
                        owned_by: m.owned_by,
                    })
                    .collect())
            }
            ModeloLLM::Gemini => {
                let models = ProvedorGemini::listar_modelos(api_key, None).await?;
                Ok(models
                    .into_iter()
                    .map(|m| {
                        let id = m
                            .name
                            .strip_prefix("models/")
                            .unwrap_or(&m.name)
                            .to_string();
                        InfoModelo {
                            id,
                            owned_by: "google".to_string(),
                        }
                    })
                    .collect())
            }
        }
    }
}
