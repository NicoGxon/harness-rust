use rig::{
    agent::Agent, client::CompletionClient, memory::InMemoryConversationMemory, providers::deepseek,
};
use serde::Deserialize;
use tools::{CreateFileTool, ListDirTool, ReadFileTool};

pub struct ProvedorDeepSeek {
    pub agent: Agent<deepseek::CompletionModel>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct ModelsResponse {
    pub data: Vec<ModelEntry>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct ModelEntry {
    pub id: String,
    pub owned_by: String,
}

impl ProvedorDeepSeek {
    pub fn new(
        preamble: &str,
        memory: InMemoryConversationMemory,
        model: &str,
        api_key: &str,
        temperatura: f64,
    ) -> Self {
        let cliente = deepseek::Client::new(api_key).expect("Failed to initialize deepseek client");

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

        ProvedorDeepSeek { agent }
    }

    pub async fn listar_modelos(
        api_key: &str,
        base_url: Option<&str>,
    ) -> Result<Vec<ModelEntry>, Box<dyn std::error::Error + Send + Sync>> {
        let base = base_url.unwrap_or("https://api.deepseek.com");
        let url = format!("{}/models", base);
        let http = reqwest::Client::new();
        let resp = http
            .get(&url)
            .bearer_auth(api_key)
            .send()
            .await?
            .json::<ModelsResponse>()
            .await?;
        Ok(resp.data)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    use tokio::net::TcpListener;

    #[tokio::test]
    async fn test_list_deepseek_models() {
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

            // Verify the request path and method
            assert!(request_line.contains("GET /models HTTP/1.1"));

            // Respond with a mock JSON
            let response_body = r#"{
                "object": "list",
                "data": [
                    {
                        "id": "deepseek-chat",
                        "object": "model",
                        "owned_by": "deepseek"
                    },
                    {
                        "id": "deepseek-reasoner",
                        "object": "model",
                        "owned_by": "deepseek"
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

        // Call list_deepseek_models with our mock URL
        let models_result = ProvedorDeepSeek::listar_modelos("mock-api-key", Some(&mock_url)).await;

        assert!(
            models_result.is_ok(),
            "La llamada a la API debe ser exitosa"
        );
        let models = models_result.unwrap();
        assert!(
            !models.is_empty(),
            "La lista de modelos retornada no debe estar vacía"
        );

        for model in models {
            assert!(!model.id.is_empty(), "El ID del modelo no debe estar vacío");
            assert!(
                !model.owned_by.is_empty(),
                "El creador/dueño (owned_by) del modelo no debe estar vacío"
            );
        }
    }
}
