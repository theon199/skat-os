use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::editor::TextBuffer;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TableData {
    pub headers: Vec<String>,
    pub rows: Vec<Vec<String>>,
    pub selected_row: usize,
    pub raw_json: Option<String>,
}

impl TableData {
    pub fn new(headers: Vec<String>, rows: Vec<Vec<String>>) -> Self {
        let mut data = Self {
            headers,
            rows,
            selected_row: 0,
            raw_json: None,
        };
        data.clamp_selected_row();
        data
    }

    /// Clamps selected_row to valid range [0, rows.len())
    pub fn clamp_selected_row(&mut self) {
        if self.rows.is_empty() {
            self.selected_row = 0;
        } else if self.selected_row >= self.rows.len() {
            self.selected_row = self.rows.len() - 1;
        }
    }

    /// Selects a specific row index safely bounded
    pub fn select_row(&mut self, idx: usize) {
        if self.rows.is_empty() {
            self.selected_row = 0;
        } else {
            self.selected_row = idx.min(self.rows.len() - 1);
        }
    }

    /// Parses structured table from a JSON array of objects or arrays
    pub fn from_json(json_str: &str) -> Option<Self> {
        let trimmed = json_str.trim();
        if !trimmed.starts_with('[') && !trimmed.starts_with('{') {
            return None;
        }

        let parsed: serde_json::Value = serde_json::from_str(trimmed).ok()?;

        match parsed {
            serde_json::Value::Array(items) => {
                if items.is_empty() {
                    return None;
                }

                // Check if elements are JSON objects
                if let Some(first_obj) = items[0].as_object() {
                    let mut headers: Vec<String> = first_obj.keys().cloned().collect();

                    // Check if other objects have keys not in the first object
                    for item in items.iter().skip(1) {
                        if let Some(obj) = item.as_object() {
                            for k in obj.keys() {
                                if !headers.contains(k) {
                                    headers.push(k.clone());
                                }
                            }
                        }
                    }

                    if headers.is_empty() {
                        return None;
                    }

                    let mut rows = Vec::new();
                    for item in &items {
                        if let Some(obj) = item.as_object() {
                            let mut row = Vec::new();
                            for h in &headers {
                                let val_str = match obj.get(h) {
                                    Some(serde_json::Value::String(s)) => s.clone(),
                                    Some(serde_json::Value::Null) => String::new(),
                                    Some(other) => other.to_string(),
                                    None => String::new(),
                                };
                                row.push(val_str);
                            }
                            rows.push(row);
                        }
                    }

                    if rows.is_empty() {
                        return None;
                    }

                    Some(Self {
                        headers,
                        rows,
                        selected_row: 0,
                        raw_json: Some(trimmed.to_string()),
                    })
                } else if let Some(first_arr) = items[0].as_array() {
                    // Array of arrays: first row as headers if strings
                    if items.len() < 2 {
                        return None;
                    }
                    let headers: Vec<String> = first_arr
                        .iter()
                        .map(|v| match v {
                            serde_json::Value::String(s) => s.clone(),
                            other => other.to_string(),
                        })
                        .collect();

                    if headers.is_empty() {
                        return None;
                    }

                    let mut rows = Vec::new();
                    for item in items.iter().skip(1) {
                        if let Some(arr) = item.as_array() {
                            let mut row: Vec<String> = arr
                                .iter()
                                .map(|v| match v {
                                    serde_json::Value::String(s) => s.clone(),
                                    other => other.to_string(),
                                })
                                .collect();

                            // Align columns with header count to prevent out-of-bounds access
                            if row.len() < headers.len() {
                                row.resize(headers.len(), String::new());
                            } else if row.len() > headers.len() {
                                row.truncate(headers.len());
                            }
                            rows.push(row);
                        }
                    }

                    if rows.is_empty() {
                        return None;
                    }

                    Some(Self {
                        headers,
                        rows,
                        selected_row: 0,
                        raw_json: Some(trimmed.to_string()),
                    })
                } else {
                    None
                }
            }
            serde_json::Value::Object(obj) => {
                // Key-value object converted to 2-column table
                let headers = vec!["Key".to_string(), "Value".to_string()];
                let mut rows = Vec::new();
                for (k, v) in obj {
                    let val_str = match v {
                        serde_json::Value::String(s) => s,
                        serde_json::Value::Null => String::new(),
                        other => other.to_string(),
                    };
                    rows.push(vec![k, val_str]);
                }
                Some(Self {
                    headers,
                    rows,
                    selected_row: 0,
                    raw_json: Some(trimmed.to_string()),
                })
            }
            _ => None,
        }
    }

