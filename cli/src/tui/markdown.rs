use console::style;
use std::io::{self, Write};

/// Envoltorio inteligente para streaming de Markdown en tiempo real que ajusta palabras a los márgenes,
/// formatea bloques de código con bordes, resalta encabezados, listas, citas y estilos inline (negritas, itálicas, código inline).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LineType {
    Normal,
    Header(u8),
    BulletList,
    NumberedList,
    Blockquote,
}

pub struct MarkdownStreamProcessor {
    max_cols: usize,
    col_count: usize,
    in_code_block: bool,

    // Inline state
    in_bold: bool,
    in_italic: bool,
    in_inline_code: bool,
    in_strikethrough: bool,

    // Line state
    is_line_start: bool,
    current_line_type: LineType,
    line_buffer: String,
    pending_word: String,

    // Table state
    in_table: bool,
    table_col_widths: Vec<usize>,

    // Reasoning state
    is_reasoning: bool,
}

impl MarkdownStreamProcessor {
    pub fn new(term_width: u16) -> Self {
        let max_cols = (term_width as usize).saturating_sub(2).max(20);
        Self {
            max_cols,
            col_count: 0,
            in_code_block: false,
            in_bold: false,
            in_italic: false,
            in_inline_code: false,
            in_strikethrough: false,
            is_line_start: true,
            current_line_type: LineType::Normal,
            line_buffer: String::new(),
            pending_word: String::new(),
            in_table: false,
            table_col_widths: Vec::new(),
            is_reasoning: false,
        }
    }

    pub fn write_reasoning_chunk(&mut self, chunk: &str) -> io::Result<()> {
        if !self.is_reasoning {
            if !self.is_line_start {
                println!();
            }
            println!("{}", style("Thought:").dim().italic());
            self.is_reasoning = true;
            self.col_count = 0;
            self.is_line_start = true;
        }
        for ch in chunk.chars() {
            if ch == '\n' {
                println!();
                self.col_count = 0;
            } else {
                print!("{}", style(ch).dim().italic());
                self.col_count += 1;
                if self.col_count >= self.max_cols {
                    println!();
                    self.col_count = 0;
                }
            }
        }
        io::stdout().flush()
    }

    pub fn write_chunk(&mut self, chunk: &str) -> io::Result<()> {
        if self.is_reasoning {
            println!();
            self.col_count = 0;
            self.is_line_start = true;
            self.is_reasoning = false;
        }

        for ch in chunk.chars() {
            if ch == '\n' {
                let trimmed = self.line_buffer.trim();
                let is_special = trimmed == "---"
                    || trimmed == "***"
                    || trimmed == "___"
                    || trimmed.starts_with("```")
                    || trimmed.starts_with("~~~")
                    || (!self.in_code_block && trimmed.starts_with('|'));

                if is_special {
                    self.pending_word.clear();
                } else {
                    self.flush_pending_word()?;
                }

                self.handle_line_break()?;
            } else if ch == ' ' {
                self.flush_pending_word()?;
                self.handle_space()?;
            } else {
                self.pending_word.push(ch);
                self.line_buffer.push(ch);
                if self.pending_word.chars().count() >= self.max_cols {
                    self.flush_pending_word()?;
                }
            }
        }
        Ok(())
    }

    fn is_table_line(&self) -> bool {
        if self.in_code_block {
            return false;
        }
        let trimmed = self.line_buffer.trim();
        trimmed.starts_with('|') || (self.in_table && trimmed.contains('|'))
    }

    fn handle_space(&mut self) -> io::Result<()> {
        if self.is_line_start || self.is_table_line() {
            return Ok(());
        }

        if self.col_count > 0 && self.col_count + 1 <= self.max_cols {
            print!(" ");
            self.col_count += 1;
            io::stdout().flush()?;
        }
        Ok(())
    }

