use anyhow::{Context, Result};
use std::path::Path;
use std::process::Stdio;
use std::time::Duration;
use tokio::io::AsyncReadExt;
use tokio::process::Command;
use tokio::time::timeout;

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct CodeBlock {
    pub language: String,
    pub code: String,
    pub start_fence_line: usize,
    pub end_fence_line: usize,
    pub existing_output: Option<OutputBlockSpan>,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct OutputBlockSpan {
    pub start_line: usize,
    pub end_line: usize,
    pub is_collapsed: bool,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct ExecutionResult {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
    pub timed_out: bool,
}

pub struct Runner;

impl Runner {
    /// Detects if cursor is inside or immediately adjacent to a fenced code block
    pub fn find_code_block(buffer: &crate::editor::TextBuffer, cursor_line: usize) -> Option<CodeBlock> {
        let total_lines = buffer.line_count();
        if total_lines == 0 {
            return None;
        }

        // Check if cursor is on or inside a code block
        // Scan backwards to find opening ```
        let mut start_fence = None;
        let mut lang = String::new();

        for i in (0..=cursor_line.min(total_lines - 1)).rev() {
            let line = buffer.line_to_string(i);
            let trimmed = line.trim();

            if trimmed.starts_with("```") {
                // If it's an output block, ignore it as code
                if trimmed.starts_with("```output") {
                    continue;
                }

                // If this is an opening fence
                let l = trimmed.trim_start_matches('`').trim();
                start_fence = Some(i);
                lang = l.to_string();
                break;
            }
        }

        let start = match start_fence {
            Some(s) => s,
            None => return None,
        };

        // Scan forward from start+1 to find closing ```
        let mut end_fence = None;
        for i in (start + 1)..total_lines {
            let line = buffer.line_to_string(i);
            let trimmed = line.trim();
            if trimmed.starts_with("```") {
                end_fence = Some(i);
                break;
            }
        }

        let end = match end_fence {
            Some(e) => e,
            None => return None,
        };

        // Check if cursor is within [start, end]
        if cursor_line < start || cursor_line > end {
            return None;
        }

        // Collect code lines
        let mut code_lines = Vec::new();
        for i in (start + 1)..end {
            code_lines.push(buffer.line_to_string(i));
        }
        let code = code_lines.join("\n");

        // Check if an output block follows end_fence
        let existing_output = Self::find_following_output_block(buffer, end);

        Some(CodeBlock {
            language: if lang.is_empty() { "bash".to_string() } else { lang },
            code,
            start_fence_line: start,
            end_fence_line: end,
            existing_output,
        })
    }

    /// Finds if an ```output block immediately follows a code block
    fn find_following_output_block(buffer: &crate::editor::TextBuffer, end_fence_line: usize) -> Option<OutputBlockSpan> {
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
            // Find closing fence
            for j in (check_line + 1)..total_lines {
                let l = buffer.line_to_string(j);
                if l.trim().starts_with("```") {
                    return Some(OutputBlockSpan {
                        start_line: check_line,
                        end_line: j,
                        is_collapsed,
                    });
                }
            }
        }

        None
    }

    /// Executes the code block asynchronously with a timeout
    pub async fn execute(block: &CodeBlock, working_dir: &Path) -> Result<ExecutionResult> {
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
                // Default to bash
                let mut c = Command::new("bash");
                c.arg("-c").arg(&code);
                c
            }
        };

        cmd.current_dir(working_dir);
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        let mut child = cmd.spawn().with_context(|| format!("Failed to spawn command for language {}", lang))?;

        let run_future = async {
            let mut stdout_buf = Vec::new();
            let mut stderr_buf = Vec::new();

            if let Some(mut stdout) = child.stdout.take() {
                let _ = stdout.read_to_end(&mut stdout_buf).await;
            }
            if let Some(mut stderr) = child.stderr.take() {
                let _ = stderr.read_to_end(&mut stderr_buf).await;
            }

            let status = child.wait().await?;
            let exit_code = status.code().unwrap_or(-1);

            let stdout = String::from_utf8_lossy(&stdout_buf).to_string();
            let stderr = String::from_utf8_lossy(&stderr_buf).to_string();

            Ok::<ExecutionResult, anyhow::Error>(ExecutionResult {
                stdout,
                stderr,
                exit_code,
                timed_out: false,
            })
        };

        match timeout(Duration::from_secs(30), run_future).await {
            Ok(res) => res,
            Err(_) => {
                let _ = child.kill().await;
                Ok(ExecutionResult {
                    stdout: String::new(),
                    stderr: "Execution timed out after 30 seconds.".to_string(),
                    exit_code: -1,
                    timed_out: true,
                })
            }
        }
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

        let formatted_output = format!(
            "```output [exit: {}]\n{}\n```",
            result.exit_code,
            output_content.trim_end()
        );

        if let Some(existing) = &block.existing_output {
            // Replace lines from existing.start_line to existing.end_line
            Self::replace_lines(buffer, existing.start_line, existing.end_line, &formatted_output);
        } else {
            // Insert after end_fence_line
            let insert_line = block.end_fence_line + 1;
            Self::insert_lines_at(buffer, insert_line, &formatted_output);
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
