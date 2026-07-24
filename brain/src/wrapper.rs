use crate::provedores::deepseek::ProvedorDeepSeek;
use crate::provedores::gemini::ProvedorGemini;
use futures::StreamExt;
use futures::stream::BoxStream;
use rig::agent::MultiTurnStreamItem;
use rig::memory::InMemoryConversationMemory;
use rig::streaming::{StreamedAssistantContent, StreamingPrompt};

/// Error específico del módulo brain.
#[derive(Debug, thiserror::Error)]
pub enum BrainError {
    /// Error durante el streaming de una respuesta del proveedor.
    #[error("Error de streaming del proveedor: {0}")]
    Stream(String),

    /// Error de red al listar modelos disponibles.
    #[error("Error al listar modelos: {0}")]
    ListModels(String),
}

/// Identificador de proveedor LLM soportado.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ModeloLLM {
    Gemini,
    DeepSeek,
}

impl std::fmt::Display for ModeloLLM {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Gemini => write!(f, "Gemini"),
            Self::DeepSeek => write!(f, "DeepSeek"),
        }
    }
}

/// Variante interna que contiene el agente activo del proveedor seleccionado.
pub enum ActiveAgent {
    Gemini(ProvedorGemini),
    DeepSeek(ProvedorDeepSeek),
}

/// Información normalizada de un modelo LLM, independiente del proveedor.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct InfoModelo {
    pub id: String,
    pub owned_by: String,
}

/// Fragmento de una respuesta en streaming del LLM.
#[derive(Debug, Clone)]
pub enum StreamChunk {
    /// Texto generado por el modelo.
    Text(String),
    /// Razonamiento intermedio (chain-of-thought).
    Reasoning(String),
    /// Métricas de uso de tokens al finalizar un turno.
    Usage(rig::completion::Usage),
}

/// Wrapper principal que abstrae la interacción con distintos proveedores LLM.
///
/// Permite crear un agente y hacer streaming de prompts de forma uniforme,
/// independientemente de si el backend es Gemini o DeepSeek.
pub struct BrainWrapper {
    agent: ActiveAgent,
}

/// Convierte un item del stream multi-turno de `rig` en un [`StreamChunk`] normalizado.
///
/// Esta función centraliza la lógica de mapeo que antes estaba duplicada
/// en cada brazo del `match` por proveedor dentro de [`BrainWrapper::prompt`].
fn map_stream_item<R, E: std::error::Error + Send + Sync + 'static>(
    item: Result<MultiTurnStreamItem<R>, E>,
) -> Result<StreamChunk, BrainError> {
    match item {
        Ok(MultiTurnStreamItem::StreamAssistantItem(content)) => match content {
            StreamedAssistantContent::Text(text) => Ok(StreamChunk::Text(text.text)),
            StreamedAssistantContent::ReasoningDelta { reasoning, .. } => {
                Ok(StreamChunk::Reasoning(reasoning))
            }
            StreamedAssistantContent::Reasoning(r) => Ok(StreamChunk::Reasoning(r.display_text())),
            _ => Ok(StreamChunk::Text(String::new())),
        },
        Ok(MultiTurnStreamItem::CompletionCall(call)) => Ok(StreamChunk::Usage(call.usage)),
        Ok(MultiTurnStreamItem::FinalResponse(resp)) => Ok(StreamChunk::Usage(resp.usage)),
        Err(e) => Err(BrainError::Stream(e.to_string())),
        _ => Ok(StreamChunk::Text(String::new())),
    }
}

impl BrainWrapper {
    /// Crea un nuevo `BrainWrapper` con el proveedor y modelo especificados.
    ///
    /// # Argumentos
    /// * `provedor` - El proveedor LLM a utilizar.
    /// * `model` - Identificador del modelo (e.g. `"gemini-2.0-flash"`).
    /// * `preamble` - System prompt del agente.
    /// * `api_key` - Clave de API del proveedor.
    /// * `temperature` - Temperatura de generación (0.0 - 2.0).
    /// * `memory` - Memoria de conversación para contexto multi-turno.
    pub fn new(
        provedor: ModeloLLM,
        model: &str,
        preamble: &str,
        api_key: &str,
        temperature: f64,
        memory: InMemoryConversationMemory,
    ) -> Self {
        let agent = match provedor {
            ModeloLLM::Gemini => ActiveAgent::Gemini(ProvedorGemini::new(
                preamble,
                memory,
                model,
                api_key,
                temperature,
            )),
            ModeloLLM::DeepSeek => ActiveAgent::DeepSeek(ProvedorDeepSeek::new(
                preamble,
                memory,
                model,
                api_key,
                temperature,
            )),
        };
        Self { agent }
    }

