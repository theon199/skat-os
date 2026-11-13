use anyhow::{Context, Result};
use regex::Regex;
use std::collections::{HashMap, VecDeque};
use std::io::{Read, Write};
use std::path::Path;
use std::process::Stdio;
use std::sync::LazyLock;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::process::Command;

static SEC_INTERPOLATE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\{\{\s*secrets?[:.]([a-zA-Z0-9_./:#=-]+)\s*\}\}").unwrap()
});

static VAR_INTERPOLATE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\{\{\s*([a-zA-Z0-9_-]+)(?:\[(\d+)\])?(?:[.:]([a-zA-Z0-9_./:#=-]+))?\s*\}\}").unwrap()
});

static DEP_TEMPLATE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\{\{\s*([a-zA-Z0-9_-]+)(?:\[\d+\])?(?:[.:][a-zA-Z0-9_.-]+)?\s*\}\}").unwrap()
});

static DEP_ENV_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\$\{?SKAT_([a-zA-Z0-9_]+?)(?:_STDOUT|_STDERR|_EXIT_CODE)?\}?\b").unwrap()
});

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputStatus {
    Running,
    Exit(i32),
    Interrupted,
}

pub struct StreamingHandle {
    pub stdin_tx: tokio::sync::mpsc::UnboundedSender<Vec<u8>>,
    pub cancel_tx: Option<tokio::sync::oneshot::Sender<()>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BlockFormat {
    #[default]
    Text,
    Table,
    Chart,
    Sparkline,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FenceAttributes {
    pub language: String,
    pub name: Option<String>,
    pub format: BlockFormat,
    pub target: Option<String>,
    pub proxy: Option<String>,
    pub parallel: bool,
    pub confirm: Option<String>,
    pub danger: bool,
    pub blast_radius: Option<String>,
    pub dry_run: bool,
    pub approvals: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeBlock {
    pub id: String,
    pub name: Option<String>,
    pub language: String,
    pub code: String,
    pub start_fence_line: usize,
    pub end_fence_line: usize,
    pub existing_output: Option<OutputBlockSpan>,
    pub format: BlockFormat,
    pub target: Option<String>,
    pub proxy: Option<String>,
    pub parallel: bool,
    pub confirm: Option<String>,
    pub danger: bool,
    pub blast_radius: Option<String>,
    pub dry_run: bool,
    pub approvals: usize,
}

impl CodeBlock {
    pub fn analyze_security(&self) -> crate::guard::SecurityAnalysisResult {
        crate::guard::StaticAnalyzer::analyze(&self.code)
    }

    pub fn is_dangerous(&self) -> bool {
        self.danger
            || self.blast_radius.as_deref() == Some("critical")
            || self.blast_radius.as_deref() == Some("high")
            || self.analyze_security().is_destructive
    }

    pub fn requires_confirmation(&self) -> bool {
        self.confirm.is_some() || self.is_dangerous()
    }

    pub fn effective_confirm_prompt(&self) -> Option<String> {
        if let Some(msg) = &self.confirm {
            Some(msg.clone())
        } else {
            let sec = self.analyze_security();
            if sec.is_destructive {
                Some(format!("Destructive action detected ({})! Confirm execution?", sec.reason))
            } else if self.is_dangerous() {
                Some("High blast-radius execution. Are you sure you want to proceed?".to_string())
            } else {
                None
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutputBlockSpan {
    pub start_line: usize,
    pub end_line: usize,
    pub is_collapsed: bool,
    pub exit_code: i32,
    pub output_text: String,
    pub format: BlockFormat,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionResult {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
    pub timed_out: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlaybookReport {
    pub total_steps: usize,
    pub executed_steps: usize,
    pub failed_step: Option<usize>,
    pub failed_block_id: Option<String>,
    pub success: bool,
    pub error_message: Option<String>,
}

pub struct Runner;

impl Runner {
    /// Parses code block fence attributes (e.g. `bash name=get_pod` or `name="get_pod" python`)
    pub fn parse_fence_header(header_rest: &str) -> (String, Option<String>) {
        let (lang, name, _) = Self::parse_fence_header_full(header_rest);
        (lang, name)
    }

    /// Splits a fence header string into tokens respecting single and double quotes
    pub fn tokenize_fence_header(input: &str) -> Vec<String> {
        let mut tokens = Vec::new();
        let mut chars = input.chars().peekable();

        while let Some(&c) = chars.peek() {
            if c.is_whitespace() {
                chars.next();
                continue;
            }

            let mut token = String::new();
            let mut in_quote: Option<char> = None;

            while let Some(&ch) = chars.peek() {
                if let Some(q) = in_quote {
                    chars.next();
                    if ch == q {
                        in_quote = None;
                    } else {
                        token.push(ch);
                    }
                } else if ch == '"' || ch == '\'' {
                    in_quote = Some(ch);
                    chars.next();
                } else if ch.is_whitespace() {
                    break;
                } else {
                    token.push(ch);
                    chars.next();
                }
            }

            if !token.is_empty() {
                tokens.push(token);
            }
        }

        tokens
    }

    /// Full parser for all fenced code block attributes (name, format, target, proxy, parallel, language, confirm, danger, blast_radius, dry_run, approvals)
    pub fn parse_fence_attributes(header_rest: &str) -> FenceAttributes {
        let mut language = String::new();
        let mut name = None;
        let mut format = BlockFormat::Text;
        let mut target = None;
        let mut proxy = None;
        let mut parallel = false;
        let mut confirm = None;
        let mut danger = false;
        let mut blast_radius = None;
        let mut dry_run = false;
        let mut approvals = 1;

        let tokens = Self::tokenize_fence_header(header_rest);

        for token in tokens {
            if let Some(val) = token.strip_prefix("name=") {
                let clean = val.trim_matches(|c| c == '"' || c == '\'');
                if !clean.is_empty() {
                    name = Some(clean.to_string());
                }
            } else if let Some(val) = token.strip_prefix("format=") {
                let clean = val.trim_matches(|c| c == '"' || c == '\'').to_lowercase();
                format = match clean.as_str() {
                    "table" => BlockFormat::Table,
                    "chart" | "bar" | "barchart" => BlockFormat::Chart,
                    "sparkline" | "spark" => BlockFormat::Sparkline,
                    _ => BlockFormat::Text,
                };
            } else if let Some(val) = token.strip_prefix("target=") {
                let clean = val.trim_matches(|c| c == '"' || c == '\'');
                if !clean.is_empty() {
                    target = Some(clean.to_string());
                }
            } else if let Some(val) = token.strip_prefix("proxy=") {
                let clean = val.trim_matches(|c| c == '"' || c == '\'');
                if !clean.is_empty() {
                    proxy = Some(clean.to_string());
                }
            } else if let Some(val) = token.strip_prefix("parallel=") {
                let clean = val.trim_matches(|c| c == '"' || c == '\'').to_lowercase();
                parallel = clean == "true" || clean == "1";
            } else if token == "parallel" {
                parallel = true;
            } else if let Some(val) = token.strip_prefix("confirm=") {
                let clean = val.trim_matches(|c| c == '"' || c == '\'');
                if !clean.is_empty() {
                    confirm = Some(clean.to_string());
                }
            } else if let Some(val) = token.strip_prefix("danger=") {
                let clean = val.trim_matches(|c| c == '"' || c == '\'').to_lowercase();
                danger = clean == "true" || clean == "1";
            } else if token == "danger" {
                danger = true;
            } else if let Some(val) = token.strip_prefix("blast_radius=") {
                let clean = val.trim_matches(|c| c == '"' || c == '\'').to_lowercase();
                if clean == "critical" || clean == "high" {
                    danger = true;
                }
                blast_radius = Some(clean);
            } else if let Some(val) = token.strip_prefix("dry_run=") {
                let clean = val.trim_matches(|c| c == '"' || c == '\'').to_lowercase();
                dry_run = clean == "true" || clean == "1";
            } else if token == "dry_run" {
                dry_run = true;
            } else if let Some(val) = token.strip_prefix("approvals=") {
                let clean = val.trim_matches(|c| c == '"' || c == '\'');
                if let Ok(n) = clean.parse::<usize>() {
                    approvals = n;
                }
            } else if !token.contains('=') && language.is_empty() {
                language = token.to_lowercase();
            }
        }

        if language.is_empty() {
            language = "bash".to_string();
        }

        FenceAttributes {
            language,
            name,
            format,
            target,
            proxy,
            parallel,
            confirm,
            danger,
            blast_radius,
            dry_run,
            approvals,
        }
    }

    /// Full parser for code block fence attributes including format attribute (backward-compatible)
    pub fn parse_fence_header_full(header_rest: &str) -> (String, Option<String>, BlockFormat) {
        let attrs = Self::parse_fence_attributes(header_rest);
        (attrs.language, attrs.name, attrs.format)
    }

    /// Detects if cursor is inside or immediately adjacent to a fenced code block
    pub fn find_code_block(buffer: &crate::editor::TextBuffer, cursor_line: usize) -> Option<CodeBlock> {
        Self::all_code_blocks(buffer)
            .into_iter()
            .find(|b| cursor_line >= b.start_fence_line && cursor_line <= b.end_fence_line)
    }

    /// Finds if an ```output block immediately follows a code block
    pub fn find_following_output_block(buffer: &crate::editor::TextBuffer, end_fence_line: usize) -> Option<OutputBlockSpan> {
        let total_lines = buffer.line_count();
        let mut check_line = end_fence_line + 1;

        // Skip at most one blank line
        if check_line < total_lines && buffer.line_to_string(check_line).trim().is_empty() {
            check_line += 1;
        }

        if check_line >= total_lines {
            return None;
        }

        let first = buffer.line_to_string(check_line);
        let trimmed = first.trim();

        if trimmed.starts_with("```output") {
            let is_collapsed = trimmed.contains("[collapsed]");

            let mut exit_code = 0;
            if trimmed.contains("[interrupted]") {
                exit_code = 130;
            } else if let Some(idx) = trimmed.find("[exit: ") {
                let rest = &trimmed[idx + 7..];
                if let Some(end_idx) = rest.find(']') {
                    let code_str = rest[..end_idx].trim();
                    if let Ok(c) = code_str.parse::<i32>() {
                        exit_code = c;
                    }
                }
            }

            let format = if trimmed.contains("format=table") {
                BlockFormat::Table
            } else if trimmed.contains("format=chart") || trimmed.contains("format=bar") {
                BlockFormat::Chart
            } else if trimmed.contains("format=sparkline") || trimmed.contains("format=spark") {
                BlockFormat::Sparkline
            } else {
                BlockFormat::Text
            };

            // Find closing fence
            for j in (check_line + 1)..total_lines {
                let l = buffer.line_to_string(j);
                if l.trim().starts_with("```") {
                    let mut out_lines = Vec::new();
                    for line in (check_line + 1)..j {
                        out_lines.push(buffer.line_to_string(line));
                    }
                    let full_text = out_lines.join("\n");
                    let output_text = if full_text.trim() == "(No output)" {
                        String::new()
                    } else {
                        full_text
                    };

                    return Some(OutputBlockSpan {
                        start_line: check_line,
                        end_line: j,
                        is_collapsed,
                        exit_code,
                        output_text,
                        format,
                    });
                }
            }
        }

        None
    }

    /// Returns all code blocks in the buffer in document order
    pub fn all_code_blocks(buffer: &crate::editor::TextBuffer) -> Vec<CodeBlock> {
        let mut blocks = Vec::new();
        let total_lines = buffer.line_count();
        let mut i = 0;
        let mut block_idx = 0;

        while i < total_lines {
            let line = buffer.line_to_string(i);
            let trimmed = line.trim();

            if trimmed.starts_with("```output") {
                // Skip output block to its closing fence
                i += 1;
                while i < total_lines {
                    let l = buffer.line_to_string(i);
                    if l.trim().starts_with("```") {
                        i += 1;
                        break;
                    }
                    i += 1;
                }
                continue;
            }

            if trimmed.starts_with("```") {
                let start_fence = i;
                let header_str = trimmed.trim_start_matches('`').trim().to_string();
                let attrs = Self::parse_fence_attributes(&header_str);

                // Find closing fence
                let mut end_fence = None;
                let mut j = i + 1;
                while j < total_lines {
                    let l = buffer.line_to_string(j);
                    if l.trim().starts_with("```") {
                        end_fence = Some(j);
                        break;
                    }
                    j += 1;
                }

                if let Some(end) = end_fence {
                    let mut code_lines = Vec::new();
                    for line_num in (start_fence + 1)..end {
                        code_lines.push(buffer.line_to_string(line_num));
                    }
                    let code = code_lines.join("\n");

                    let existing_output = Self::find_following_output_block(buffer, end);

                    let id = if let Some(n) = &attrs.name {
                        n.clone()
                    } else {
                        format!("block_{}", block_idx)
                    };

                    blocks.push(CodeBlock {
                        id,
                        name: attrs.name,
                        language: attrs.language,
                        code,
                        start_fence_line: start_fence,
                        end_fence_line: end,
                        existing_output,
                        format: attrs.format,
                        target: attrs.target,
                        proxy: attrs.proxy,
                        parallel: attrs.parallel,
                        confirm: attrs.confirm,
                        danger: attrs.danger,
                        blast_radius: attrs.blast_radius,
                        dry_run: attrs.dry_run,
                        approvals: attrs.approvals,
                    });

                    block_idx += 1;
                    i = end + 1;
                    continue;
                }
            }

            i += 1;
        }

        blocks
    }

    /// Finds a code block by identifier (custom name, block_X, or line position)
    pub fn find_code_block_by_id(buffer: &crate::editor::TextBuffer, target_id: &str) -> Option<CodeBlock> {
        let blocks = Self::all_code_blocks(buffer);
        for (idx, b) in blocks.iter().enumerate() {
            let index_id = format!("block_{}", idx);
            if target_id == index_id
                || target_id == b.id
                || target_id == format!("{}_{}", b.start_fence_line, b.language)
                || b.name.as_deref() == Some(target_id)
            {
                return Some(b.clone());
            }
        }
        if let Some(num_str) = target_id.strip_prefix("block_")
            && let Ok(num) = num_str.parse::<usize>()
            && let Some(b) = blocks.get(num)
        {
            return Some(b.clone());
        }
        None
    }

    /// Interpolates previous outputs, form parameters ({{params.CLUSTER}}), table selections ({{get_pods.selected.name}}),
    /// multi-host fleet outputs ({{web_servers[0].stdout}}, {{web_servers.failed}}), and dynamic secrets ({{secret.KEY}})
    pub fn interpolate_variables_with_context(
        code: &str,
        outputs: &HashMap<String, ExecutionResult>,
        params: &HashMap<String, String>,
        tables: &HashMap<String, crate::table::TableData>,
    ) -> String {
        // 0. Secret interpolation: {{secret.KEY}}, {{secrets.KEY}}, {{secret:KEY}}
        let code_after_sec = SEC_INTERPOLATE_RE.replace_all(code, |caps: &regex::Captures| {
            let key = &caps[1];
            let mut vault = crate::vault::SecretVault::new();
            if let Some(val) = vault.resolve(key) {
                val
            } else {
                caps[0].to_string()
            }
        });

        VAR_INTERPOLATE_RE.replace_all(&code_after_sec, |caps: &regex::Captures| {
            let key = &caps[1];
            let index_opt: Option<usize> = caps.get(2).and_then(|m| m.as_str().parse().ok());
            let field = caps.get(3).map(|m| m.as_str()).unwrap_or("");

            // 0. Secret interpolation fallback
            if key.eq_ignore_ascii_case("secret") || key.eq_ignore_ascii_case("secrets") {
                let mut vault = crate::vault::SecretVault::new();
                if let Some(val) = vault.resolve(field) {
                    return val;
                }
            }

            // 1. Fleet output interpolation: {{web_servers[0].stdout}}, {{web_servers.failed}}, {{web_servers.success}}
            let result = outputs.get(key).or_else(|| {
                outputs
                    .iter()
                    .find(|(k, _)| k.eq_ignore_ascii_case(key))
                    .map(|(_, v)| v)
            });

            if let Some(res) = result
                && let Some(fleet) = crate::mesh::FleetOutput::parse(&res.stdout)
            {
                if let Some(idx) = index_opt {
                    if let Some(host_out) = fleet.hosts.get(idx) {
                        let out_field = if field.is_empty() { "stdout" } else { field };
                        return match out_field.to_lowercase().as_str() {
                            "stdout" => crate::ansi::strip_ansi_codes(&host_out.stdout)
                                .trim_end_matches(&['\r', '\n'][..])
                                .to_string(),
                            "stderr" => crate::ansi::strip_ansi_codes(&host_out.stderr)
                                .trim_end_matches(&['\r', '\n'][..])
                                .to_string(),
                            "exit_code" | "code" => host_out.exit_code.to_string(),
                            "host" => host_out.host.clone(),
                            _ => crate::ansi::strip_ansi_codes(&host_out.stdout)
                                .trim_end_matches(&['\r', '\n'][..])
                                .to_string(),
                        };
                    }
                } else {
                    match field.to_lowercase().as_str() {
                        "failed" => return fleet.failed_hosts().join(", "),
                        "success" => return fleet.is_success().to_string(),
                        "count" | "total" => return fleet.hosts.len().to_string(),
                        "hosts" => {
                            let all_h: Vec<String> = fleet.hosts.iter().map(|h| h.host.clone()).collect();
                            return all_h.join(", ");
                        }
                        _ => {}
                    }
                }
            }

            // 2. Parameter interpolation: {{params.CLUSTER}}, {{param.CLUSTER}}, {{param:CLUSTER}}
            if (key.eq_ignore_ascii_case("params") || key.eq_ignore_ascii_case("param"))
                && let Some(val) = params.get(field).or_else(|| {
                    params
                        .iter()
                        .find(|(k, _)| k.eq_ignore_ascii_case(field))
                        .map(|(_, v)| v)
                })
            {
                return val.clone();
            }

            // Also check direct param match if {{CLUSTER}} without dot
            if field.is_empty()
                && index_opt.is_none()
                && let Some(val) = params.get(key).or_else(|| {
                    params
                        .iter()
                        .find(|(k, _)| k.eq_ignore_ascii_case(key))
                        .map(|(_, v)| v)
                })
            {
                return val.clone();
            }

            // 3. Table selection interpolation: {{table_name.selected.id}}, {{table_name.selected}}
            if field.starts_with("selected") {
                let subfield = field.strip_prefix("selected.").or_else(|| field.strip_prefix("selected"));
                let table_opt = tables.get(key).or_else(|| {
                    tables
                        .iter()
                        .find(|(k, _)| k.eq_ignore_ascii_case(key))
                        .map(|(_, v)| v)
                });

                if let Some(tbl) = table_opt {
                    match subfield {
                        Some("") | None => {
                            if let Some(json) = tbl.get_selected_row_json() {
                                return json;
                            }
                        }
                        Some(col_name) => {
                            if let Some(cell) = tbl.get_selected_cell(col_name) {
                                return cell.to_string();
                            }
                        }
                    }
                }
            }

            // 4. Execution outputs interpolation: {{block_0.stdout}}, {{get_pod.stdout}}, etc.
            if let Some(res) = result {
                let out_field = if field.is_empty() { "stdout" } else { field };
                match out_field.to_lowercase().as_str() {
                    "stdout" => crate::ansi::strip_ansi_codes(&res.stdout)
                        .trim_end_matches(&['\r', '\n'][..])
                        .to_string(),
                    "stderr" => crate::ansi::strip_ansi_codes(&res.stderr)
                        .trim_end_matches(&['\r', '\n'][..])
                        .to_string(),
                    "exit_code" | "code" => res.exit_code.to_string(),
                    _ => crate::ansi::strip_ansi_codes(&res.stdout)
                        .trim_end_matches(&['\r', '\n'][..])
                        .to_string(),
                }
            } else {
                caps[0].to_string()
            }
        })
        .to_string()
    }

    /// Interpolates with params
    pub fn interpolate_variables_with_params(
        code: &str,
        outputs: &HashMap<String, ExecutionResult>,
        params: &HashMap<String, String>,
    ) -> String {
        Self::interpolate_variables_with_context(code, outputs, params, &HashMap::new())
    }

    /// Interpolates previous outputs using template syntax {{name.stdout}}, {{block_0.stdout}}, etc.
    /// Trailing newlines are stripped from interpolated values.
    pub fn interpolate_variables(code: &str, outputs: &HashMap<String, ExecutionResult>) -> String {
        Self::interpolate_variables_with_context(code, outputs, &HashMap::new(), &HashMap::new())
    }

    /// Auto-injects environment variables for outputs, params, and table selections
    pub fn build_environment_variables_with_context(
        outputs: &HashMap<String, ExecutionResult>,
        params: &HashMap<String, String>,
        tables: &HashMap<String, crate::table::TableData>,
    ) -> HashMap<String, String> {
        let mut envs = HashMap::new();

        // 1. Code block execution outputs
        for (key, res) in outputs {
            let clean_key: String = key
                .chars()
                .map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_uppercase() } else { '_' })
                .collect();
            let clean_stdout = crate::ansi::strip_ansi_codes(&res.stdout);
            let clean_stderr = crate::ansi::strip_ansi_codes(&res.stderr);
            let stdout_trimmed = clean_stdout.trim_end_matches(&['\r', '\n'][..]).to_string();
            let stderr_trimmed = clean_stderr.trim_end_matches(&['\r', '\n'][..]).to_string();
            let exit_code_str = res.exit_code.to_string();

            envs.insert(format!("SKAT_{}_STDOUT", clean_key), stdout_trimmed.clone());
            envs.insert(format!("SKAT_{}_STDERR", clean_key), stderr_trimmed);
            envs.insert(format!("SKAT_{}_EXIT_CODE", clean_key), exit_code_str);
            envs.insert(format!("SKAT_{}", clean_key), stdout_trimmed);

            // Also check if fleet output
            if let Some(fleet) = crate::mesh::FleetOutput::parse(&res.stdout) {
                for (idx, h) in fleet.hosts.iter().enumerate() {
                    let h_stdout = crate::ansi::strip_ansi_codes(&h.stdout).trim_end_matches(&['\r', '\n'][..]).to_string();
                    let h_stderr = crate::ansi::strip_ansi_codes(&h.stderr).trim_end_matches(&['\r', '\n'][..]).to_string();
                    envs.insert(format!("SKAT_{}_{}_STDOUT", clean_key, idx), h_stdout.clone());
                    envs.insert(format!("SKAT_{}_{}_STDERR", clean_key, idx), h_stderr);
                    envs.insert(format!("SKAT_{}_{}_EXIT_CODE", clean_key, idx), h.exit_code.to_string());
                    envs.insert(format!("SKAT_{}_{}_HOST", clean_key, idx), h.host.clone());
                    envs.insert(format!("SKAT_{}_{}", clean_key, idx), h_stdout);
                }
                envs.insert(format!("SKAT_{}_FAILED", clean_key), fleet.failed_hosts().join(","));
                envs.insert(format!("SKAT_{}_SUCCESS", clean_key), fleet.is_success().to_string());
                envs.insert(format!("SKAT_{}_COUNT", clean_key), fleet.hosts.len().to_string());
            }
        }

        // 2. Form parameters ($SKAT_PARAM_CLUSTER, $SKAT_PARAM_NAMESPACE, etc.)
        for (name, val) in params {
            let clean_name: String = name
                .chars()
                .map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_uppercase() } else { '_' })
                .collect();
            envs.insert(format!("SKAT_PARAM_{}", clean_name), val.clone());
            envs.entry(format!("SKAT_{}", clean_name)).or_insert_with(|| val.clone());
        }

        // 3. Table selections ($SKAT_GET_PODS_SELECTED_NAME, $SKAT_GET_PODS_SELECTED, etc.)
        for (name, tbl) in tables {
            let clean_tbl: String = name
                .chars()
                .map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_uppercase() } else { '_' })
                .collect();

            if let Some(row) = tbl.rows.get(tbl.selected_row) {
                for (col_idx, h) in tbl.headers.iter().enumerate() {
                    if let Some(cell_val) = row.get(col_idx) {
                        let clean_col: String = h
                            .chars()
                            .map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_uppercase() } else { '_' })
                            .collect();
                        envs.insert(format!("SKAT_{}_SELECTED_{}", clean_tbl, clean_col), cell_val.clone());
                    }
                }
            }
            if let Some(row_json) = tbl.get_selected_row_json() {
                envs.insert(format!("SKAT_{}_SELECTED", clean_tbl), row_json);
            }
        }

        envs
    }

    /// Auto-injects environment variables for previous outputs ($SKAT_BLOCK_0_STDOUT, $SKAT_GET_POD_STDOUT, etc.)
    pub fn build_environment_variables(outputs: &HashMap<String, ExecutionResult>) -> HashMap<String, String> {
        Self::build_environment_variables_with_context(outputs, &HashMap::new(), &HashMap::new())
    }

    /// Auto-injects environment variables for previous outputs and parameters
    pub fn build_environment_variables_with_params(
        outputs: &HashMap<String, ExecutionResult>,
        params: &HashMap<String, String>,
    ) -> HashMap<String, String> {
        Self::build_environment_variables_with_context(outputs, params, &HashMap::new())
    }

    /// Collects existing output blocks from buffer into an outputs map
    pub fn collect_existing_outputs(buffer: &crate::editor::TextBuffer) -> HashMap<String, ExecutionResult> {
        let mut outputs = HashMap::new();
        let blocks = Self::all_code_blocks(buffer);
        for (idx, b) in blocks.into_iter().enumerate() {
            if let Some(out) = b.existing_output {
                let res = ExecutionResult {
                    stdout: out.output_text,
                    stderr: String::new(),
                    exit_code: out.exit_code,
                    timed_out: false,
                };
                outputs.insert(format!("block_{}", idx), res.clone());
                outputs.insert(b.id.clone(), res.clone());
                if let Some(name) = b.name {
                    outputs.insert(name, res);
                }
            }
        }
        outputs
    }

    /// Extracts referenced block names/IDs from template {{name.field}}, {{name[0].field}}, and $SKAT_NAME_STDOUT
    pub fn extract_dependencies(code: &str) -> Vec<String> {
        let mut deps: Vec<String> = Vec::new();
        for cap in DEP_TEMPLATE_RE.captures_iter(code) {
            let name = cap[1].to_string();
            if name.eq_ignore_ascii_case("params") || name.eq_ignore_ascii_case("param") {
                continue;
            }
            if !deps.iter().any(|d| d.eq_ignore_ascii_case(&name)) {
                deps.push(name);
            }
        }

        for cap in DEP_ENV_RE.captures_iter(code) {
            let name = cap[1].to_lowercase();
            if name.starts_with("param_") || name == "param" {
                continue;
            }
            if !deps.iter().any(|d| d.eq_ignore_ascii_case(&name)) {
                deps.push(name);
            }
        }

        deps
    }

    /// Resolves dependency block indices for each code block
    pub fn resolve_block_dependencies(blocks: &[CodeBlock]) -> Result<Vec<(usize, Vec<usize>)>, String> {
        let mut result = Vec::new();

        for (i, block) in blocks.iter().enumerate() {
            let deps = Self::extract_dependencies(&block.code);
            let mut dep_indices = Vec::new();

            for dep in deps {
                let found_idx = blocks.iter().enumerate().position(|(idx, b)| {
                    b.name.as_deref().map(|n| n.eq_ignore_ascii_case(&dep)).unwrap_or(false)
                        || b.id.eq_ignore_ascii_case(&dep)
                        || format!("block_{}", idx).eq_ignore_ascii_case(&dep)
                });

                if let Some(idx) = found_idx {
                    if !dep_indices.contains(&idx) {
                        dep_indices.push(idx);
                    }
                } else {
                    return Err(format!(
                        "Block '{}' references unknown block '{}'",
                        block.name.as_deref().unwrap_or(&block.id),
                        dep
                    ));
                }
            }

            result.push((i, dep_indices));
        }

        Ok(result)
    }

    /// Returns topological execution order of blocks or an error if a cycle exists
    pub fn resolve_execution_order(blocks: &[CodeBlock]) -> Result<Vec<usize>, String> {
        let dependencies = Self::resolve_block_dependencies(blocks)?;
        let mut in_degree = vec![0; blocks.len()];
        let mut adj = vec![Vec::new(); blocks.len()];

        for (node, deps) in &dependencies {
            for &dep in deps {
                adj[dep].push(*node);
                in_degree[*node] += 1;
            }
        }

        let mut queue = VecDeque::new();
        for (i, &deg) in in_degree.iter().enumerate() {
            if deg == 0 {
                queue.push_back(i);
            }
        }

        let mut order = Vec::new();
        while let Some(u) = queue.pop_front() {
            order.push(u);
            for &v in &adj[u] {
                in_degree[v] -= 1;
                if in_degree[v] == 0 {
                    queue.push_back(v);
                }
            }
        }

        if order.len() != blocks.len() {
            return Err("Circular dependency detected between code blocks".to_string());
        }

        Ok(order)
    }

    /// Executes a code block with real-time output streaming and PTY support
    pub async fn execute_streaming<FChunk, FEnd>(
        block: &CodeBlock,
        working_dir: &Path,
        env_vars: &HashMap<String, String>,
        on_chunk: FChunk,
        on_end: FEnd,
    ) -> Result<StreamingHandle>
    where
        FChunk: FnMut(String) + Send + 'static,
        FEnd: FnOnce(ExecutionResult) + Send + 'static,
    {
        Self::execute_streaming_with_pool(
            block,
            working_dir,
            env_vars,
            crate::mesh::global_ssh_pool(),
            None,
            on_chunk,
            on_end,
        )
        .await
    }

    /// Executes a code block with explicit SSH session pool and optional document buffer
    /// Executes a code block with explicit SSH session pool and optional document buffer
    pub async fn execute_streaming_with_pool<FChunk, FEnd>(
        block: &CodeBlock,
        working_dir: &Path,
        env_vars: &HashMap<String, String>,
        pool: &crate::mesh::SshSessionPool,
        buffer: Option<&crate::editor::TextBuffer>,
        mut on_chunk: FChunk,
        on_end: FEnd,
    ) -> Result<StreamingHandle>
    where
        FChunk: FnMut(String) + Send + 'static,
        FEnd: FnOnce(ExecutionResult) + Send + 'static,
    {
        // 0. Secret vault initialization and resolution
        let mut vault = crate::vault::SecretVault::with_workspace(working_dir);
        if let Some(buf) = buffer {
            for param in crate::form::FormParser::find_all(buf) {
                if param.param_type == crate::form::FormParamType::Secret && !param.value.is_empty() {
                    vault.insert(param.name, param.value);
                }
            }
            if let Ok(pass) = std::env::var("SKAT_VAULT_PASSPHRASE") {
                let _ = vault.load_from_frontmatter(&buf.get_text(), &pass);
            }
        }

        // Interpolate secrets into block code
        let interpolated_code = vault.interpolate_secrets_with_fallback(&block.code, Some(env_vars));
        let mut block_to_run = block.clone();
        block_to_run.code = interpolated_code;
        if let Some(t) = &block_to_run.target {
            block_to_run.target = Some(vault.interpolate_secrets_with_fallback(t, Some(env_vars)));
        }
        if let Some(p) = &block_to_run.proxy {
            block_to_run.proxy = Some(vault.interpolate_secrets_with_fallback(p, Some(env_vars)));
        }

        let mut effective_envs = env_vars.clone();
        if block.dry_run {
            effective_envs.insert("SKAT_DRY_RUN".to_string(), "1".to_string());
        }

        let vault_chunk = vault.clone();
        let mut masked_on_chunk = move |chunk: String| {
            let masked = vault_chunk.mask(&chunk);
            on_chunk(masked);
        };

        let vault_end = vault.clone();
        let working_dir_buf = working_dir.to_path_buf();
        let block_id = block.id.clone();
        let doc_path = "document.md".to_string();
        let target_hosts = if let Some(t) = &block_to_run.target {
            vec![t.clone()]
        } else {
            vec!["localhost".to_string()]
        };
        let sanitized_cmd = vault.mask(&block.code);
        let start_time = std::time::Instant::now();

        let masked_on_end = move |mut res: ExecutionResult| {
            res.stdout = vault_end.mask(&res.stdout);
            res.stderr = vault_end.mask(&res.stderr);
            let duration_ms = start_time.elapsed().as_millis() as u64;

            // Record execution to immutable audit trail (.skat/audit.log)
            if let Ok(mut recorder) = crate::audit::AuditRecorder::new(&working_dir_buf) {
                let _ = recorder.record(
                    &doc_path,
                    &block_id,
                    "local",
                    target_hosts,
                    &sanitized_cmd,
                    HashMap::new(),
                    res.exit_code,
                    duration_ms,
                    &res.stdout,
                );
            }

            on_end(res);
        };

        // Route targeted code blocks to SSH / fleet mesh execution
        if let Some(target) = &block_to_run.target {
            return crate::mesh::execute_targeted_streaming(
                pool.clone(),
                target,
                block_to_run.proxy.as_deref(),
                block_to_run.parallel,
                buffer,
                working_dir,
                block_to_run.code.clone(),
                block_to_run.language.clone(),
                effective_envs,
                masked_on_chunk,
                masked_on_end,
            )
            .await;
        }

        let lang = block_to_run.language.to_lowercase();
        let code = block_to_run.code.clone();

        // 1. Try PTY execution
        let pty_system = portable_pty::native_pty_system();
        let pair_res = pty_system.openpty(portable_pty::PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        });

        if let Ok(pair) = pair_res {
            let mut cmd = match lang.as_str() {
                "python" | "py" | "python3" => {
                    let mut c = portable_pty::CommandBuilder::new("python3");
                    c.args(["-c", &code]);
                    c
                }
                "node" | "js" | "javascript" => {
                    let mut c = portable_pty::CommandBuilder::new("node");
                    c.args(["-e", &code]);
                    c
                }
                "zsh" => {
                    let mut c = portable_pty::CommandBuilder::new("zsh");
                    c.args(["-c", &code]);
                    c
                }
                "sh" => {
                    let mut c = portable_pty::CommandBuilder::new("sh");
                    c.args(["-c", &code]);
                    c
                }
                _ => {
                    let mut c = portable_pty::CommandBuilder::new("bash");
                    c.args(["-c", &code]);
                    c
                }
            };

            cmd.cwd(working_dir);
            for (k, v) in &effective_envs {
                cmd.env(k, v);
            }
            cmd.env("TERM", "xterm-256color");
            cmd.env("COLORTERM", "truecolor");

            if let Ok(mut child) = pair.slave.spawn_command(cmd) {
                drop(pair.slave);
                let mut reader = pair.master.try_clone_reader()?;
                let mut writer = pair.master.take_writer()?;
                drop(pair.master);

                let (stdin_tx, mut stdin_rx) = tokio::sync::mpsc::unbounded_channel::<Vec<u8>>();
                let (cancel_tx, mut cancel_rx) = tokio::sync::oneshot::channel::<()>();

                let (chunk_tx, mut chunk_rx) = tokio::sync::mpsc::unbounded_channel::<Vec<u8>>();
                let (done_tx, mut done_rx) = tokio::sync::oneshot::channel::<()>();

                // Spawn reader thread with 8KB buffer for streaming throughput
                std::thread::spawn(move || {
                    let mut buf = [0u8; 8192];
                    loop {
                        match reader.read(&mut buf) {
                            Ok(0) => break,
                            Ok(n) => {
                                if chunk_tx.send(buf[..n].to_vec()).is_err() {
                                    break;
                                }
                            }
                            Err(_) => break,
                        }
                    }
                    let _ = done_tx.send(());
                });

                // Spawn stdin writer thread
                std::thread::spawn(move || {
                    while let Some(bytes) = stdin_rx.blocking_recv() {
                        if writer.write_all(&bytes).is_err() || writer.flush().is_err() {
                            break;
                        }
                    }
                });

                // Supervision task
                tokio::spawn(async move {
                    let mut full_output = Vec::new();
                    let mut interrupted = false;
                    let mut timed_out = false;
                    let mut utf8_carry = Vec::new();
                    let start_time = tokio::time::Instant::now();
                    let timeout_duration = Duration::from_secs(60);

                    loop {
                        tokio::select! {
                            _ = &mut cancel_rx => {
                                interrupted = true;
                                let _ = child.kill();
                                break;
                            }
                            Some(bytes) = chunk_rx.recv() => {
                                full_output.extend_from_slice(&bytes);
                                let mut to_decode = std::mem::take(&mut utf8_carry);
                                to_decode.extend_from_slice(&bytes);
                                match std::str::from_utf8(&to_decode) {
                                    Ok(valid_str) => {
                                        masked_on_chunk(valid_str.to_string());
                                    }
                                    Err(e) => {
                                        let valid_idx = e.valid_up_to();
                                        let valid_text = String::from_utf8_lossy(&to_decode[..valid_idx]).to_string();
                                        utf8_carry.extend_from_slice(&to_decode[valid_idx..]);
                                        if !valid_text.is_empty() {
                                            masked_on_chunk(valid_text);
                                        }
                                    }
                                }
                            }
                            _ = &mut done_rx => {
                                break;
                            }
                            _ = tokio::time::sleep_until(start_time + timeout_duration) => {
                                timed_out = true;
                                let _ = child.kill();
                                break;
                            }
                        }
                    }

                    while let Ok(bytes) = chunk_rx.try_recv() {
                        full_output.extend_from_slice(&bytes);
                        let mut to_decode = std::mem::take(&mut utf8_carry);
                        to_decode.extend_from_slice(&bytes);
                        match std::str::from_utf8(&to_decode) {
                            Ok(valid_str) => {
                                masked_on_chunk(valid_str.to_string());
                            }
                            Err(e) => {
                                let valid_idx = e.valid_up_to();
                                let valid_text = String::from_utf8_lossy(&to_decode[..valid_idx]).to_string();
                                utf8_carry.extend_from_slice(&to_decode[valid_idx..]);
                                if !valid_text.is_empty() {
                                    masked_on_chunk(valid_text);
                                }
                            }
                        }
                    }

                    if !utf8_carry.is_empty() {
                        let text = String::from_utf8_lossy(&utf8_carry).to_string();
                        masked_on_chunk(text);
                    }

                    let status = tokio::task::spawn_blocking(move || child.wait())
                        .await
                        .unwrap_or_else(|_| Err(std::io::Error::other("wait error")));
                    let exit_code = if interrupted {
                        130
                    } else if timed_out {
                        -1
                    } else {
                        status.map(|s| s.exit_code() as i32).unwrap_or(-1)
                    };

                    let stdout = String::from_utf8_lossy(&full_output).to_string();
                    let stderr = if timed_out {
                        "Execution timed out after 60 seconds.".to_string()
                    } else {
                        String::new()
                    };

                    masked_on_end(ExecutionResult {
                        stdout,
                        stderr,
                        exit_code,
                        timed_out,
                    });
                });

                return Ok(StreamingHandle {
                    stdin_tx,
                    cancel_tx: Some(cancel_tx),
                });
            }
        }

        Self::execute_streaming_fallback(&block_to_run, working_dir, &effective_envs, masked_on_chunk, masked_on_end).await
    }

    async fn execute_streaming_fallback<FChunk, FEnd>(
        block: &CodeBlock,
        working_dir: &Path,
        env_vars: &HashMap<String, String>,
        mut on_chunk: FChunk,
        on_end: FEnd,
    ) -> Result<StreamingHandle>
    where
        FChunk: FnMut(String) + Send + 'static,
        FEnd: FnOnce(ExecutionResult) + Send + 'static,
    {
        let lang = block.language.to_lowercase();
        let code = block.code.clone();

        let mut cmd = match lang.as_str() {
            "python" | "py" | "python3" => {
                let mut c = Command::new("python3");
                c.arg("-c").arg(&code);
                c
            }
            "node" | "js" | "javascript" => {
                let mut c = Command::new("node");
                c.arg("-e").arg(&code);
                c
            }
            "zsh" => {
                let mut c = Command::new("zsh");
                c.arg("-c").arg(&code);
                c
            }
            "sh" => {
                let mut c = Command::new("sh");
                c.arg("-c").arg(&code);
                c
            }
            _ => {
                let mut c = Command::new("bash");
                c.arg("-c").arg(&code);
                c
            }
        };

        cmd.current_dir(working_dir);
        for (k, v) in env_vars {
            cmd.env(k, v);
        }
        cmd.env("TERM", "xterm-256color");
        cmd.env("COLORTERM", "truecolor");
        cmd.stdin(Stdio::piped());
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        let mut child = cmd.spawn().with_context(|| format!("Failed to spawn command for language {}", lang))?;
        let mut stdin = child.stdin.take();
        let mut stdout = child.stdout.take();
        let mut stderr = child.stderr.take();

        let (stdin_tx, mut stdin_rx) = tokio::sync::mpsc::unbounded_channel::<Vec<u8>>();
        let (cancel_tx, mut cancel_rx) = tokio::sync::oneshot::channel::<()>();

        if let Some(mut sin) = stdin.take() {
            tokio::spawn(async move {
                while let Some(bytes) = stdin_rx.recv().await {
                    if sin.write_all(&bytes).await.is_err() || sin.flush().await.is_err() {
                        break;
                    }
                }
            });
        }

        tokio::spawn(async move {
            let mut full_stdout = Vec::new();
            let mut full_stderr = Vec::new();
            let mut interrupted = false;
            let mut timed_out = false;

            let mut stdout_buf = [0u8; 8192];
            let mut stderr_buf = [0u8; 8192];
            let mut stdout_carry = Vec::new();
            let mut stderr_carry = Vec::new();
            let mut stdout_closed = stdout.is_none();
            let mut stderr_closed = stderr.is_none();

            let start_time = tokio::time::Instant::now();
            let timeout_duration = Duration::from_secs(60);

            loop {
                if stdout_closed && stderr_closed {
                    break;
                }

                tokio::select! {
                    _ = &mut cancel_rx => {
                        interrupted = true;
                        let _ = child.kill().await;
                        break;
                    }
                    res = async {
                        if let Some(ref mut out) = stdout {
                            out.read(&mut stdout_buf).await
                        } else {
                            std::future::pending().await
                        }
                    }, if !stdout_closed => {
                        match res {
                            Ok(0) | Err(_) => stdout_closed = true,
                            Ok(n) => {
                                full_stdout.extend_from_slice(&stdout_buf[..n]);
                                let mut to_decode = std::mem::take(&mut stdout_carry);
                                to_decode.extend_from_slice(&stdout_buf[..n]);
                                match std::str::from_utf8(&to_decode) {
                                    Ok(valid_str) => {
                                        on_chunk(valid_str.to_string());
                                    }
                                    Err(e) => {
                                        let valid_idx = e.valid_up_to();
                                        let valid_text = String::from_utf8_lossy(&to_decode[..valid_idx]).to_string();
                                        stdout_carry.extend_from_slice(&to_decode[valid_idx..]);
                                        if !valid_text.is_empty() {
                                            on_chunk(valid_text);
                                        }
                                    }
                                }
                            }
                        }
                    }
                    res = async {
                        if let Some(ref mut err) = stderr {
                            err.read(&mut stderr_buf).await
                        } else {
                            std::future::pending().await
                        }
                    }, if !stderr_closed => {
                        match res {
                            Ok(0) | Err(_) => stderr_closed = true,
                            Ok(n) => {
                                full_stderr.extend_from_slice(&stderr_buf[..n]);
                                let mut to_decode = std::mem::take(&mut stderr_carry);
                                to_decode.extend_from_slice(&stderr_buf[..n]);
                                match std::str::from_utf8(&to_decode) {
                                    Ok(valid_str) => {
                                        on_chunk(valid_str.to_string());
                                    }
                                    Err(e) => {
                                        let valid_idx = e.valid_up_to();
                                        let valid_text = String::from_utf8_lossy(&to_decode[..valid_idx]).to_string();
                                        stderr_carry.extend_from_slice(&to_decode[valid_idx..]);
                                        if !valid_text.is_empty() {
                                            on_chunk(valid_text);
                                        }
                                    }
                                }
                            }
                        }
                    }
                    _ = tokio::time::sleep_until(start_time + timeout_duration) => {
                        timed_out = true;
                        let _ = child.kill().await;
                        break;
                    }
                }
            }

            if !stdout_carry.is_empty() {
                let text = String::from_utf8_lossy(&stdout_carry).to_string();
                on_chunk(text);
            }
            if !stderr_carry.is_empty() {
                let text = String::from_utf8_lossy(&stderr_carry).to_string();
                on_chunk(text);
            }

            let status = child.wait().await;
            let exit_code = if interrupted {
                130
            } else if timed_out {
                -1
            } else {
                status.map(|s| s.code().unwrap_or(-1)).unwrap_or(-1)
            };

            let stdout = String::from_utf8_lossy(&full_stdout).to_string();
            let stderr = if timed_out {
                "Execution timed out after 60 seconds.".to_string()
            } else {
                String::from_utf8_lossy(&full_stderr).to_string()
            };

            on_end(ExecutionResult {
                stdout,
                stderr,
                exit_code,
                timed_out,
            });
        });

        Ok(StreamingHandle {
            stdin_tx,
            cancel_tx: Some(cancel_tx),
        })
    }

    /// Executes a code block with environment variables and working directory
    pub async fn execute_with_env(
        block: &CodeBlock,
        working_dir: &Path,
        env_vars: &HashMap<String, String>,
    ) -> Result<ExecutionResult> {
        let (res_tx, res_rx) = tokio::sync::oneshot::channel();
        let _handle = Self::execute_streaming(
            block,
            working_dir,
            env_vars,
            |_chunk| {},
            move |result| {
                let _ = res_tx.send(result);
            },
        ).await?;

        res_rx.await.context("Execution task dropped without result")
    }

    /// Executes the code block asynchronously with a timeout
    pub async fn execute(block: &CodeBlock, working_dir: &Path) -> Result<ExecutionResult> {
        Self::execute_with_env(block, working_dir, &HashMap::new()).await
    }

    /// Executes a sequential playbook of code blocks with context (outputs, params, tables)
    pub async fn execute_playbook_with_context<F1, F2>(
        blocks: &[CodeBlock],
        working_dir: &Path,
        initial_outputs: HashMap<String, ExecutionResult>,
        params: HashMap<String, String>,
        mut tables: HashMap<String, crate::table::TableData>,
        mut on_step_start: F1,
        mut on_step_finish: F2,
    ) -> Result<PlaybookReport>
    where
        F1: FnMut(usize, &CodeBlock),
        F2: FnMut(usize, &CodeBlock, &ExecutionResult),
    {
        let mut outputs = initial_outputs;

        for (i, block) in blocks.iter().enumerate() {
            // Interpolate previous outputs, parameters, table selections, and targets
            let interpolated_code = Self::interpolate_variables_with_context(&block.code, &outputs, &params, &tables);
            let mut block_to_run = block.clone();
            block_to_run.code = interpolated_code;
            if let Some(t) = &block.target {
                block_to_run.target = Some(Self::interpolate_variables_with_context(t, &outputs, &params, &tables));
            }
            if let Some(p) = &block.proxy {
                block_to_run.proxy = Some(Self::interpolate_variables_with_context(p, &outputs, &params, &tables));
            }

            let env_vars = Self::build_environment_variables_with_context(&outputs, &params, &tables);

            on_step_start(i, &block_to_run);

            let result = match Self::execute_with_env(&block_to_run, working_dir, &env_vars).await {
                Ok(res) => res,
                Err(err) => {
                    return Ok(PlaybookReport {
                        total_steps: blocks.len(),
                        executed_steps: i + 1,
                        failed_step: Some(i),
                        failed_block_id: Some(block.id.clone()),
                        success: false,
                        error_message: Some(format!(
                            "Step {} ({}) failed with error: {}",
                            i + 1,
                            block.name.as_deref().unwrap_or(&block.id),
                            err
                        )),
                    });
                }
            };

            outputs.insert(format!("block_{}", i), result.clone());
            outputs.insert(block.id.clone(), result.clone());
            if let Some(name) = &block.name {
                outputs.insert(name.clone(), result.clone());
            }
            if let Some(target) = &block_to_run.target
                && let Some(group) = target.strip_prefix('@')
            {
                outputs.insert(group.to_string(), result.clone());
            }

            // If output parses into a table, update tables map
            if let Some(tbl) = crate::table::TableData::parse(&result.stdout) {
                tables.insert(format!("block_{}", i), tbl.clone());
                tables.insert(block.id.clone(), tbl.clone());
                if let Some(name) = &block.name {
                    tables.insert(name.clone(), tbl);
                }
            }

            on_step_finish(i, &block_to_run, &result);

            // Fail-fast safeguard: halt sequence if exit_code != 0
            if result.exit_code != 0 {
                return Ok(PlaybookReport {
                    total_steps: blocks.len(),
                    executed_steps: i + 1,
                    failed_step: Some(i),
                    failed_block_id: Some(block.id.clone()),
                    success: false,
                    error_message: Some(format!(
                        "Step {} ({}) failed with exit code {}",
                        i + 1,
                        block.name.as_deref().unwrap_or(&block.id),
                        result.exit_code
                    )),
                });
            }
        }

        Ok(PlaybookReport {
            total_steps: blocks.len(),
            executed_steps: blocks.len(),
            failed_step: None,
            failed_block_id: None,
            success: true,
            error_message: None,
        })
    }

    /// Executes a sequential playbook of code blocks with fail-fast safeguard and chaining
    pub async fn execute_playbook<F1, F2>(
        blocks: &[CodeBlock],
        working_dir: &Path,
        initial_outputs: HashMap<String, ExecutionResult>,
        on_step_start: F1,
        on_step_finish: F2,
    ) -> Result<PlaybookReport>
    where
        F1: FnMut(usize, &CodeBlock),
        F2: FnMut(usize, &CodeBlock, &ExecutionResult),
    {
        Self::execute_playbook_with_context(
            blocks,
            working_dir,
            initial_outputs,
            HashMap::new(),
            HashMap::new(),
            on_step_start,
            on_step_finish,
        )
        .await
    }

    /// Injects or updates the output block in the text buffer with a given status
    pub fn apply_output_status_for_block(
        buffer: &mut crate::editor::TextBuffer,
        block: &CodeBlock,
        status: OutputStatus,
        text: &str,
    ) {
        let existing = Self::find_following_output_block(buffer, block.end_fence_line);
        let is_collapsed = existing.as_ref().map(|e| e.is_collapsed).unwrap_or(false);

        let header_status = match status {
            OutputStatus::Running => "[running]".to_string(),
            OutputStatus::Exit(code) => format!("[exit: {}]", code),
            OutputStatus::Interrupted => "[interrupted]".to_string(),
        };

        let format_attr = match block.format {
            BlockFormat::Table => " format=table",
            BlockFormat::Chart => " format=chart",
            BlockFormat::Sparkline => " format=sparkline",
            BlockFormat::Text => {
                if crate::table::TableData::parse(text).is_some() {
                    " format=table"
                } else {
                    ""
                }
            }
        };

        let header = if is_collapsed {
            format!("```output {}{} [collapsed]", header_status, format_attr)
        } else {
            format!("```output {}{}", header_status, format_attr)
        };

        let content = if text.trim().is_empty() {
            if matches!(status, OutputStatus::Running) {
                String::new()
            } else {
                "(No output)".to_string()
            }
        } else {
            text.trim_end_matches(&['\r', '\n'][..]).to_string()
        };

        let formatted_output = if content.is_empty() {
            format!("{}\n```", header)
        } else {
            format!("{}\n{}\n```", header, content)
        };

        if let Some(ex) = existing {
            Self::replace_lines(buffer, ex.start_line, ex.end_line, &formatted_output);
        } else {
            let insert_line = block.end_fence_line + 1;
            Self::insert_lines_at(buffer, insert_line, &formatted_output);
        }
    }

    /// Finds a block by ID and applies status and content to its output block
    pub fn apply_streaming_status(
        buffer: &mut crate::editor::TextBuffer,
        block_id: &str,
        status: OutputStatus,
        text: &str,
    ) -> bool {
        if let Some(block) = Self::find_code_block_by_id(buffer, block_id) {
            Self::apply_output_status_for_block(buffer, &block, status, text);
            true
        } else {
            false
        }
    }

    /// Appends an incremental chunk of output to a streaming block
    pub fn append_stream_chunk(
        buffer: &mut crate::editor::TextBuffer,
        block_id: &str,
        chunk: &str,
        accumulated: &mut String,
    ) -> bool {
        accumulated.push_str(chunk);
        let processed = crate::ansi::process_terminal_stream(accumulated);
        Self::apply_streaming_status(buffer, block_id, OutputStatus::Running, &processed)
    }

    /// Injects or updates the output block in the text buffer with custom content
    pub fn apply_output_content(
        buffer: &mut crate::editor::TextBuffer,
        block: &CodeBlock,
        text: &str,
        exit_code: i32,
    ) {
        let status = if exit_code == 130 {
            OutputStatus::Interrupted
        } else {
            OutputStatus::Exit(exit_code)
        };
        Self::apply_output_status_for_block(buffer, block, status, text);
    }

    /// Finds a block by ID and applies execution output to the buffer
    pub fn apply_output_by_id(
        buffer: &mut crate::editor::TextBuffer,
        block_id: &str,
        text: &str,
        exit_code: i32,
    ) -> bool {
        let status = if exit_code == 130 {
            OutputStatus::Interrupted
        } else {
            OutputStatus::Exit(exit_code)
        };
        Self::apply_streaming_status(buffer, block_id, status, text)
    }

    /// Injects or updates the output block in the text buffer
    pub fn apply_output(
        buffer: &mut crate::editor::TextBuffer,
        block: &CodeBlock,
        result: &ExecutionResult,
    ) {
        let mut output_content = String::new();
        if !result.stdout.is_empty() {
            output_content.push_str(&result.stdout);
        }
        if !result.stderr.is_empty() {
            if !output_content.is_empty() && !output_content.ends_with('\n') {
                output_content.push('\n');
            }
            output_content.push_str(&result.stderr);
        }
        if output_content.is_empty() {
            output_content = "(No output)".to_string();
        }

        let status = if result.exit_code == 130 {
            OutputStatus::Interrupted
        } else {
            OutputStatus::Exit(result.exit_code)
        };

        if !Self::apply_streaming_status(buffer, &block.id, status, &output_content) {
            Self::apply_output_status_for_block(buffer, block, status, &output_content);
        }
    }

    /// Toggles the collapsed state of an output block at the cursor
    pub fn toggle_output_collapse(buffer: &mut crate::editor::TextBuffer, cursor_line: usize) -> bool {
        let total_lines = buffer.line_count();
        if total_lines == 0 {
            return false;
        }

        // Find enclosing output block
        let mut start_line = None;
        for i in (0..=cursor_line.min(total_lines - 1)).rev() {
            let l = buffer.line_to_string(i);
            if l.trim().starts_with("```output") {
                start_line = Some(i);
                break;
            }
        }

        let start = match start_line {
            Some(s) => s,
            None => return false,
        };

        let mut end_line = None;
        for j in (start + 1)..total_lines {
            let l = buffer.line_to_string(j);
            if l.trim().starts_with("```") {
                end_line = Some(j);
                break;
            }
        }

        let end = match end_line {
            Some(e) => e,
            None => return false,
        };

        if cursor_line < start || cursor_line > end {
            return false;
        }

        // Toggle [collapsed] in header
        let header = buffer.line_to_string(start);
        let new_header = if header.contains("[collapsed]") {
            header.replace(" [collapsed]", "")
        } else {
            let trimmed = header.trim_end();
            format!("{} [collapsed]", trimmed)
        };

        Self::replace_lines(buffer, start, start, &new_header);
        true
    }

    /// Finds the output block enclosing the given line, if any
    pub fn find_enclosing_output_block(buffer: &crate::editor::TextBuffer, cursor_line: usize) -> Option<OutputBlockSpan> {
        let total_lines = buffer.line_count();
        if total_lines == 0 {
            return None;
        }

        let mut start_line = None;
        for i in (0..=cursor_line.min(total_lines - 1)).rev() {
            let l = buffer.line_to_string(i);
            if l.trim().starts_with("```output") {
                start_line = Some(i);
                break;
            }
        }

        let start = start_line?;

        let mut end_line = None;
        for j in (start + 1)..total_lines {
            let l = buffer.line_to_string(j);
            if l.trim().starts_with("```") {
                end_line = Some(j);
                break;
            }
        }

        let end = end_line?;

        if cursor_line >= start && cursor_line <= end {
            let header = buffer.line_to_string(start);
            let trimmed = header.trim();
            let is_collapsed = trimmed.contains("[collapsed]");
            let format = if trimmed.contains("format=table") {
                BlockFormat::Table
            } else if trimmed.contains("format=chart") || trimmed.contains("format=bar") {
                BlockFormat::Chart
            } else if trimmed.contains("format=sparkline") || trimmed.contains("format=spark") {
                BlockFormat::Sparkline
            } else {
                BlockFormat::Text
            };

            let mut exit_code = 0;
            if trimmed.contains("[interrupted]") {
                exit_code = 130;
            } else if let Some(idx) = trimmed.find("[exit: ") {
                let rest = &trimmed[idx + 7..];
                if let Some(end_idx) = rest.find(']')
                    && let Ok(c) = rest[..end_idx].trim().parse::<i32>()
                {
                    exit_code = c;
                }
            }

            let mut out_lines = Vec::new();
            for l in (start + 1)..end {
                out_lines.push(buffer.line_to_string(l));
            }
            let output_text = out_lines.join("\n");

            Some(OutputBlockSpan {
                start_line: start,
                end_line: end,
                is_collapsed,
                exit_code,
                output_text,
                format,
            })
        } else {
            None
        }
    }

    fn replace_lines(buffer: &mut crate::editor::TextBuffer, from_line: usize, to_line: usize, new_text: &str) {
        let total_lines = buffer.line_count();
        if from_line >= total_lines {
            return;
        }
        let end_line = to_line.min(total_lines - 1);

        let start_char = buffer.rope.line_to_char(from_line);
        let end_char = if end_line + 1 < total_lines {
            buffer.rope.line_to_char(end_line + 1)
        } else {
            buffer.rope.len_chars()
        };

        buffer.rope.remove(start_char..end_char);
        let mut insert_str = new_text.to_string();
        if !insert_str.ends_with('\n') && end_line + 1 < total_lines {
            insert_str.push('\n');
        }
        buffer.rope.insert(start_char, &insert_str);
        buffer.is_dirty = true;
    }

    fn insert_lines_at(buffer: &mut crate::editor::TextBuffer, at_line: usize, text: &str) {
        let total_lines = buffer.line_count();
        let insert_char = if at_line < total_lines {
            buffer.rope.line_to_char(at_line)
        } else {
            buffer.rope.len_chars()
        };

        let mut insert_str = String::from("\n");
        insert_str.push_str(text);
        if at_line < total_lines {
            insert_str.push('\n');
        }

        buffer.rope.insert(insert_char, &insert_str);
        buffer.is_dirty = true;
    }
}
