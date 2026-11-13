use regex::Regex;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChartData {
    pub values: Vec<u64>,
    pub bars: Vec<(String, u64)>,
    pub max_value: u64,
}

impl Default for ChartData {
    fn default() -> Self {
        Self {
            values: Vec::new(),
            bars: Vec::new(),
            max_value: 10,
        }
    }
}

impl ChartData {
    pub fn new(values: Vec<u64>, bars: Vec<(String, u64)>) -> Self {
        let max_val = values.iter().copied().max().unwrap_or(10).max(10);
        Self {
            values,
            bars,
            max_value: max_val,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.values.is_empty() && self.bars.is_empty()
    }

    /// Parses streaming or static code output into numeric metrics and labeled bars
    pub fn parse(text: &str) -> Self {
        let stripped = crate::ansi::strip_ansi_codes(text);
        let trimmed = stripped.trim();
        if trimmed.is_empty() || trimmed == "(No output)" {
            return Self::default();
        }

        let mut values = Vec::new();
        let mut bars = Vec::new();

        // Pattern 1: labeled lines like "CPU: 45", "memory = 80", "disk 60%"
        let label_re = Regex::new(r"^\s*([a-zA-Z0-9_-]+)\s*[:=]\s*(\d+(?:\.\d+)?)\s*%?\s*$").unwrap();
        // Pattern 2: comma-separated or space-separated numbers
        let num_re = Regex::new(r"\b\d+(?:\.\d+)?\b").unwrap();

        for line in trimmed.lines() {
            let l_trimmed = line.trim();
            if l_trimmed.is_empty() {
                continue;
            }

            if let Some(caps) = label_re.captures(l_trimmed) {
                let label = caps[1].to_string();
                if let Ok(num) = caps[2].parse::<f64>() {
                    let val = num.round() as u64;
                    bars.push((label, val));
                    values.push(val);
                }
            } else {
                // Check if line contains numeric tokens
                for num_cap in num_re.find_iter(l_trimmed) {
                    if let Ok(num) = num_cap.as_str().parse::<f64>() {
                        let val = num.round() as u64;
                        values.push(val);
                    }
                }
            }
        }

        // If JSON array of numbers was provided like [10, 25, 40]
        if values.is_empty() && trimmed.starts_with('[') && trimmed.ends_with(']') {
            if let Ok(serde_json::Value::Array(items)) = serde_json::from_str::<serde_json::Value>(trimmed) {
                for item in items {
                    if let Some(n) = item.as_f64() {
                        values.push(n.round() as u64);
                    }
                }
            }
        }

        let max_val = values.iter().copied().max().unwrap_or(10).max(10);

        Self {
            values,
            bars,
            max_value: max_val,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_numeric_stream() {
        let stream = "10\n25\n45\n80\n60\n";
        let chart = ChartData::parse(stream);
        assert_eq!(chart.values, vec![10, 25, 45, 80, 60]);
        assert_eq!(chart.max_value, 80);
    }

    #[test]
    fn test_parse_labeled_metrics() {
        let text = "CPU: 45\nMEM: 78\nDISK: 32\n";
        let chart = ChartData::parse(text);
        assert_eq!(
            chart.bars,
            vec![
                ("CPU".to_string(), 45),
                ("MEM".to_string(), 78),
                ("DISK".to_string(), 32),
            ]
        );
        assert_eq!(chart.values, vec![45, 78, 32]);
        assert_eq!(chart.max_value, 78);
    }

    #[test]
    fn test_parse_comma_separated_sparkline() {
        let text = "12, 45, 67, 89, 23, 56";
        let chart = ChartData::parse(text);
        assert_eq!(chart.values, vec![12, 45, 67, 89, 23, 56]);
        assert_eq!(chart.max_value, 89);
    }
}
