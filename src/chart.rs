use regex::Regex;
use std::sync::LazyLock;

static LABEL_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^\s*([a-zA-Z0-9_-]+)\s*[:=]\s*(-?\d+(?:\.\d+)?)\s*(?:%|[a-zA-Z/]+)?\s*$").unwrap()
});

static NUM_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\b\d+(?:\.\d+)?\b").unwrap());

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

        // Fast path for structured JSON arrays or objects
        let is_json = (trimmed.starts_with('[') && trimmed.ends_with(']'))
            || (trimmed.starts_with('{') && trimmed.ends_with('}'));
        if is_json {
            let parsed_json = serde_json::from_str::<serde_json::Value>(trimmed);
            if let Ok(json_val) = parsed_json {
                let mut json_values = Vec::new();
                let mut json_bars = Vec::new();

                match json_val {
                    serde_json::Value::Array(items) => {
                        for item in items {
                            if let Some(n) = item.as_f64() {
                                json_values.push(n.round().max(0.0) as u64);
                            } else if let Some(obj) = item.as_object() {
                                let label = obj
                                    .get("label")
                                    .or_else(|| obj.get("name"))
                                    .or_else(|| obj.get("key"))
                                    .and_then(|v| v.as_str())
                                    .map(|s| s.to_string());
                                let val_opt = obj
                                    .get("value")
                                    .or_else(|| obj.get("val"))
                                    .and_then(|v| v.as_f64());
                                if let (Some(l), Some(v)) = (label, val_opt) {
                                    let val = v.round().max(0.0) as u64;
                                    json_bars.push((l, val));
                                    json_values.push(val);
                                }
                            }
                        }
                    }
                    serde_json::Value::Object(obj) => {
                        for (k, v) in obj {
                            if let Some(num) = v.as_f64() {
                                let val = num.round().max(0.0) as u64;
                                json_bars.push((k, val));
                                json_values.push(val);
                            }
                        }
                    }
                    _ => {}
                }

                if !json_values.is_empty() {
                    let max_val = json_values.iter().copied().max().unwrap_or(10).max(10);
                    return Self {
                        values: json_values,
                        bars: json_bars,
                        max_value: max_val,
                    };
                }
            }
        }

        let mut values = Vec::new();
        let mut bars = Vec::new();

        for line in trimmed.lines() {
            let l_trimmed = line.trim();
            if l_trimmed.is_empty() {
                continue;
            }

            if let Some(caps) = LABEL_RE.captures(l_trimmed) {
                let label = caps[1].to_string();
                if let Ok(num) = caps[2].parse::<f64>() {
                    let val = num.round().max(0.0) as u64;
                    bars.push((label, val));
                    values.push(val);
                }
            } else {
                // Check if line contains numeric tokens
                for num_cap in NUM_RE.find_iter(l_trimmed) {
                    if let Ok(num) = num_cap.as_str().parse::<f64>() {
                        let val = num.round().max(0.0) as u64;
                        values.push(val);
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

    #[test]
    fn test_parse_json_metrics() {
        let json_arr = "[10, 20, 30, 40]";
        let chart = ChartData::parse(json_arr);
        assert_eq!(chart.values, vec![10, 20, 30, 40]);
        assert_eq!(chart.max_value, 40);

        let json_objs = r#"[{"label": "CPU", "value": 55}, {"label": "MEM", "value": 75}]"#;
        let chart2 = ChartData::parse(json_objs);
        assert_eq!(chart2.values, vec![55, 75]);
        assert_eq!(
            chart2.bars,
            vec![("CPU".to_string(), 55), ("MEM".to_string(), 75)]
        );
        assert_eq!(chart2.max_value, 75);
    }

    #[test]
    fn test_parse_labeled_metrics_with_units() {
        let text = "latency = 45ms\nmemory: 512MB\nerror_rate: 2%\n";
        let chart = ChartData::parse(text);
        assert_eq!(
            chart.bars,
            vec![
                ("latency".to_string(), 45),
                ("memory".to_string(), 512),
                ("error_rate".to_string(), 2),
            ]
        );
        assert_eq!(chart.values, vec![45, 512, 2]);
        assert_eq!(chart.max_value, 512);
    }
}
