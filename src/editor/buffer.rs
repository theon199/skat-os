use ropey::Rope;
use std::cmp::min;

#[derive(Clone, Debug)]
pub struct TextBuffer {
    pub rope: Rope,
    pub cursor_line: usize,
    pub cursor_col: usize,
    pub scroll_top: usize,
    pub scroll_left: usize,
    pub is_dirty: bool,
    undo_stack: Vec<String>,
    redo_stack: Vec<String>,
}

impl Default for TextBuffer {
    fn default() -> Self {
        Self::new()
    }
}

impl TextBuffer {
    pub fn new() -> Self {
        Self {
            rope: Rope::new(),
            cursor_line: 0,
            cursor_col: 0,
            scroll_top: 0,
            scroll_left: 0,
            is_dirty: false,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
        }
    }

    pub fn from_str(text: &str) -> Self {
        Self {
            rope: Rope::from_str(text),
            cursor_line: 0,
            cursor_col: 0,
            scroll_top: 0,
            scroll_left: 0,
            is_dirty: false,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
        }
    }

    pub fn line_count(&self) -> usize {
        self.rope.len_lines()
    }

    pub fn line_len(&self, line_idx: usize) -> usize {
        if line_idx >= self.line_count() {
            return 0;
        }
        let line = self.rope.line(line_idx);
        let len = line.len_chars();
        // Exclude newline character(s) from character count
        if len > 0 {
            let last_char = line.char(len - 1);
            if last_char == '\n' {
                if len > 1 && line.char(len - 2) == '\r' {
                    return len - 2;
                }
                return len - 1;
            }
        }
        len
    }

    pub fn line_to_string(&self, line_idx: usize) -> String {
        if line_idx >= self.line_count() {
            return String::new();
        }
        let line = self.rope.line(line_idx);
        let mut s = line.to_string();
        if s.ends_with("\r\n") {
            s.truncate(s.len() - 2);
        } else if s.ends_with('\n') || s.ends_with('\r') {
            s.truncate(s.len() - 1);
        }
        s
    }

    pub fn get_text(&self) -> String {
        self.rope.to_string()
    }

    #[allow(dead_code)]
    pub fn set_text(&mut self, text: &str) {
        self.push_undo_state();
        self.rope = Rope::from_str(text);
        self.cursor_line = min(self.cursor_line, self.line_count().saturating_sub(1));
        self.cursor_col = min(self.cursor_col, self.line_len(self.cursor_line));
        self.is_dirty = true;
    }

    fn push_undo_state(&mut self) {
        let current_text = self.rope.to_string();
        if self.undo_stack.last().map(|s| s == &current_text).unwrap_or(false) {
            return;
        }
        self.undo_stack.push(current_text);
        if self.undo_stack.len() > 100 {
            self.undo_stack.remove(0);
        }
        self.redo_stack.clear();
    }

    pub fn undo(&mut self) -> bool {
        if let Some(prev) = self.undo_stack.pop() {
            self.redo_stack.push(self.rope.to_string());
            self.rope = Rope::from_str(&prev);
            self.cursor_line = min(self.cursor_line, self.line_count().saturating_sub(1));
            self.cursor_col = min(self.cursor_col, self.line_len(self.cursor_line));
            self.is_dirty = true;
            return true;
        }
        false
    }

    pub fn redo(&mut self) -> bool {
        if let Some(next) = self.redo_stack.pop() {
            self.undo_stack.push(self.rope.to_string());
            self.rope = Rope::from_str(&next);
            self.cursor_line = min(self.cursor_line, self.line_count().saturating_sub(1));
            self.cursor_col = min(self.cursor_col, self.line_len(self.cursor_line));
            self.is_dirty = true;
            return true;
        }
        false
    }

    fn char_index_at_cursor(&self) -> usize {
        let line_start = self.rope.line_to_char(self.cursor_line);
        let line_len = self.line_len(self.cursor_line);
        let col = min(self.cursor_col, line_len);
        line_start + col
    }

    pub fn insert_char(&mut self, c: char) {
        self.push_undo_state();
        let idx = self.char_index_at_cursor();
        self.rope.insert_char(idx, c);
        self.cursor_col += 1;
        self.is_dirty = true;
    }

    pub fn insert_str(&mut self, s: &str) {
        self.push_undo_state();
        let idx = self.char_index_at_cursor();
        self.rope.insert(idx, s);
        self.cursor_col += s.chars().count();
        self.is_dirty = true;
    }

    pub fn insert_newline(&mut self) {
        self.push_undo_state();
        // Detect leading indentation from current line to preserve it
        let current_line = self.line_to_string(self.cursor_line);
        let indent = current_line
            .chars()
            .take_while(|c| *c == ' ' || *c == '\t')
            .collect::<String>();

        let idx = self.char_index_at_cursor();
        self.rope.insert_char(idx, '\n');

        self.cursor_line += 1;
        self.cursor_col = 0;

        if !indent.is_empty() {
            let next_idx = self.rope.line_to_char(self.cursor_line);
            self.rope.insert(next_idx, &indent);
            self.cursor_col = indent.chars().count();
        }

        self.is_dirty = true;
    }

