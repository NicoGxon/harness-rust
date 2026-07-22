use serde::Deserialize;
use rig::{
    agent::Agent, client::CompletionClient, memory::InMemoryConversationMemory, providers::gemini,
};
use tools::{CreateFileTool, ListDirTool, ReadFileTool};

pub struct ProvedorGemini {
    pub agent: Agent<gemini::completion::CompletionModel>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct GeminiModelsResponse {
    pub models: Vec<GeminiModelEntry>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct GeminiModelEntry {
    pub name: String,
    #[serde(rename = "displayName")]
    pub display_name: String,
}

impl ProvedorGemini {
    pub fn new(
        preamble: &str,
        memory: InMemoryConversationMemory,
        model: &str,
        api_key: &str,
        temperatura: f64,
    ) -> Self {
        let cliente = gemini::Client::new(api_key).expect("Failed to initialize gemini client");

        let agent = cliente
            .agent(model)
            .preamble(preamble)
            .memory(memory)
            .conversation("typhon-chat")
            .temperature(temperatura)
            .tool(CreateFileTool)
            .tool(ReadFileTool)
            .tool(ListDirTool)
            .build();

        ProvedorGemini { agent }
    }

    pub async fn listar_modelos(
        api_key: &str,
        base_url: Option<&str>,
    ) -> Result<Vec<GeminiModelEntry>, Box<dyn std::error::Error + Send + Sync>> {
        let base = base_url.unwrap_or("https://generativelanguage.googleapis.com");
        let url = format!("{}/v1beta/models?key={}", base, api_key);
        let http = reqwest::Client::new();
        let resp = http
            .get(&url)
            .send()
            .await?
            .json::<GeminiModelsResponse>()
            .await?;
        Ok(resp.models)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    use tokio::net::TcpListener;

    #[tokio::test]
    async fn test_list_gemini_models() {
        // Bind to a local port
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let mock_url = format!("http://{}", addr);

        // Spawn mock server handler in a background task
        tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let (reader, mut writer) = socket.split();
            let mut reader = BufReader::new(reader);
            let mut request_line = String::new();
            reader.read_line(&mut request_line).await.unwrap();

            // Read headers to consume request
            let mut line = String::new();
            loop {
                line.clear();
                reader.read_line(&mut line).await.unwrap();
                if line == "\r\n" || line.is_empty() {
                    break;
                }
            }

            // Verify the request path and query parameters
            assert!(request_line.contains("GET /v1beta/models?key=mock-api-key HTTP/1.1"));

            // Respond with mock JSON matching Google API structure
            let response_body = r#"{
                "models": [
                    {
                        "name": "models/gemini-1.5-flash",
                        "displayName": "Gemini 1.5 Flash"
                    },
                    {
                        "name": "models/gemini-1.5-pro",
                        "displayName": "Gemini 1.5 Pro"
                    }
                ]
            }"#;

            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                response_body.len(),
                response_body
            );
            writer.write_all(response.as_bytes()).await.unwrap();
            writer.flush().await.unwrap();
        });

        // Call listar_modelos with mock URL
        let models_result = ProvedorGemini::listar_modelos("mock-api-key", Some(&mock_url)).await;

        assert!(models_result.is_ok(), "La llamada a la API debe ser exitosa");
        let models = models_result.unwrap();
        assert!(!models.is_empty(), "La lista de modelos no debe estar vacía");

        for model in models {
            assert!(!model.name.is_empty(), "El nombre del modelo no debe estar vacío");
            assert!(!model.display_name.is_empty(), "El nombre a mostrar (displayName) no debe estar vacío");
        }
    }
}
