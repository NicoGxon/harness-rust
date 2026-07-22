use rig::tool::Tool;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::fs::File;
use std::io::Write;

// Definimos los argumentos que el LLM debe proporcionar
#[derive(Deserialize, Debug)]
pub struct CreateFileArgs {
    pub path: String,
    pub content: String,
}

// Error personalizado para el sistema de archivos
#[derive(Debug)]
pub struct FileError(String);

impl std::fmt::Display for FileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Error de archivo: {}", self.0)
    }
}

impl std::error::Error for FileError {}

// El struct que representa nuestra herramienta
#[derive(Deserialize, Serialize, Clone)]
pub struct CreateFileTool;

// Implementamos el Trait Tool para CreateFileTool en rig 0.40.0
impl Tool for CreateFileTool {
    const NAME: &'static str = "crear_archivo";

    type Error = FileError;
    type Args = CreateFileArgs;
    type Output = String;

    fn description(&self) -> String {
        "Crea un archivo de texto en el disco con el nombre y contenido especificados.".to_string()
    }

    fn parameters(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "La ruta o nombre del archivo a crear (ejemplo: 'nota.txt')"
                },
                "content": {
                    "type": "string",
                    "description": "El contenido de texto que se va a escribir dentro del archivo"
                }
            },
            "required": ["path", "content"]
        })
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        tracing::info!(
            "[HERRAMIENTA] Ejecutando 'crear_archivo'. Path: '{}', Content length: {} bytes",
            args.path,
            args.content.len()
        );
        let mut file = File::create(&args.path).map_err(|e| FileError(e.to_string()))?;

        file.write_all(args.content.as_bytes())
            .map_err(|e| FileError(e.to_string()))?;

        Ok(format!("Archivo '{}' creado de forma exitosa.", args.path))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rig::tool::Tool;
    use std::fs;

    #[tokio::test]
    async fn test_create_file_tool_success() {
        let temp_dir = std::env::temp_dir();
        let file_path = temp_dir.join("test_typhon_create_file.txt");
        let file_path_str = file_path.to_string_lossy().to_string();

        let tool = CreateFileTool;
        let args = CreateFileArgs {
            path: file_path_str.clone(),
            content: "Hello from tests!".to_string(),
        };

        let result = tool.call(args).await;
        assert!(result.is_ok());
        assert_eq!(
            result.unwrap(),
            format!("Archivo '{}' creado de forma exitosa.", file_path_str)
        );

        // Read file contents to verify
        let content = fs::read_to_string(&file_path).unwrap();
        assert_eq!(content, "Hello from tests!");

        // Clean up
        let _ = fs::remove_file(file_path);
    }

    #[tokio::test]
    async fn test_create_file_tool_error() {
        let tool = CreateFileTool;
        // Using an invalid path like a directory that doesn't exist to trigger an error
        let args = CreateFileArgs {
            path: "/nonexistent_directory_xyz/file.txt".to_string(),
            content: "test".to_string(),
        };

        let result = tool.call(args).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("Error de archivo:"));
    }

    #[test]
    fn test_tool_metadata() {
        let tool = CreateFileTool;
        assert_eq!(CreateFileTool::NAME, "crear_archivo");
        assert!(!tool.description().is_empty());
        let params = tool.parameters();
        assert_eq!(params["type"], "object");
        assert!(
            params["required"]
                .as_array()
                .unwrap()
                .contains(&json!("path"))
        );
        assert!(
            params["required"]
                .as_array()
                .unwrap()
                .contains(&json!("content"))
        );
    }
}
