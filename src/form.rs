use regex::Regex;
use ropey::Rope;
use std::collections::HashMap;

use crate::editor::TextBuffer;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FormParamType {
    Input,
    Select { options: Vec<String> },
    Toggle,
    Secret,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormParam {
    pub param_type: FormParamType,
    pub name: String,
    pub value: String,
    pub raw_text: String,
    pub line: usize,
    pub start_col: usize,
    pub end_col: usize,
}

impl FormParam {
    /// Formats the parameter back into its markdown widget representation
    pub fn to_raw_string(&self) -> String {
        match &self.param_type {
            FormParamType::Input => {
                format!("{{{{param:input:{}={}}}}}", self.name, self.value)
            }
            FormParamType::Secret => {
                if self.value.is_empty() {
                    format!("{{{{param:secret:{}}}}}", self.name)
                } else {
                    format!("{{{{param:secret:{}={}}}}}", self.name, self.value)
                }
            }
            FormParamType::Toggle => {
                format!("{{{{param:toggle:{}={}}}}}", self.name, self.value)
            }
            FormParamType::Select { options } => {
                format!("{{{{param:select:{}={}}}}}", self.name, options.join(","))
            }
        }
    }

    /// Returns masked representation for secret parameters, or raw value otherwise
    pub fn display_value(&self) -> String {
        match &self.param_type {
            FormParamType::Secret => {
                if self.value.is_empty() {
                    "••••••••".to_string()
                } else {
                    "•".repeat(self.value.len())
                }
            }
            _ => self.value.clone(),
        }
    }
}

pub struct FormParser;

impl FormParser {
    /// Regex pattern matching {{param:type:NAME=value}} or {{param:secret:NAME}}
    fn regex() -> Regex {
        Regex::new(r"\{\{param:(input|select|toggle|secret):([a-zA-Z0-9_-]+)(?:=([^}]+))?\}\}").unwrap()
    }

    /// Parses all parameters on a single line string
    pub fn parse_line(line_idx: usize, line: &str) -> Vec<FormParam> {
        let mut params = Vec::new();
        let re = Self::regex();

        for cap in re.captures_iter(line) {
            let full_match = cap.get(0).unwrap();
            let raw_text = full_match.as_str().to_string();
            let start_col = full_match.start();
            let end_col = full_match.end();

            let p_type_str = &cap[1];
            let name = cap[2].to_string();
            let val_str = cap.get(3).map(|m| m.as_str()).unwrap_or("").to_string();

            match p_type_str {
                "input" => {
                    params.push(FormParam {
                        param_type: FormParamType::Input,
                        name,
                        value: val_str,
                        raw_text,
                        line: line_idx,
                        start_col,
                        end_col,
                    });
                }
                "secret" => {
                    params.push(FormParam {
                        param_type: FormParamType::Secret,
                        name,
                        value: val_str,
                        raw_text,
                        line: line_idx,
                        start_col,
                        end_col,
                    });
                }
                "toggle" => {
                    let clean_val = if val_str.eq_ignore_ascii_case("true") {
                        "true".to_string()
                    } else {
                        "false".to_string()
                    };
                    params.push(FormParam {
                        param_type: FormParamType::Toggle,
                        name,
                        value: clean_val,
                        raw_text,
                        line: line_idx,
                        start_col,
                        end_col,
                    });
                }
                "select" => {
                    let options: Vec<String> = val_str
                        .split(',')
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty())
                        .collect();
                    let active_value = options.first().cloned().unwrap_or_default();
                    params.push(FormParam {
                        param_type: FormParamType::Select { options },
                        name,
                        value: active_value,
                        raw_text,
                        line: line_idx,
                        start_col,
                        end_col,
                    });
                }
                _ => {}
            }
        }

        params
    }

    /// Finds all parameters across the entire TextBuffer in document order
    pub fn find_all(buffer: &TextBuffer) -> Vec<FormParam> {
        let mut all = Vec::new();
        for i in 0..buffer.line_count() {
            let line = buffer.line_to_string(i);
            all.extend(Self::parse_line(i, &line));
        }
        all
    }

    /// Finds if cursor (line, col) is directly on a parameter widget
    pub fn find_at_cursor(buffer: &TextBuffer, line: usize, col: usize) -> Option<FormParam> {
        if line >= buffer.line_count() {
            return None;
        }
        let line_str = buffer.line_to_string(line);
        Self::parse_line(line, &line_str)
            .into_iter()
            .find(|p| col >= p.start_col && col <= p.end_col)
    }

    /// Finds the next parameter after (line, col), wrapping around to document start
    pub fn find_next(buffer: &TextBuffer, line: usize, col: usize) -> Option<FormParam> {
        let all = Self::find_all(buffer);
        if all.is_empty() {
            return None;
        }

        if let Some(curr) = Self::find_at_cursor(buffer, line, col) {
            if let Some(pos) = all.iter().position(|p| p.name == curr.name && p.line == curr.line && p.start_col == curr.start_col) {
                if pos + 1 < all.len() {
                    return Some(all[pos + 1].clone());
                } else {
                    return all.first().cloned();
                }
            }
        }

        for p in &all {
            if p.line > line || (p.line == line && p.start_col >= col) {
                return Some(p.clone());
            }
        }

        all.first().cloned()
    }

    /// Finds the previous parameter before (line, col), wrapping around to document end
    pub fn find_prev(buffer: &TextBuffer, line: usize, col: usize) -> Option<FormParam> {
        let all = Self::find_all(buffer);
        if all.is_empty() {
            return None;
        }

        if let Some(curr) = Self::find_at_cursor(buffer, line, col) {
            if let Some(pos) = all.iter().position(|p| p.name == curr.name && p.line == curr.line && p.start_col == curr.start_col) {
                if pos == 0 {
                    return all.last().cloned();
                } else {
                    return Some(all[pos - 1].clone());
                }
            }
        }

        for p in all.iter().rev() {
            if p.line < line || (p.line == line && p.end_col <= col) {
                return Some(p.clone());
            }
        }

        all.last().cloned()
    }

    /// Cycles or toggles a parameter at cursor position
    /// - For Toggle: toggles true <-> false
    /// - For Select: rotates options so next option is first
    /// Returns (param_name, new_value) if toggled/cycled
    pub fn toggle_or_cycle_at_cursor(buffer: &mut TextBuffer) -> Option<(String, String)> {
        let line = buffer.cursor_line;
        let col = buffer.cursor_col;
        let param = Self::find_at_cursor(buffer, line, col)?;

        match &param.param_type {
            FormParamType::Toggle => {
                let new_val = if param.value == "true" { "false" } else { "true" };
                let mut updated = param.clone();
                updated.value = new_val.to_string();
                let new_raw = updated.to_raw_string();
                Self::replace_param_range(buffer, param.line, param.start_col, param.end_col, &new_raw);
                Some((param.name, new_val.to_string()))
            }
            FormParamType::Select { options } => {
                let mut options = options.clone();
                if options.is_empty() {
                    return None;
                }
                options.rotate_left(1);
                let new_val = options[0].clone();
                let mut updated = param.clone();
                updated.value = new_val.clone();
                updated.param_type = FormParamType::Select { options };
                let new_raw = updated.to_raw_string();
                Self::replace_param_range(buffer, param.line, param.start_col, param.end_col, &new_raw);
                Some((param.name, new_val))
            }
            FormParamType::Input | FormParamType::Secret => None,
        }
    }

    /// Updates an input or secret parameter at cursor position with a new string value
    pub fn update_input_at_cursor(buffer: &mut TextBuffer, new_val: &str) -> Option<(String, String)> {
        let line = buffer.cursor_line;
        let col = buffer.cursor_col;
        let param = Self::find_at_cursor(buffer, line, col)?;

        let mut updated = param.clone();
        updated.value = new_val.to_string();
        let new_raw = updated.to_raw_string();
        Self::replace_param_range(buffer, param.line, param.start_col, param.end_col, &new_raw);
        Some((param.name, new_val.to_string()))
    }

    /// Replaces a character slice on a line in the buffer
    fn replace_param_range(
        buffer: &mut TextBuffer,
        line: usize,
        start_col: usize,
        end_col: usize,
        new_text: &str,
    ) {
        let line_start = buffer.rope.line_to_char(line);
        let start_char = line_start + start_col;
        let end_char = line_start + end_col;
        if start_char <= end_char && end_char <= buffer.rope.len_chars() {
            buffer.rope.remove(start_char..end_char);
            buffer.rope.insert(start_char, new_text);
            buffer.is_dirty = true;
        }
    }

    /// Updates all occurrences of a parameter by name across the buffer with a new value
    pub fn update_param_in_buffer(buffer: &mut TextBuffer, param_name: &str, new_value: &str) -> bool {
        let text = buffer.get_text();
        let updated = Self::update_param_in_str(&text, param_name, new_value);
        if text != updated {
            buffer.set_text(&updated);
            true
        } else {
            false
        }
    }

    /// Updates all occurrences of a parameter in a string
    pub fn update_param_in_str(text: &str, param_name: &str, new_value: &str) -> String {
        let re = Self::regex();
        re.replace_all(text, |cap: &regex::Captures| {
            let p_type = &cap[1];
            let name = &cap[2];
            let val_str = cap.get(3).map(|m| m.as_str()).unwrap_or("");

            if name.eq_ignore_ascii_case(param_name) {
                match p_type {
                    "input" => {
                        format!("{{{{param:input:{}={}}}}}", name, new_value)
                    }
                    "secret" => {
                        format!("{{{{param:secret:{}={}}}}}", name, new_value)
                    }
                    "toggle" => {
                        let clean = if new_value.eq_ignore_ascii_case("true") {
                            "true"
                        } else {
                            "false"
                        };
                        format!("{{{{param:toggle:{}={}}}}}", name, clean)
                    }
                    "select" => {
                        let mut options: Vec<String> = val_str
                            .split(',')
                            .map(|s| s.trim().to_string())
                            .filter(|s| !s.is_empty())
                            .collect();
                        // Put new_value first
                        if let Some(pos) = options.iter().position(|opt| opt.eq_ignore_ascii_case(new_value)) {
                            options.rotate_left(pos);
                        } else {
                            options.insert(0, new_value.to_string());
                        }
                        format!("{{{{param:select:{}={}}}}}", name, options.join(","))
                    }
                    _ => cap[0].to_string(),
                }
            } else {
                cap[0].to_string()
            }
        })
        .to_string()
    }

    /// Updates a parameter in a Rope
    pub fn update_param_in_rope(rope: &mut Rope, param_name: &str, new_value: &str) -> bool {
        let s = rope.to_string();
        let updated = Self::update_param_in_str(&s, param_name, new_value);
        if s != updated {
            *rope = Rope::from_str(&updated);
            true
        } else {
            false
        }
    }

    /// Collects a key-value map of all active parameters in the buffer
    pub fn collect_params(buffer: &TextBuffer) -> HashMap<String, String> {
        let mut map = HashMap::new();
        for param in Self::find_all(buffer) {
            map.insert(param.name, param.value);
        }
        map
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_line_params() {
        let line = "Deploy to {{param:select:CLUSTER=prod-us-east,prod-eu-west,staging}} in {{param:input:NAMESPACE=default}}. Dry run: {{param:toggle:DRY_RUN=true}}";
        let params = FormParser::parse_line(0, line);
        assert_eq!(params.len(), 3);

        assert_eq!(params[0].name, "CLUSTER");
        assert_eq!(params[0].value, "prod-us-east");
        assert!(matches!(params[0].param_type, FormParamType::Select { .. }));

        assert_eq!(params[1].name, "NAMESPACE");
        assert_eq!(params[1].value, "default");
        assert_eq!(params[1].param_type, FormParamType::Input);

        assert_eq!(params[2].name, "DRY_RUN");
        assert_eq!(params[2].value, "true");
        assert_eq!(params[2].param_type, FormParamType::Toggle);
    }

    #[test]
    fn test_parse_secret_param() {
        let line = "API key: {{param:secret:API_KEY=my_secret_token_123}}";
        let params = FormParser::parse_line(0, line);
        assert_eq!(params.len(), 1);
        assert_eq!(params[0].name, "API_KEY");
        assert_eq!(params[0].value, "my_secret_token_123");
        assert_eq!(params[0].param_type, FormParamType::Secret);
        assert_eq!(params[0].display_value(), "•".repeat(params[0].value.len()));
    }

    #[test]
    fn test_toggle_and_cycle_params() {
        let text = "Cluster: {{param:select:CLUSTER=prod,staging,dev}} Enabled: {{param:toggle:ACTIVE=false}}";
        let mut buffer = TextBuffer::from_str(text);

        // Cursor on toggle
        buffer.cursor_line = 0;
        buffer.cursor_col = 65;
        let toggled = FormParser::toggle_or_cycle_at_cursor(&mut buffer);
        assert_eq!(toggled, Some(("ACTIVE".to_string(), "true".to_string())));
        assert!(buffer.get_text().contains("{{param:toggle:ACTIVE=true}}"));

        // Cursor on select
        buffer.cursor_col = 15;
        let cycled = FormParser::toggle_or_cycle_at_cursor(&mut buffer);
        assert_eq!(cycled, Some(("CLUSTER".to_string(), "staging".to_string())));
        assert!(buffer.get_text().contains("{{param:select:CLUSTER=staging,dev,prod}}"));
    }

    #[test]
    fn test_update_param_in_buffer() {
        let text = "Target: {{param:input:HOST=10.0.0.1}} with key {{param:secret:SSH_KEY=orig_pass}}";
        let mut buffer = TextBuffer::from_str(text);

        assert!(FormParser::update_param_in_buffer(&mut buffer, "HOST", "192.168.1.50"));
        assert!(buffer.get_text().contains("{{param:input:HOST=192.168.1.50}}"));

        assert!(FormParser::update_param_in_buffer(&mut buffer, "SSH_KEY", "new_secure_key"));
        assert!(buffer.get_text().contains("{{param:secret:SSH_KEY=new_secure_key}}"));
    }
}
