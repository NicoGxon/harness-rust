pub struct InputState {
    pub text: String,
    pub cursor: usize, // Índice de carácter (0..=char_count)
}

impl InputState {
    pub fn new() -> Self {
        Self {
            text: String::new(),
            cursor: 0,
        }
    }

    pub fn char_count(&self) -> usize {
        self.text.chars().count()
    }

    pub fn insert_char(&mut self, c: char) {
        let byte_idx = self
            .text
            .char_indices()
            .map(|(i, _)| i)
            .nth(self.cursor)
            .unwrap_or(self.text.len());
        self.text.insert(byte_idx, c);
        self.cursor += 1;
    }

    pub fn delete_prev_char(&mut self) {
        if self.cursor > 0 {
            let prev_idx = self.cursor - 1;
            if let Some((byte_idx, _)) = self.text.char_indices().nth(prev_idx) {
                self.text.remove(byte_idx);
                self.cursor -= 1;
            }
        }
    }

    pub fn delete_next_char(&mut self) {
        if self.cursor < self.char_count() {
            if let Some((byte_idx, _)) = self.text.char_indices().nth(self.cursor) {
                self.text.remove(byte_idx);
            }
        }
    }

    pub fn move_left(&mut self) {
        self.cursor = self.cursor.saturating_sub(1);
    }

    pub fn move_right(&mut self) {
        let count = self.char_count();
        if self.cursor < count {
            self.cursor += 1;
        }
    }

    pub fn move_home(&mut self) {
        self.cursor = 0;
    }

    pub fn move_end(&mut self) {
        self.cursor = self.char_count();
    }

    pub fn move_word_left(&mut self) {
        if self.cursor == 0 {
            return;
        }
        let chars: Vec<char> = self.text.chars().collect();
        let mut idx = self.cursor.saturating_sub(1);
        while idx > 0 && chars[idx].is_whitespace() {
            idx -= 1;
        }
        while idx > 0 && !chars[idx - 1].is_whitespace() {
            idx -= 1;
        }
        self.cursor = idx;
    }

    pub fn move_word_right(&mut self) {
        let count = self.char_count();
        if self.cursor >= count {
            return;
        }
        let chars: Vec<char> = self.text.chars().collect();
        let mut idx = self.cursor;
        while idx < count && !chars[idx].is_whitespace() {
            idx += 1;
        }
        while idx < count && chars[idx].is_whitespace() {
            idx += 1;
        }
        self.cursor = idx;
    }

    /// Formatea las líneas con ajuste por caracteres exacto y calcula las coordenadas del cursor.
    pub fn format_display_lines(&self, term_width: u16) -> (String, u16, u16, u16) {
        let max_cols = (term_width as usize).saturating_sub(2).max(10);

        let mut lines: Vec<String> = Vec::new();
        let mut current_line = String::from("❯ ");
        let mut current_cols = 2usize;

        let mut target_x = 2u16;
        let mut target_y = 0u16;

        let chars: Vec<char> = self.text.chars().collect();

        for (i, &ch) in chars.iter().enumerate() {
            if i == self.cursor {
                target_x = current_cols as u16;
                target_y = lines.len() as u16;
            }

            if ch == '\n' {
                lines.push(current_line);
                current_line = String::new();
                current_cols = 0;
            } else {
                if current_cols >= max_cols {
                    lines.push(current_line);
                    current_line = String::new();
                    current_cols = 0;
                }
                current_line.push(ch);
                current_cols += 1;
            }
        }

        if self.cursor >= chars.len() {
            target_x = current_cols as u16;
            target_y = lines.len() as u16;
        }

        lines.push(current_line);
        let total_lines = lines.len() as u16;
        let display_text = lines.join("\n");

        (display_text, target_x, target_y, total_lines)
    }

    pub fn clear(&mut self) {
        self.text.clear();
        self.cursor = 0;
    }
}