    /// Parses tabular CSV / TSV / pipe-delimited text
    pub fn from_csv(csv_str: &str) -> Option<Self> {
        let lines: Vec<&str> = csv_str
            .lines()
            .map(|l| l.trim())
            .filter(|l| !l.is_empty())
            .collect();

        if lines.len() < 2 {
            return None;
        }

        // Detect delimiter: tab, comma, or pipe
        let first_line = lines[0];
        let delimiter = if first_line.contains('\t') {
            '\t'
        } else if first_line.contains(',') {
            ','
        } else if first_line.contains('|') {
            '|'
        } else {
            return None;
        };

        let parse_row = |line: &str| -> Vec<String> {
            let trimmed_line = if delimiter == '|' {
                line.trim_matches('|')
            } else {
                line
            };
            trimmed_line
                .split(delimiter)
                .map(|s| s.trim().trim_matches(|c| c == '"' || c == '\'').to_string())
                .collect()
        };

        let headers = parse_row(lines[0]);
        if headers.is_empty() {
            return None;
        }

        let mut rows = Vec::new();
        for &line in &lines[1..] {
            // Skip markdown table divider line like |---|---|
            if line
                .chars()
                .all(|c| c == '-' || c == '|' || c == ':' || c.is_whitespace())
            {
                continue;
            }
            let mut row = parse_row(line);
            if row.is_empty() {
                continue;
            }
            // Align columns with header count
            if row.len() < headers.len() {
                row.resize(headers.len(), String::new());
            } else if row.len() > headers.len() {
                row.truncate(headers.len());
            }
            rows.push(row);
        }

        if rows.is_empty() {
            return None;
        }

        Some(Self {
            headers,
            rows,
            selected_row: 0,
            raw_json: None,
        })
    }

    /// Parses text output into a TableData (trying JSON first, then CSV/TSV)
    pub fn parse(text: &str) -> Option<Self> {
        let stripped = crate::ansi::strip_ansi_codes(text);
        let trimmed = stripped.trim();
        if trimmed.is_empty() || trimmed == "(No output)" {
            return None;
        }

        Self::from_json(trimmed).or_else(|| Self::from_csv(trimmed))
    }

    /// Retrieves cell value by column name for the specified row
    pub fn get_cell(&self, row_idx: usize, col_name: &str) -> Option<&str> {
        let col_idx = self
            .headers
            .iter()
            .position(|h| h.eq_ignore_ascii_case(col_name))?;

        self.rows
            .get(row_idx)
            .and_then(|r| r.get(col_idx).map(|s| s.as_str()))
    }

    /// Retrieves cell value by column index for the specified row
    pub fn get_cell_by_idx(&self, row_idx: usize, col_idx: usize) -> Option<&str> {
        self.rows
            .get(row_idx)
            .and_then(|r| r.get(col_idx).map(|s| s.as_str()))
    }

    /// Retrieves cell value in currently selected row by column name
    pub fn get_selected_cell(&self, col_name: &str) -> Option<&str> {
        self.get_cell(self.selected_row, col_name)
    }

    /// Retrieves the currently selected row slice
    pub fn get_selected_row(&self) -> Option<&Vec<String>> {
        self.rows.get(self.selected_row)
    }

