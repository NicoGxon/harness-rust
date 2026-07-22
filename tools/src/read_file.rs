use rig::tool::Tool;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::fs;

#[derive(Deserialize, Debug)]
pub struct ReadFileArgs {
    pub path: String,
    pub start_line: Option<usize>,
    pub end_line: Option<usize>,
}

#[derive(Debug)]
pub struct ReadFileError(pub String);

impl std::fmt::Display for ReadFileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Error al leer archivo: {}", self.0)
    }
}

impl std::error::Error for ReadFileError {}

#[derive(Deserialize, Serialize, Clone)]
pub struct ReadFileTool;

impl Tool for ReadFileTool {
    const NAME: &'static str = "leer_archivo";

    type Error = ReadFileError;
    type Args = ReadFileArgs;
    type Output = String;

    fn description(&self) -> String {
        "Lee el contenido de un archivo de texto en el disco. Opcionalmente se puede especificar un rango de líneas (1-based).".to_string()
    }

    fn parameters(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "La ruta del archivo de texto a leer"
                },
                "start_line": {
                    "type": "integer",
                    "description": "Línea inicial opcional (indexada en 1, inclusive)"
                },
                "end_line": {
                    "type": "integer",
                    "description": "Línea final opcional (indexada en 1, inclusive)"
                }
            },
            "required": ["path"]
        })
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        tracing::info!(
            "[HERRAMIENTA] Ejecutando 'leer_archivo'. Path: '{}', start_line: {:?}, end_line: {:?}",
            args.path,
            args.start_line,
            args.end_line
        );

        let content = fs::read_to_string(&args.path).map_err(|e| ReadFileError(e.to_string()))?;

        if args.start_line.is_none() && args.end_line.is_none() {
            return Ok(content);
        }

        let lines: Vec<&str> = content.lines().collect();
        let total_lines = lines.len();

        let start = args.start_line.unwrap_or(1).saturating_sub(1);
        let end = args.end_line.unwrap_or(total_lines).min(total_lines);

        if start >= total_lines || start >= end {
            return Ok(format!(
                "[Rango inválido: el archivo '{}' tiene {} líneas, se solicitó de {} a {}]",
                args.path,
                total_lines,
                start + 1,
                end
            ));
        }

        let selected = lines[start..end].join("\n");
        Ok(selected)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rig::tool::Tool;
    use std::fs::File;
    use std::io::Write;

    #[tokio::test]
    async fn test_read_file_tool_full() {
        let temp_dir = std::env::temp_dir();
        let file_path = temp_dir.join("test_typhon_read_file.txt");
        let file_path_str = file_path.to_string_lossy().to_string();

        let mut file = File::create(&file_path).unwrap();
        writeln!(file, "Línea 1").unwrap();
        writeln!(file, "Línea 2").unwrap();
        writeln!(file, "Línea 3").unwrap();

        let tool = ReadFileTool;
        let args = ReadFileArgs {
            path: file_path_str.clone(),
            start_line: None,
            end_line: None,
        };

        let result = tool.call(args).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().trim(), "Línea 1\nLínea 2\nLínea 3");

        let _ = fs::remove_file(file_path);
    }

    #[tokio::test]
    async fn test_read_file_tool_slice() {
        let temp_dir = std::env::temp_dir();
        let file_path = temp_dir.join("test_typhon_read_slice.txt");
        let file_path_str = file_path.to_string_lossy().to_string();

        let mut file = File::create(&file_path).unwrap();
        writeln!(file, "Uno").unwrap();
        writeln!(file, "Dos").unwrap();
        writeln!(file, "Tres").unwrap();
        writeln!(file, "Cuatro").unwrap();

        let tool = ReadFileTool;
        let args = ReadFileArgs {
            path: file_path_str.clone(),
            start_line: Some(2),
            end_line: Some(3),
        };

        let result = tool.call(args).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "Dos\nTres");

        let _ = fs::remove_file(file_path);
    }

    #[tokio::test]
    async fn test_read_file_not_found() {
        let tool = ReadFileTool;
        let args = ReadFileArgs {
            path: "/ruta_inexistente_12345.txt".to_string(),
            start_line: None,
            end_line: None,
        };

        let result = tool.call(args).await;
        assert!(result.is_err());
    }
}