    /// Envía un prompt al agente activo y retorna un stream de [`StreamChunk`].
    ///
    /// El stream emite fragmentos de texto y razonamiento a medida que el modelo
    /// los genera, seguidos de métricas de uso de tokens al finalizar cada turno.
    pub async fn prompt(
        &self,
        prompt: &str,
        max_turns: u32,
    ) -> Result<BoxStream<'_, Result<StreamChunk, BrainError>>, BrainError> {
        let max = max_turns as usize;
        match &self.agent {
            ActiveAgent::Gemini(g) => {
                let stream = g.agent.stream_prompt(prompt).max_turns(max).await;
                Ok(stream.map(map_stream_item).boxed())
            }
            ActiveAgent::DeepSeek(d) => {
                let stream = d.agent.stream_prompt(prompt).max_turns(max).await;
                Ok(stream.map(map_stream_item).boxed())
            }
        }
    }

    /// Retorna la lista estática de proveedores LLM soportados.
    pub fn listar_provedores() -> &'static [ModeloLLM] {
        &[ModeloLLM::Gemini, ModeloLLM::DeepSeek]
    }
}

impl ModeloLLM {
    /// Lista los modelos disponibles del proveedor, consultando su API remota.
    ///
    /// Retorna una lista normalizada de [`InfoModelo`] independiente del proveedor.
    /// Para Gemini, el prefijo `"models/"` se elimina del ID del modelo.
    pub async fn listar_modelos(&self, api_key: &str) -> Result<Vec<InfoModelo>, BrainError> {
        match self {
            ModeloLLM::DeepSeek => {
                let models = ProvedorDeepSeek::listar_modelos(api_key, None)
                    .await
                    .map_err(|e| BrainError::ListModels(e.to_string()))?;
                Ok(models
                    .into_iter()
                    .map(|m| InfoModelo {
                        id: m.id,
                        owned_by: m.owned_by,
                    })
                    .collect())
            }
            ModeloLLM::Gemini => {
                let models = ProvedorGemini::listar_modelos(api_key, None)
                    .await
                    .map_err(|e| BrainError::ListModels(e.to_string()))?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provedores::deepseek::ModelEntry;
    use crate::provedores::gemini::GeminiModelEntry;

    // ── BrainError ─────────────────────────────────────────────────

    #[test]
    fn test_brain_error_stream_display() {
        let err = BrainError::Stream("connection reset".to_string());
        assert_eq!(
            err.to_string(),
            "Error de streaming del proveedor: connection reset"
        );
    }

    #[test]
    fn test_brain_error_list_models_display() {
        let err = BrainError::ListModels("timeout".to_string());
        assert_eq!(err.to_string(), "Error al listar modelos: timeout");
    }

    #[test]
    fn test_brain_error_is_std_error() {
        let err = BrainError::Stream("test".to_string());
        // Verifica que BrainError implementa std::error::Error
        let _: &dyn std::error::Error = &err;
    }

    #[test]
    fn test_brain_error_debug() {
        let err = BrainError::Stream("oops".to_string());
        let debug = format!("{:?}", err);
        assert!(debug.contains("Stream"));
        assert!(debug.contains("oops"));
    }

    // ── ModeloLLM: derived traits + Display ────────────────────────

    #[test]
    fn test_modelo_llm_debug() {
        assert_eq!(format!("{:?}", ModeloLLM::Gemini), "Gemini");
        assert_eq!(format!("{:?}", ModeloLLM::DeepSeek), "DeepSeek");
    }

    #[test]
    fn test_modelo_llm_display() {
        assert_eq!(format!("{}", ModeloLLM::Gemini), "Gemini");
        assert_eq!(format!("{}", ModeloLLM::DeepSeek), "DeepSeek");
    }

    #[test]
    fn test_modelo_llm_display_vs_debug_consistency() {
        // Display y Debug producen el mismo resultado para este enum
        for m in BrainWrapper::listar_provedores() {
            assert_eq!(format!("{}", m), format!("{:?}", m));
        }
    }

    #[test]
    fn test_modelo_llm_clone_and_copy() {
        let a = ModeloLLM::Gemini;
        let b = a; // Copy — `a` sigue siendo válido
        let c = a.clone();
        assert_eq!(a, b);
        assert_eq!(a, c);
    }

    #[test]
    fn test_modelo_llm_eq_and_ne() {
        assert_eq!(ModeloLLM::Gemini, ModeloLLM::Gemini);
        assert_eq!(ModeloLLM::DeepSeek, ModeloLLM::DeepSeek);
        assert_ne!(ModeloLLM::Gemini, ModeloLLM::DeepSeek);
    }

    // ── InfoModelo: serialization ──────────────────────────────────

    #[test]
    fn test_info_modelo_serialize_roundtrip() {
        let info = InfoModelo {
            id: "test-model-v2".to_string(),
            owned_by: "acme-corp".to_string(),
        };
        let json = serde_json::to_string(&info).unwrap();
        let back: InfoModelo = serde_json::from_str(&json).unwrap();
        assert_eq!(back.id, "test-model-v2");
        assert_eq!(back.owned_by, "acme-corp");
    }

    #[test]
    fn test_info_modelo_deserialize_from_json() {
        let json = r#"{"id":"deepseek-chat","owned_by":"deepseek"}"#;
        let info: InfoModelo = serde_json::from_str(json).unwrap();
        assert_eq!(info.id, "deepseek-chat");
        assert_eq!(info.owned_by, "deepseek");
    }

    #[test]
    fn test_info_modelo_clone() {
        let original = InfoModelo {
            id: "model-x".to_string(),
            owned_by: "owner-y".to_string(),
        };
        let cloned = original.clone();
        assert_eq!(original.id, cloned.id);
        assert_eq!(original.owned_by, cloned.owned_by);
    }

    #[test]
    fn test_info_modelo_json_field_names() {
        let info = InfoModelo {
            id: "x".to_string(),
            owned_by: "y".to_string(),
        };
        let json = serde_json::to_value(&info).unwrap();
        // Verifica que los campos JSON se llaman exactamente "id" y "owned_by"
        assert!(json.get("id").is_some());
        assert!(json.get("owned_by").is_some());
    }

    // ── BrainWrapper::listar_provedores ────────────────────────────

    #[test]
    fn test_listar_provedores_contenido() {
        let provs = BrainWrapper::listar_provedores();
        assert_eq!(provs.len(), 2);
        assert!(provs.contains(&ModeloLLM::Gemini));
        assert!(provs.contains(&ModeloLLM::DeepSeek));
    }

    #[test]
    fn test_listar_provedores_orden() {
        let provs = BrainWrapper::listar_provedores();
        assert_eq!(provs[0], ModeloLLM::Gemini);
        assert_eq!(provs[1], ModeloLLM::DeepSeek);
    }

    // ── Mapping: Gemini → InfoModelo (strip_prefix logic) ──────────

    #[test]
    fn test_gemini_mapping_strips_models_prefix() {
        let entries = vec![
            GeminiModelEntry {
                name: "models/gemini-2.0-flash".to_string(),
                display_name: "Gemini 2.0 Flash".to_string(),
            },
            GeminiModelEntry {
                name: "models/gemini-1.5-pro".to_string(),
                display_name: "Gemini 1.5 Pro".to_string(),
            },
        ];

        // Replica la lógica de ModeloLLM::listar_modelos para Gemini
        let infos: Vec<InfoModelo> = entries
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
            .collect();

        assert_eq!(infos[0].id, "gemini-2.0-flash");
        assert_eq!(infos[1].id, "gemini-1.5-pro");
        assert!(infos.iter().all(|i| i.owned_by == "google"));
    }

    #[test]
    fn test_gemini_mapping_without_prefix_is_passthrough() {
        let entry = GeminiModelEntry {
            name: "custom-model-sin-prefijo".to_string(),
            display_name: "Custom".to_string(),
        };

        let id = entry
            .name
            .strip_prefix("models/")
            .unwrap_or(&entry.name)
            .to_string();
        assert_eq!(id, "custom-model-sin-prefijo");
    }

    // ── Mapping: DeepSeek → InfoModelo ─────────────────────────────

    #[test]
    fn test_deepseek_mapping_fields() {
        let entries = vec![
            ModelEntry {
                id: "deepseek-chat".to_string(),
                owned_by: "deepseek".to_string(),
            },
            ModelEntry {
                id: "deepseek-reasoner".to_string(),
                owned_by: "deepseek".to_string(),
            },
        ];

        // Replica la lógica de ModeloLLM::listar_modelos para DeepSeek
        let infos: Vec<InfoModelo> = entries
            .into_iter()
            .map(|m| InfoModelo {
                id: m.id,
                owned_by: m.owned_by,
            })
            .collect();

        assert_eq!(infos.len(), 2);
        assert_eq!(infos[0].id, "deepseek-chat");
        assert_eq!(infos[0].owned_by, "deepseek");
        assert_eq!(infos[1].id, "deepseek-reasoner");
    }

    // ── StreamChunk ────────────────────────────────────────────────

    #[test]
    fn test_stream_chunk_text_variant() {
        let chunk = StreamChunk::Text("hola mundo".to_string());
        match chunk {
            StreamChunk::Text(t) => assert_eq!(t, "hola mundo"),
            _ => panic!("Se esperaba la variante Text"),
        }
    }

    #[test]
    fn test_stream_chunk_reasoning_variant() {
        let chunk = StreamChunk::Reasoning("pensando...".to_string());
        match chunk {
            StreamChunk::Reasoning(r) => assert_eq!(r, "pensando..."),
            _ => panic!("Se esperaba la variante Reasoning"),
        }
    }

    #[test]
    fn test_stream_chunk_debug_format() {
        let chunk = StreamChunk::Text("test".to_string());
        let debug = format!("{:?}", chunk);
        assert!(debug.contains("Text"));
        assert!(debug.contains("test"));
    }

    // ── map_stream_item (función extraída) ─────────────────────────

    #[test]
    fn test_map_stream_item_error() {
        // Simula un error del stream usando un tipo de error simple
        let err: Result<MultiTurnStreamItem<()>, std::io::Error> = Err(std::io::Error::new(
            std::io::ErrorKind::BrokenPipe,
            "broken",
        ));

        let result = map_stream_item(err);
        assert!(result.is_err());

        match result.unwrap_err() {
            BrainError::Stream(msg) => assert!(msg.contains("broken")),
            other => panic!("Se esperaba BrainError::Stream, se obtuvo: {:?}", other),
        }
    }
}