    fn handle_line_break(&mut self) -> io::Result<()> {
        let trimmed = self.line_buffer.trim().to_string();

        // Si veníamos procesando una tabla y esta línea no es de tabla, cerramos la tabla
        if self.in_table && !trimmed.starts_with('|') {
            self.print_table_bottom_border()?;
            self.in_table = false;
            self.table_col_widths.clear();
        }

        // Fila de tabla de Markdown (| ... |)
        if !self.in_code_block && (trimmed.starts_with('|') || (self.in_table && trimmed.contains('|'))) {
            self.handle_table_row(&trimmed)?;
            self.line_buffer.clear();
            self.pending_word.clear();
            self.is_line_start = true;
            self.col_count = 0;
            self.current_line_type = LineType::Normal;
            return io::stdout().flush();
        }

        // Bloques de código (``` / ~~~)
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            if !self.in_code_block {
                self.in_code_block = true;
                let lang = trimmed
                    .trim_start_matches('`')
                    .trim_start_matches('~')
                    .trim();
                let label = if lang.is_empty() { "code" } else { lang };
                let fence_len = self.max_cols.saturating_sub(label.len() + 7).max(4);
                println!(
                    "{}",
                    style(format!("╭── [ {} ] {}", label, "─".repeat(fence_len)))
                        .cyan()
                        .bold()
                );
            } else {
                self.in_code_block = false;
                println!(
                    "{}",
                    style(format!("╰{}", "─".repeat(self.max_cols.saturating_sub(1)))).cyan()
                );
            }
            self.line_buffer.clear();
            self.pending_word.clear();
            self.is_line_start = true;
            self.col_count = 0;
            self.current_line_type = LineType::Normal;
            return io::stdout().flush();
        }

        // Horizontal Rule
        if !self.in_code_block && (trimmed == "---" || trimmed == "***" || trimmed == "___") {
            println!("{}", style("─".repeat(self.max_cols)).dim());
            self.line_buffer.clear();
            self.pending_word.clear();
            self.is_line_start = true;
            self.col_count = 0;
            self.current_line_type = LineType::Normal;
            return io::stdout().flush();
        }

