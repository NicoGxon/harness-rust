#[cfg(test)]
mod tests_cli {
    use crate::tui::markdown::{MarkdownStreamProcessor, get_visible_len};

    #[test]
    fn test_visible_len() {
        assert_eq!(get_visible_len("**hello**", false), 5);
        assert_eq!(get_visible_len("`code`", false), 4);
        assert_eq!(get_visible_len("*italic*", false), 6);
        assert_eq!(get_visible_len("~~strike~~", false), 6);
        assert_eq!(get_visible_len("plain", false), 5);
        assert_eq!(get_visible_len("`code`", true), 6);
    }

    #[test]
    fn test_markdown_stream_processor() {
        let mut processor = MarkdownStreamProcessor::new(80);
        assert!(processor.write_chunk("## Encabezado\n").is_ok());
        assert!(processor.write_chunk("---\n").is_ok());
        assert!(processor.write_chunk("- Elemento **negrita**\n").is_ok());
        assert!(processor.write_chunk("> Cita con `código`\n").is_ok());
        assert!(
            processor
                .write_chunk("```rust\nfn main() {}\n```\n")
                .is_ok()
        );
        assert!(processor.flush_final().is_ok());
    }

    #[test]
    fn test_table_rendering() {
        let mut processor = MarkdownStreamProcessor::new(80);
        assert!(processor.write_chunk("| Columna A | Columna B |\n").is_ok());
        assert!(processor.write_chunk("| --- | --- |\n").is_ok());
        assert!(processor.write_chunk("| Dato 1 | Dato 2 |\n").is_ok());
        assert!(processor.flush_final().is_ok());
    }

    #[test]
    fn test_large_table_constrained() {
        let mut processor = MarkdownStreamProcessor::new(80);
        let large_table = "| Col 1 | Col 2 | Col 3 | Col 4 | Col 5 | Col 6 |\n\
                           | --- | --- | --- | --- | --- | --- |\n\
                           | Very Long Content That Exceeds Width | Short | Multi Word Text | Status Value | 100.00 | Active |\n";
        assert!(processor.write_chunk(large_table).is_ok());
        assert!(processor.flush_final().is_ok());
    }
}
