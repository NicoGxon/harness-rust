use rig::tool::Tool;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::fs;

#[derive(Deserialize, Debug)]
pub struct ListDirArgs {
    pub path: Option<String>,
}

#[derive(Debug)]
pub struct ListDirError(pub String);

impl std::fmt::Display for ListDirError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Error al listar directorio: {}", self.0)
    }
}

impl std::error::Error for ListDirError {}

#[derive(Deserialize, Serialize, Clone)]
pub struct ListDirTool;

impl Tool for ListDirTool {
    const NAME: &'static str = "listar_directorio";

    type Error = ListDirError;
    type Args = ListDirArgs;
    type Output = String;

    fn description(&self) -> String {
        "Lista los archivos y subdirectorios presentes en la ruta especificada (o el directorio actual si no se provee).".to_string()
    }

    fn parameters(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Ruta opcional del directorio a listar (ej. '.', './src'). Por defecto es '.'."
                }
            }
        })
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let dir_path = args.path.unwrap_or_else(|| ".".to_string());
        tracing::info!(
            "[HERRAMIENTA] Ejecutando 'listar_directorio'. Path: '{}'",
            dir_path
        );

        let entries = fs::read_dir(&dir_path).map_err(|e| ListDirError(e.to_string()))?;

        let mut files = Vec::new();
        let mut dirs = Vec::new();

        for entry in entries.flatten() {
            let is_dir = entry.file_type().map(|ft| ft.is_dir()).unwrap_or(false);
            let mut file_name = entry.file_name().to_string_lossy().into_owned();

            if is_dir {
                file_name.push('/');
                dirs.push(file_name);
            } else {
                files.push(file_name);
            }
        }

        dirs.sort();
        files.sort();

        let mut output = format!("Directorio '{}':\n", dir_path);
        output.push_str("📁 Subdirectorios:\n");
        if dirs.is_empty() {
            output.push_str("  (Ninguno)\n");
        } else {
            for d in &dirs {
                output.push_str(&format!("  - {}\n", d));
            }
        }

        output.push_str("📄 Archivos:\n");
        if files.is_empty() {
            output.push_str("  (Ninguno)\n");
        } else {
            for f in &files {
                output.push_str(&format!("  - {}\n", f));
            }
        }

        Ok(output)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rig::tool::Tool;

    #[tokio::test]
    async fn test_list_dir_success() {
        let tool = ListDirTool;
        let args = ListDirArgs {
            path: Some(".".to_string()),
        };

        let result = tool.call(args).await;
        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(output.contains("Directorio '.'"));
        assert!(output.contains("📄 Archivos:") || output.contains("📁 Subdirectorios:"));
    }

    #[tokio::test]
    async fn test_list_dir_error() {
        let tool = ListDirTool;
        let args = ListDirArgs {
            path: Some("/ruta_totalmente_invalida_999".to_string()),
        };

        let result = tool.call(args).await;
        assert!(result.is_err());
    }
}