    /// Exports selected row as JSON object string
    pub fn get_selected_row_json(&self) -> Option<String> {
        let row = self.get_selected_row()?;
        let mut map = serde_json::Map::new();
        for (i, h) in self.headers.iter().enumerate() {
            let val = row.get(i).cloned().unwrap_or_default();
            map.insert(h.clone(), serde_json::Value::String(val));
        }
        Some(serde_json::Value::Object(map).to_string())
    }

    /// Selects next row (bounded by row count)
    pub fn select_next_row(&mut self) {
        if !self.rows.is_empty() && self.selected_row + 1 < self.rows.len() {
            self.selected_row += 1;
        }
    }

    /// Selects previous row
    pub fn select_prev_row(&mut self) {
        if self.selected_row > 0 {
            self.selected_row -= 1;
        }
    }

    /// Scans buffer for existing output blocks that contain table data
    pub fn collect_tables(buffer: &TextBuffer) -> HashMap<String, TableData> {
        let mut tables = HashMap::new();
        let blocks = crate::runner::Runner::all_code_blocks(buffer);

        for (idx, block) in blocks.into_iter().enumerate() {
            if let Some(tbl) = block
                .existing_output
                .as_ref()
                .and_then(|out| Self::parse(&out.output_text))
            {
                tables.insert(format!("block_{}", idx), tbl.clone());
                tables.insert(block.id.clone(), tbl.clone());
                if let Some(name) = block.name {
                    tables.insert(name, tbl);
                }
            }
        }

        tables
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_json_table() {
        let json = r#"[
            {"id": "pod-1", "name": "nginx", "status": "Running"},
            {"id": "pod-2", "name": "redis", "status": "Pending"}
        ]"#;

        let table = TableData::parse(json).expect("Failed to parse JSON table");
        assert_eq!(table.headers, vec!["id", "name", "status"]);
        assert_eq!(table.rows.len(), 2);
        assert_eq!(table.rows[0], vec!["pod-1", "nginx", "Running"]);
        assert_eq!(table.rows[1], vec!["pod-2", "redis", "Pending"]);

        assert_eq!(table.get_cell(0, "name"), Some("nginx"));
        assert_eq!(table.get_selected_cell("id"), Some("pod-1"));
    }

    #[test]
    fn test_parse_csv_table() {
        let csv = "ID,Service,Port\n1,web,80\n2,db,5432";
        let table = TableData::parse(csv).expect("Failed to parse CSV table");
        assert_eq!(table.headers, vec!["ID", "Service", "Port"]);
        assert_eq!(table.rows.len(), 2);
        assert_eq!(table.rows[0], vec!["1", "web", "80"]);
        assert_eq!(table.rows[1], vec!["2", "db", "5432"]);
    }

    #[test]
    fn test_row_navigation_and_export() {
        let json = r#"[
            {"id": "1", "user": "alice"},
            {"id": "2", "user": "bob"}
        ]"#;
        let mut table = TableData::parse(json).unwrap();
        assert_eq!(table.selected_row, 0);
        assert_eq!(table.get_selected_cell("user"), Some("alice"));

        table.select_next_row();
        assert_eq!(table.selected_row, 1);
        assert_eq!(table.get_selected_cell("user"), Some("bob"));

        let row_json = table.get_selected_row_json().unwrap();
        assert!(row_json.contains("\"user\":\"bob\""));
    }

    #[test]
    fn test_array_of_arrays_column_alignment() {
        let json = r#"[
            ["col1", "col2", "col3"],
            ["a", "b"],
            ["c", "d", "e", "f"]
        ]"#;
        let table = TableData::parse(json).expect("Failed to parse array-of-arrays");
        assert_eq!(table.headers, vec!["col1", "col2", "col3"]);
        assert_eq!(table.rows.len(), 2);
        // Short row aligned with empty string
        assert_eq!(table.rows[0], vec!["a", "b", ""]);
        // Long row truncated to header length
        assert_eq!(table.rows[1], vec!["c", "d", "e"]);
        // Safe indexing
        assert_eq!(table.get_cell(0, "col3"), Some(""));
        assert_eq!(table.get_cell(1, "col3"), Some("e"));
        assert_eq!(table.get_cell(99, "col1"), None);
    }
}