        println!();
        self.line_buffer.clear();
        self.pending_word.clear();
        self.is_line_start = true;
        self.col_count = 0;
        self.current_line_type = LineType::Normal;
        io::stdout().flush()
    }

    fn handle_table_row(&mut self, trimmed_line: &str) -> io::Result<()> {
        let parts: Vec<&str> = trimmed_line.split('|').collect();
        if parts.len() < 2 {
            return Ok(());
        }

        let mut start = 0;
        let mut end = parts.len();
        if start < end && parts[start].trim().is_empty() {
            start += 1;
        }
        if end > start && parts[end - 1].trim().is_empty() {
            end -= 1;
        }

        let raw_cells: Vec<&str> = if start < end {
            parts[start..end].iter().map(|s| s.trim()).collect()
        } else {
            Vec::new()
        };

        if raw_cells.is_empty() {
            return Ok(());
        }

        let num_cols = raw_cells.len();
        let border_overhead = 3 * num_cols + 1;
        let available_text_width = self.max_cols.saturating_sub(border_overhead);
        let max_col_width = (available_text_width / num_cols).max(4);

        // Comprobar si es fila divisoria (--- | --- | ---)
        let is_separator = raw_cells
            .iter()
            .all(|cell| cell.is_empty() || cell.chars().all(|c| c == '-' || c == ':' || c == ' '));

        if is_separator {
            self.in_table = true;
            let mut line = String::from("├");
            let count = if self.table_col_widths.is_empty() {
                num_cols
            } else {
                self.table_col_widths.len()
            };
            for idx in 0..count {
                let width = self.table_col_widths.get(idx).copied().unwrap_or(max_col_width);
                line.push_str(&"─".repeat(width + 2));
                if idx + 1 < count {
                    line.push('┼');
                } else {
                    line.push('┤');
                }
            }
            println!("{}", style(line).cyan().dim());
            return io::stdout().flush();
        }

        // Actualizar anchos de columnas respetando el límite max_col_width
        for (idx, &cell) in raw_cells.iter().enumerate() {
            let vis_len = get_visible_len(cell, false).min(max_col_width);
            if idx >= self.table_col_widths.len() {
                self.table_col_widths.push(vis_len.max(4));
            } else {
                self.table_col_widths[idx] =
                    self.table_col_widths[idx].max(vis_len).min(max_col_width).max(4);
            }
        }

        // Renderizar borde superior si es el inicio de la tabla
        if !self.in_table {
            self.in_table = true;
            let mut top_line = String::from("┌");
            for (idx, &width) in self.table_col_widths.iter().enumerate() {
                top_line.push_str(&"─".repeat(width + 2));
                if idx + 1 < self.table_col_widths.len() {
                    top_line.push('┬');
                } else {
                    top_line.push('┐');
                }
            }
            println!("{}", style(top_line).cyan().dim());
        }

        // Imprimir las celdas de la fila
        print!("{}", style("│").cyan());
        for (idx, &cell) in raw_cells.iter().enumerate() {
            let target_width = self
                .table_col_widths
                .get(idx)
                .copied()
                .unwrap_or(4)
                .min(max_col_width);
            let (display_text, vis_len) = format_cell_text(cell, target_width);
            let pad_len = target_width.saturating_sub(vis_len);

            print!(" ");
            self.print_styled_inline(&display_text)?;
            print!("{} {}", " ".repeat(pad_len), style("│").cyan());
        }
        println!();
        io::stdout().flush()
    }

    fn print_table_bottom_border(&mut self) -> io::Result<()> {
        if self.table_col_widths.is_empty() {
            return Ok(());
        }
        let mut bot_line = String::from("└");
        for (idx, &width) in self.table_col_widths.iter().enumerate() {
            bot_line.push_str(&"─".repeat(width + 2));
            if idx + 1 < self.table_col_widths.len() {
                bot_line.push('┴');
            } else {
                bot_line.push('┘');
            }
        }
        println!("{}", style(bot_line).cyan().dim());
        io::stdout().flush()
    }

    fn flush_pending_word(&mut self) -> io::Result<()> {
        if self.pending_word.is_empty() {
            return Ok(());
        }

        if self.is_table_line() {
            return Ok(());
        }

        let word = std::mem::take(&mut self.pending_word);

        if self.is_line_start {
            if self.in_code_block {
                print!("{}", style("│ ").cyan());
                self.col_count = 2;
                self.is_line_start = false;
            } else {
                // Ocultar caracteres de marcado estructural al inicio de la línea
                if word == "#" {
                    self.current_line_type = LineType::Header(1);
                    self.is_line_start = false;
                    return Ok(()); // Ocultar '#'
                } else if word == "##" {
                    self.current_line_type = LineType::Header(2);
                    self.is_line_start = false;
                    return Ok(()); // Ocultar '##'
                } else if word == "###" {
                    self.current_line_type = LineType::Header(3);
                    self.is_line_start = false;
                    return Ok(()); // Ocultar '###'
                } else if word == "####" || word == "#####" || word == "######" {
                    self.current_line_type = LineType::Header(4);
                    self.is_line_start = false;
                    return Ok(()); // Ocultar '####'
                } else if word == "-" || word == "*" || word == "+" {
                    self.current_line_type = LineType::BulletList;
                    print!("{}", style("• ").cyan().bold());
                    self.col_count = 2;
                    self.is_line_start = false;
                    return io::stdout().flush(); // Ocultar '-' y reemplazar por '• '
                } else if word == ">" {
                    self.current_line_type = LineType::Blockquote;
                    print!("{}", style("│ ").cyan().dim());
                    self.col_count = 2;
                    self.is_line_start = false;
                    return io::stdout().flush(); // Ocultar '>' y reemplazar por '│ '
                } else if word.ends_with('.')
                    && word.len() <= 4
                    && word.chars().all(|c| c.is_ascii_digit() || c == '.')
                {
                    self.current_line_type = LineType::NumberedList;
                    print!("{} ", style(&word).yellow().bold());
                    self.col_count = word.chars().count() + 1;
                    self.is_line_start = false;
                    return io::stdout().flush();
                }
                self.is_line_start = false;
            }
        }

        let visible_len = get_visible_len(&word, self.in_code_block);

        if self.col_count > 0 && self.col_count + 1 + visible_len > self.max_cols {
            println!();
            if self.in_code_block {
                print!("{}", style("│ ").cyan());
                self.col_count = 2;
            } else if self.current_line_type == LineType::Blockquote {
                print!("{}", style("│ ").cyan().dim());
                self.col_count = 2;
            } else if self.current_line_type == LineType::BulletList
                || self.current_line_type == LineType::NumberedList
            {
                print!("  ");
                self.col_count = 2;
            } else {
                self.col_count = 0;
            }
        } else if self.col_count > 0 {
            print!(" ");
            self.col_count += 1;
        }

        if self.in_code_block {
            print!("{}", style(&word).cyan());
            self.col_count += word.chars().count();
        } else {
            self.print_styled_inline(&word)?;
        }

        io::stdout().flush()
    }

    fn print_styled_inline(&mut self, word: &str) -> io::Result<()> {
        let mut chars = word.chars().peekable();
        let mut segment = String::new();

        while let Some(&ch) = chars.peek() {
            if ch == '`' {
                chars.next();
                self.flush_segment(&segment)?;
                segment.clear();
                self.in_inline_code = !self.in_inline_code;
            } else if self.in_inline_code {
                segment.push(ch);
                chars.next();
            } else if ch == '*' || ch == '_' {
                chars.next();
                let is_double = chars.peek() == Some(&ch);
                if is_double {
                    chars.next();
                    self.flush_segment(&segment)?;
                    segment.clear();
                    self.in_bold = !self.in_bold;
                } else {
                    self.flush_segment(&segment)?;
                    segment.clear();
                    self.in_italic = !self.in_italic;
                }
            } else if ch == '~' {
                chars.next();
                if chars.peek() == Some(&'~') {
                    chars.next();
                    self.flush_segment(&segment)?;
                    segment.clear();
                    self.in_strikethrough = !self.in_strikethrough;
                } else {
                    segment.push('~');
                    chars.next();
                }
            } else {
                segment.push(ch);
                chars.next();
            }
        }

        self.flush_segment(&segment)
    }

    fn flush_segment(&mut self, segment: &str) -> io::Result<()> {
        if segment.is_empty() {
            return Ok(());
        }

        let count = segment.chars().count();
        self.col_count += count;

        if self.in_inline_code {
            print!("{}", style(segment).yellow().bold());
        } else {
            match self.current_line_type {
                LineType::Header(1) => {
                    print!("{}", style(segment).magenta().bold());
                }
                LineType::Header(2) => {
                    print!("{}", style(segment).cyan().bold());
                }
                LineType::Header(3) => {
                    print!("{}", style(segment).yellow().bold());
                }
                LineType::Header(4) => {
                    print!("{}", style(segment).blue().bold());
                }
                LineType::Blockquote => {
                    print!("{}", style(segment).cyan().dim().italic());
                }
                _ => {
                    let mut styled = style(segment);
                    if self.in_bold {
                        styled = styled.bold();
                    }
                    if self.in_italic {
                        styled = styled.italic();
                    }
                    if self.in_strikethrough {
                        styled = styled.strikethrough();
                    }
                    print!("{}", styled);
                }
            }
        }
        io::stdout().flush()
    }

    pub fn flush_final(&mut self) -> io::Result<()> {
        self.flush_pending_word()?;
        if self.in_table {
            self.print_table_bottom_border()?;
            self.in_table = false;
            self.table_col_widths.clear();
        }
        if self.in_code_block {
            self.in_code_block = false;
            println!(
                "{}",
                style(format!("╰{}", "─".repeat(self.max_cols.saturating_sub(1)))).cyan()
            );
        }
        println!();
        io::stdout().flush()
    }
}

pub fn get_visible_len(word: &str, in_code_block: bool) -> usize {
    if in_code_block {
        word.chars().count()
    } else {
        word.replace("**", "")
            .replace("__", "")
            .replace("~~", "")
            .replace('*', "")
            .replace('_', "")
            .replace('`', "")
            .chars()
            .count()
    }
}

pub fn format_cell_text(cell: &str, max_cell_width: usize) -> (String, usize) {
    let vis_len = get_visible_len(cell, false);
    if vis_len <= max_cell_width {
        return (cell.to_string(), vis_len);
    }

    if max_cell_width <= 1 {
        return ("…".to_string(), 1);
    }

    let limit = max_cell_width.saturating_sub(1);
    let mut current_vis = 0;
    let mut truncated = String::new();

    for ch in cell.chars() {
        if ch == '*' || ch == '_' || ch == '`' || ch == '~' {
            truncated.push(ch);
        } else {
            if current_vis < limit {
                truncated.push(ch);
                current_vis += 1;
            } else {
                break;
            }
        }
    }
    truncated.push('…');
    let final_vis = get_visible_len(&truncated, false);
    (truncated, final_vis)
}
