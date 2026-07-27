pub mod reasoning;

pub use reasoning::ReasoningEffort;

use rig::{
    agent::Agent, client::CompletionClient, memory::InMemoryConversationMemory, providers::openai,
};
use serde::Deserialize;
use tools::{CreateFileTool, ListDirTool, ReadFileTool};

pub struct ProvedorOpenAI {
    pub agent: Agent<openai::responses_api::ResponsesCompletionModel>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct OpenAIModelEntry {
    pub id: String,
    #[serde(default = "default_owner")]
    pub owned_by: String,
}

#[derive(Deserialize, Debug)]
struct OpenAIModelsResponse {
    data: Vec<OpenAIModelEntry>,
}

fn default_owner() -> String {
    "openai".to_string()
}

impl ProvedorOpenAI {
    pub fn new(
        preamble: &str,
        memory: InMemoryConversationMemory,
        model: &str,
        api_key: &str,
        temperatura: f64,
        reasoning_effort: Option<ReasoningEffort>,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let cliente = openai::Client::new(api_key)?;
        let mut builder = cliente
            .agent(model)
            .preamble(preamble)
            .memory(memory)
            .conversation("typhon-chat");
        // Los modelos de razonamiento de OpenAI controlan la generación con
        // `reasoning.effort` y pueden rechazar `temperature`. En modo
        // automático conservamos la configuración existente de Typhon.
        if reasoning_effort.is_none() {
            builder = builder.temperature(temperatura);
        }
        if let Some(effort) = reasoning_effort {
            builder = builder.additional_params(serde_json::json!({
                "reasoning": {
                    "effort": effort.as_str(),
                    "summary": "auto"
                }
            }));
        }
        let agent = builder
            .tool(CreateFileTool)
            .tool(ReadFileTool)
            .tool(ListDirTool)
            .build();

        Ok(Self { agent })
    }

    pub async fn listar_modelos(
        api_key: &str,
    ) -> Result<Vec<OpenAIModelEntry>, Box<dyn std::error::Error + Send + Sync>> {
        let response = reqwest::Client::new()
            .get("https://api.openai.com/v1/models")
            .bearer_auth(api_key)
            .send()
            .await?
            .error_for_status()?
            .json::<OpenAIModelsResponse>()
            .await?;
        Ok(response.data)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_openai_model_listing() {
        let response: OpenAIModelsResponse = serde_json::from_str(
            r#"{"data":[{"id":"gpt-5","owned_by":"openai"},{"id":"custom"}]}"#,
        )
        .unwrap();

        assert_eq!(response.data.len(), 2);
        assert_eq!(response.data[0].id, "gpt-5");
        assert_eq!(response.data[1].owned_by, "openai");
    }
}