    pub fn backspace(&mut self) {
        if self.cursor_col > 0 {
            self.push_undo_state();
            let idx = self.char_index_at_cursor();
            self.rope.remove(idx - 1..idx);
            self.cursor_col -= 1;
            self.is_dirty = true;
        } else if self.cursor_line > 0 {
            self.push_undo_state();
            let prev_line = self.cursor_line - 1;
            let prev_len = self.line_len(prev_line);
            let idx = self.char_index_at_cursor();

            // The newline to delete is right before idx
            if idx > 0 {
                self.rope.remove(idx - 1..idx);
                self.cursor_line = prev_line;
                self.cursor_col = prev_len;
                self.is_dirty = true;
            }
        }
    }

    pub fn delete(&mut self) {
        let line_len = self.line_len(self.cursor_line);
        if self.cursor_col < line_len {
            self.push_undo_state();
            let idx = self.char_index_at_cursor();
            self.rope.remove(idx..idx + 1);
            self.is_dirty = true;
        } else if self.cursor_line + 1 < self.line_count() {
            self.push_undo_state();
            let idx = self.char_index_at_cursor();
            self.rope.remove(idx..idx + 1);
            self.is_dirty = true;
        }
    }

    pub fn delete_word(&mut self) {
        if self.cursor_col == 0 {
            self.backspace();
            return;
        }

        self.push_undo_state();
        let line = self.line_to_string(self.cursor_line);
        let chars: Vec<char> = line.chars().collect();
        let mut new_col = self.cursor_col;

        // Skip spaces backwards
        while new_col > 0 && chars[new_col - 1].is_whitespace() {
            new_col -= 1;
        }
        // Delete alphanumeric or symbol group
        while new_col > 0 && !chars[new_col - 1].is_whitespace() {
            new_col -= 1;
        }

        let line_start = self.rope.line_to_char(self.cursor_line);
        self.rope.remove((line_start + new_col)..(line_start + self.cursor_col));
        self.cursor_col = new_col;
        self.is_dirty = true;
    }

    pub fn move_left(&mut self) {
        if self.cursor_col > 0 {
            self.cursor_col -= 1;
        } else if self.cursor_line > 0 {
            self.cursor_line -= 1;
            self.cursor_col = self.line_len(self.cursor_line);
        }
    }

    pub fn move_right(&mut self) {
        let len = self.line_len(self.cursor_line);
        if self.cursor_col < len {
            self.cursor_col += 1;
        } else if self.cursor_line + 1 < self.line_count() {
            self.cursor_line += 1;
            self.cursor_col = 0;
        }
    }

    pub fn move_up(&mut self) {
        if self.cursor_line > 0 {
            self.cursor_line -= 1;
            self.cursor_col = min(self.cursor_col, self.line_len(self.cursor_line));
        }
    }

    pub fn move_down(&mut self) {
        if self.cursor_line + 1 < self.line_count() {
            self.cursor_line += 1;
            self.cursor_col = min(self.cursor_col, self.line_len(self.cursor_line));
        }
    }

    pub fn move_home(&mut self) {
        self.cursor_col = 0;
    }

    pub fn move_end(&mut self) {
        self.cursor_col = self.line_len(self.cursor_line);
    }

    pub fn page_up(&mut self, height: usize) {
        self.cursor_line = self.cursor_line.saturating_sub(height);
        self.cursor_col = min(self.cursor_col, self.line_len(self.cursor_line));
    }

    pub fn page_down(&mut self, height: usize) {
        self.cursor_line = min(self.cursor_line + height, self.line_count().saturating_sub(1));
        self.cursor_col = min(self.cursor_col, self.line_len(self.cursor_line));
    }

    pub fn move_word_left(&mut self) {
        if self.cursor_col == 0 {
            self.move_left();
            return;
        }
        let line = self.line_to_string(self.cursor_line);
        let chars: Vec<char> = line.chars().collect();
        let mut col = self.cursor_col;

        while col > 0 && col <= chars.len() && chars[col - 1].is_whitespace() {
            col -= 1;
        }
        while col > 0 && col <= chars.len() && !chars[col - 1].is_whitespace() {
            col -= 1;
        }
        self.cursor_col = col;
    }

    pub fn move_word_right(&mut self) {
        let line = self.line_to_string(self.cursor_line);
        let chars: Vec<char> = line.chars().collect();
        let mut col = self.cursor_col;

        while col < chars.len() && !chars[col].is_whitespace() {
            col += 1;
        }
        while col < chars.len() && chars[col].is_whitespace() {
            col += 1;
        }
        if col >= chars.len() && self.cursor_line + 1 < self.line_count() {
            self.cursor_line += 1;
            self.cursor_col = 0;
        } else {
            self.cursor_col = min(col, chars.len());
        }
    }

    pub fn adjust_scroll(&mut self, viewport_width: usize, viewport_height: usize) {
        if viewport_height == 0 || viewport_width == 0 {
            return;
        }

        // Vertical scroll: keep cursor within [scroll_top, scroll_top + viewport_height)
        if self.cursor_line < self.scroll_top {
            self.scroll_top = self.cursor_line;
        } else if self.cursor_line >= self.scroll_top + viewport_height {
            self.scroll_top = self.cursor_line - viewport_height + 1;
        }

        // Horizontal scroll
        if self.cursor_col < self.scroll_left {
            self.scroll_left = self.cursor_col;
        } else if self.cursor_col >= self.scroll_left + viewport_width {
            self.scroll_left = self.cursor_col - viewport_width + 1;
        }
    }
}
