use crate::provedores::openai::ReasoningEffort;
use rig::{
    agent::Agent, client::CompletionClient, memory::InMemoryConversationMemory, providers::chatgpt,
};
use serde::Deserialize;
use std::path::Path;
use tools::{CreateFileTool, ListDirTool, ReadFileTool};

const CHATGPT_MODELS_URL: &str = "https://chatgpt.com/backend-api/codex/models";
const CHATGPT_CLIENT_VERSION: &str = "0.145.0";

pub struct ProvedorChatGPT {
    pub agent: Agent<chatgpt::ResponsesCompletionModel>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct ChatGPTModelEntry {
    pub slug: String,
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub visibility: Option<String>,
    #[serde(default)]
    pub priority: Option<i32>,
    #[serde(default)]
    pub tool_mode: Option<String>,
}

#[derive(Deserialize, Debug)]
struct ChatGPTModelsResponse {
    models: Vec<ChatGPTModelEntry>,
}

#[derive(Deserialize, Debug)]
struct OAuthCache {
    access_token: Option<String>,
    account_id: Option<String>,
}

impl ProvedorChatGPT {
    pub fn new(
        preamble: &str,
        memory: InMemoryConversationMemory,
        model: &str,
        auth_file: &Path,
        reasoning_effort: Option<ReasoningEffort>,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let cliente = chatgpt::Client::builder()
            .oauth()
            .auth_file(auth_file)
            .allow_device_flow(false)
            .build()?;

        let mut builder = cliente
            .agent(model)
            .preamble(preamble)
            .memory(memory)
            .conversation("typhon-chat");
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

    pub async fn autorizar(
        auth_file: &Path,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let cliente = chatgpt::Client::builder()
            .oauth()
            .auth_file(auth_file)
            .allow_device_flow(true)
            .build()?;
        cliente.authorize().await?;
        Ok(())
    }

    pub async fn listar_modelos(
        auth_file: &Path,
    ) -> Result<Vec<ChatGPTModelEntry>, Box<dyn std::error::Error + Send + Sync>> {
        Self::autorizar(auth_file).await?;
        let cache = read_oauth_cache(auth_file)?;
        let access_token = cache
            .access_token
            .filter(|value| !value.trim().is_empty())
            .ok_or("La caché OAuth no contiene access_token")?;

        let mut request = reqwest::Client::new()
            .get(CHATGPT_MODELS_URL)
            .query(&[("client_version", CHATGPT_CLIENT_VERSION)])
            .bearer_auth(access_token)
            .header("originator", "typhon")
            .header(
                "user-agent",
                format!("typhon/{}", env!("CARGO_PKG_VERSION")),
            );
        if let Some(account_id) = cache.account_id.filter(|value| !value.trim().is_empty()) {
            request = request.header("ChatGPT-Account-Id", account_id);
        }

        let response = request
            .send()
            .await?
            .error_for_status()?
            .json::<ChatGPTModelsResponse>()
            .await?;

        let mut models: Vec<_> = response
            .models
            .into_iter()
            // `tool_mode` describes how the Codex backend expects the model
            // to be called; it is not a visibility flag. In particular,
            // `code_mode_only` models are valid subscription models and must
            // remain selectable.
            .filter(|model| model.visibility.as_deref().unwrap_or("list") == "list")
            .collect();
        models.sort_by_key(|model| model.priority.unwrap_or(i32::MAX));
        Ok(models)
    }

    pub fn info_modelo(model: &ChatGPTModelEntry) -> (String, String) {
        (model.slug.clone(), "chatgpt".to_string())
    }
}

fn read_oauth_cache(path: &Path) -> Result<OAuthCache, Box<dyn std::error::Error + Send + Sync>> {
    let content = std::fs::read_to_string(path)?;
    Ok(serde_json::from_str(&content)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_listed_codex_only_models_but_hides_internal_models() {
        let models = vec![
            ChatGPTModelEntry {
                slug: "gpt-direct".to_string(),
                display_name: Some("GPT Direct".to_string()),
                visibility: Some("list".to_string()),
                priority: Some(1),
                tool_mode: Some("direct".to_string()),
            },
            ChatGPTModelEntry {
                slug: "gpt-codex-only".to_string(),
                display_name: None,
                visibility: Some("list".to_string()),
                priority: Some(2),
                tool_mode: Some("code_mode_only".to_string()),
            },
            ChatGPTModelEntry {
                slug: "codex-auto-review".to_string(),
                display_name: None,
                visibility: Some("hide".to_string()),
                priority: Some(3),
                tool_mode: None,
            },
        ];
        let listed: Vec<_> = models
            .into_iter()
            .filter(|model| model.visibility.as_deref().unwrap_or("list") == "list")
            .collect();
        assert_eq!(listed.len(), 2);
        assert_eq!(listed[1].slug, "gpt-codex-only");
    }
}
