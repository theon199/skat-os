use ropey::{Rope, RopeSlice};
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

impl std::str::FromStr for TextBuffer {
    type Err = std::convert::Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self::from_str(s))
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

    #[allow(clippy::should_implement_trait)]
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
            } else if last_char == '\r' {
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

    pub fn push_undo_state(&mut self) {
        let current_text = self.rope.to_string();
        if self.undo_stack.last().is_some_and(|s| s == &current_text) {
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

    pub fn char_index_at_cursor(&self) -> usize {
        let count = self.line_count();
        if count == 0 {
            return 0;
        }
        let line = min(self.cursor_line, count.saturating_sub(1));
        let line_start = self.rope.line_to_char(line);
        let line_len = self.line_len(line);
        let col = min(self.cursor_col, line_len);
        min(line_start + col, self.rope.len_chars())
    }

    pub fn char_to_line_col(&self, char_idx: usize) -> (usize, usize) {
        let total_chars = self.rope.len_chars();
        let idx = min(char_idx, total_chars);
        let line = self.rope.char_to_line(idx);
        let line_start = self.rope.line_to_char(line);
        let col = idx.saturating_sub(line_start);
        (line, col)
    }

    pub fn line_col_to_char(&self, line: usize, col: usize) -> usize {
        if line >= self.line_count() {
            return self.rope.len_chars();
        }
        let line_start = self.rope.line_to_char(line);
        let line_len = self.line_len(line);
        min(line_start + min(col, line_len), self.rope.len_chars())
    }

    pub fn apply_remote_insert(&mut self, pos: usize, c: char) {
        let total = self.rope.len_chars();
        let idx = min(pos, total);
        let local_cursor_idx = self.char_index_at_cursor();

        self.rope.insert_char(idx, c);
        self.is_dirty = true;

        // If remote insertion happened before or at our local cursor, shift local cursor
        if idx <= local_cursor_idx {
            let (new_line, new_col) = self.char_to_line_col(local_cursor_idx + 1);
            self.cursor_line = new_line;
            self.cursor_col = new_col;
        }
    }

    pub fn apply_remote_delete(&mut self, pos: usize) {
        let total = self.rope.len_chars();
        if pos < total {
            let local_cursor_idx = self.char_index_at_cursor();
            self.rope.remove(pos..pos + 1);
            self.is_dirty = true;

            // If remote deletion happened before our local cursor, shift local cursor back
            if pos < local_cursor_idx {
                let (new_line, new_col) = self.char_to_line_col(local_cursor_idx.saturating_sub(1));
                self.cursor_line = new_line;
                self.cursor_col = new_col;
            } else {
                let max_line = self.line_count().saturating_sub(1);
                self.cursor_line = min(self.cursor_line, max_line);
                self.cursor_col = min(self.cursor_col, self.line_len(self.cursor_line));
            }
        }
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
        let line_chars: Vec<char> = current_line.chars().collect();
        let indent_len = line_chars.iter().take_while(|c| **c == ' ' || **c == '\t').count();
        let indent_to_copy = if self.cursor_col >= indent_len {
            &line_chars[..indent_len]
        } else {
            &line_chars[..self.cursor_col]
        };
        let indent: String = indent_to_copy.iter().collect();

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
            if idx > 0 {
                self.rope.remove(idx - 1..idx);
                self.cursor_col -= 1;
                self.is_dirty = true;
            }
        } else if self.cursor_line > 0 {
            self.push_undo_state();
            let prev_line = self.cursor_line - 1;
            let prev_len = self.line_len(prev_line);
            let idx = self.char_index_at_cursor();

            // The newline to delete is right before idx
            if idx > 0 {
                if idx >= 2 && self.rope.char(idx - 1) == '\n' && self.rope.char(idx - 2) == '\r' {
                    self.rope.remove(idx - 2..idx);
                } else {
                    self.rope.remove(idx - 1..idx);
                }
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
            if idx < self.rope.len_chars() {
                self.rope.remove(idx..idx + 1);
                self.is_dirty = true;
            }
        } else if self.cursor_line + 1 < self.line_count() {
            self.push_undo_state();
            let idx = self.char_index_at_cursor();
            if idx < self.rope.len_chars() {
                if idx + 1 < self.rope.len_chars() && self.rope.char(idx) == '\r' && self.rope.char(idx + 1) == '\n' {
                    self.rope.remove(idx..idx + 2);
                } else {
                    self.rope.remove(idx..idx + 1);
                }
                self.is_dirty = true;
            }
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
        let mut new_col = min(self.cursor_col, chars.len());

        // Skip spaces backwards
        while new_col > 0 && chars[new_col - 1].is_whitespace() {
            new_col -= 1;
        }
        // Delete alphanumeric or symbol group
        while new_col > 0 && !chars[new_col - 1].is_whitespace() {
            new_col -= 1;
        }

        let line_start = self.rope.line_to_char(self.cursor_line);
        let char_limit = self.rope.len_chars();
        let from_idx = min(line_start + new_col, char_limit);
        let to_idx = min(line_start + self.cursor_col, char_limit);
        if from_idx < to_idx {
            self.rope.remove(from_idx..to_idx);
        }
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
        let mut col = min(self.cursor_col, chars.len());

        while col > 0 && chars[col - 1].is_whitespace() {
            col -= 1;
        }
        while col > 0 && !chars[col - 1].is_whitespace() {
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

    /// Parses a task item prefix like `- [ ]`, `* [ ]`, `+ [ ]`, `1. [ ]`, `- [x]`, `* [X]`
    pub fn parse_task_item(line: &str) -> Option<TaskItemInfo> {
        let chars: Vec<char> = line.chars().collect();
        let len = chars.len();
        let mut i = 0;

        // Leading whitespace
        while i < len && (chars[i] == ' ' || chars[i] == '\t') {
            i += 1;
        }

        if i >= len {
            return None;
        }

        let marker_col = i;
        if chars[i] == '-' || chars[i] == '*' || chars[i] == '+' {
            i += 1;
        } else if chars[i].is_ascii_digit() {
            while i < len && chars[i].is_ascii_digit() {
                i += 1;
            }
            if i < len && (chars[i] == '.' || chars[i] == ')') {
                i += 1;
            } else {
                return None;
            }
        } else {
            return None;
        }

        // Space(s) or tab(s) between marker and '['
        if i >= len || (chars[i] != ' ' && chars[i] != '\t') {
            return None;
        }
        while i < len && (chars[i] == ' ' || chars[i] == '\t') {
            i += 1;
        }

        // Open bracket '['
        if i >= len || chars[i] != '[' {
            return None;
        }
        let bracket_open_col = i;
        i += 1;

        // Check character: ' ' or 'x' or 'X'
        if i >= len {
            return None;
        }
        let check_char = chars[i];
        let check_char_col = i;
        let is_checked = match check_char {
            ' ' => false,
            'x' | 'X' => true,
            _ => return None,
        };
        i += 1;

        // Close bracket ']'
        if i >= len || chars[i] != ']' {
            return None;
        }
        let bracket_close_col = i;
        i += 1;

        // Optional space or tab after ']'
        let prefix_end_col = if i < len && (chars[i] == ' ' || chars[i] == '\t') {
            i + 1
        } else {
            i
        };

        Some(TaskItemInfo {
            is_checked,
            marker_col,
            bracket_open_col,
            check_char_col,
            bracket_close_col,
            prefix_end_col,
        })
    }

    /// Fast zero-allocation task check directly on a RopeSlice
    fn is_rope_slice_task_checked(slice: RopeSlice) -> Option<bool> {
        let mut chars = slice.chars();
        let mut c = chars.next()?;

        // Leading whitespace
        while c == ' ' || c == '\t' {
            c = chars.next()?;
        }

        // Bullet marker: -, *, +, or digits followed by . or )
        if c == '-' || c == '*' || c == '+' {
            c = chars.next()?;
        } else if c.is_ascii_digit() {
            c = chars.next()?;
            while c.is_ascii_digit() {
                c = chars.next()?;
            }
            if c == '.' || c == ')' {
                c = chars.next()?;
            } else {
                return None;
            }
        } else {
            return None;
        }

        // Whitespace
        if c != ' ' && c != '\t' {
            return None;
        }
        while c == ' ' || c == '\t' {
            c = chars.next()?;
        }

        // Open bracket '['
        if c != '[' {
            return None;
        }

        // Check character
        c = chars.next()?;
        let is_checked = match c {
            ' ' => false,
            'x' | 'X' => true,
            _ => return None,
        };

        // Close bracket ']'
        c = chars.next()?;
        if c != ']' {
            return None;
        }

        Some(is_checked)
    }

    /// Returns task information for the specified line if it is a task item
    pub fn get_task_info(&self, line_idx: usize) -> Option<TaskItemInfo> {
        if line_idx >= self.line_count() {
            return None;
        }
        let line = self.line_to_string(line_idx);
        Self::parse_task_item(&line)
    }

    /// Checks if cursor is currently on a task item checkbox or task line
    pub fn is_cursor_on_task_item(&self) -> bool {
        if let Some(info) = self.get_task_info(self.cursor_line) {
            let line_len = self.line_len(self.cursor_line);
            self.cursor_col <= info.prefix_end_col || line_len <= info.prefix_end_col
        } else {
            false
        }
    }

    /// Toggles the task item on the current cursor line
    pub fn toggle_task_at_cursor(&mut self) -> Option<bool> {
        self.toggle_task_at_line(self.cursor_line)
    }

    /// Toggles the task item on the specified line
    pub fn toggle_task_at_line(&mut self, line: usize) -> Option<bool> {
        if let Some(info) = self.get_task_info(line) {
            let new_state = !info.is_checked;
            self.set_task_checked(line, new_state);
            Some(new_state)
        } else {
            None
        }
    }

    /// Sets the task item checked status on the specified line
    pub fn set_task_checked(&mut self, line: usize, checked: bool) -> bool {
        if let Some(info) = self.get_task_info(line) {
            if info.is_checked == checked {
                return false;
            }
            self.push_undo_state();
            let line_start = self.rope.line_to_char(line);
            let char_pos = line_start + info.check_char_col;
            if char_pos < self.rope.len_chars() {
                self.rope.remove(char_pos..char_pos + 1);
                self.rope.insert_char(char_pos, if checked { 'x' } else { ' ' });
                self.is_dirty = true;
                return true;
            }
        }
        false
    }

    /// Returns (completed_tasks, total_tasks) if task items exist in the buffer.
    /// Optimized with zero allocations per line for high-frequency status bar rendering.
    pub fn task_stats(&self) -> Option<(usize, usize)> {
        let mut total = 0;
        let mut completed = 0;
        for line in self.rope.lines() {
            if let Some(is_checked) = Self::is_rope_slice_task_checked(line) {
                total += 1;
                if is_checked {
                    completed += 1;
                }
            }
        }
        if total > 0 {
            Some((completed, total))
        } else {
            None
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskItemInfo {
    pub is_checked: bool,
    pub marker_col: usize,
    pub bracket_open_col: usize,
    pub check_char_col: usize,
    pub bracket_close_col: usize,
    pub prefix_end_col: usize,
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_task_parsing_edge_cases() {
        // Standard dashes
        let t1 = TextBuffer::parse_task_item("- [ ] Unchecked").unwrap();
        assert!(!t1.is_checked);
        assert_eq!(t1.marker_col, 0);

        let t2 = TextBuffer::parse_task_item("- [x] Checked").unwrap();
        assert!(t2.is_checked);

        // Asterisks
        let t3 = TextBuffer::parse_task_item("* [X] Uppercase X").unwrap();
        assert!(t3.is_checked);

        // Plus bullet
        let t4 = TextBuffer::parse_task_item("+ [ ] Plus bullet").unwrap();
        assert!(!t4.is_checked);

        // Ordered lists
        let t5 = TextBuffer::parse_task_item("1. [ ] First ordered").unwrap();
        assert!(!t5.is_checked);

        let t6 = TextBuffer::parse_task_item("12) [x] Paren ordered").unwrap();
        assert!(t6.is_checked);

        // Indentation
        let t7 = TextBuffer::parse_task_item("    - [ ] Indented").unwrap();
        assert_eq!(t7.marker_col, 4);

        // Empty task line
        let t8 = TextBuffer::parse_task_item("- [ ]").unwrap();
        assert!(!t8.is_checked);

        // Non-tasks
        assert!(TextBuffer::parse_task_item("- Normal bullet").is_none());
        assert!(TextBuffer::parse_task_item("[ ] No bullet").is_none());
        assert!(TextBuffer::parse_task_item("1. Just a number").is_none());
        assert!(TextBuffer::parse_task_item("- [?] Invalid check").is_none());
    }

    #[test]
    fn test_task_stats_zero_allocation() {
        let text = "# Tasks\n- [ ] Task A\n- [x] Task B\n* [x] Task C\n+ [ ] Task D\n1. [x] Task E\nNot a task\n";
        let buf = TextBuffer::from_str(text);
        assert_eq!(buf.task_stats(), Some((3, 5)));
    }

    #[test]
    fn test_line_len_crlf_and_cr() {
        let buf_crlf = TextBuffer::from_str("line1\r\nline2\r\n");
        assert_eq!(buf_crlf.line_len(0), 5);
        assert_eq!(buf_crlf.line_to_string(0), "line1");

        let buf_cr = TextBuffer::from_str("mac1\rmac2\r");
        assert_eq!(buf_cr.line_len(0), 4);
        assert_eq!(buf_cr.line_to_string(0), "mac1");
    }

    #[test]
    fn test_char_index_boundary_safety() {
        let mut buf = TextBuffer::from_str("hello\nworld");
        buf.cursor_line = 999;
        buf.cursor_col = 999;
        // Must not panic!
        let idx = buf.char_index_at_cursor();
        assert_eq!(idx, buf.rope.len_chars());
    }

    #[test]
    fn test_delete_word_bounds_safety() {
        let mut buf = TextBuffer::from_str("alpha beta gamma");
        buf.cursor_col = 50; // past line length
        buf.delete_word(); // must not panic
        assert_eq!(buf.cursor_col, 11);
    }

    #[test]
    fn test_insert_newline_indentation_cases() {
        // 1. Enter at end of indented line -> preserves indent
        let mut buf1 = TextBuffer::from_str("    hello");
        buf1.cursor_col = 9;
        buf1.insert_newline();
        assert_eq!(buf1.line_to_string(0), "    hello");
        assert_eq!(buf1.line_to_string(1), "    ");
        assert_eq!(buf1.cursor_col, 4);

        // 2. Enter at col 0 -> does not duplicate indent
        let mut buf2 = TextBuffer::from_str("    hello");
        buf2.cursor_col = 0;
        buf2.insert_newline();
        assert_eq!(buf2.line_to_string(0), "");
        assert_eq!(buf2.line_to_string(1), "    hello");
        assert_eq!(buf2.cursor_col, 0);
    }
}
