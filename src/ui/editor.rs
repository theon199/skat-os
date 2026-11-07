use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use crate::app::{App, FocusPane};
use crate::ui::theme::Theme;

pub struct EditorView;

impl EditorView {
    pub fn render(app: &App, frame: &mut Frame, area: Rect) {
        let is_focused = app.focus == FocusPane::Editor;
        let is_dirty = app.editor.is_dirty();

        let title_text = match &app.editor.file_path {
            Some(path) => {
                let name = path
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| "Untitled".to_string());
                if is_dirty {
                    format!(" 📄 {} [MODIFIED] ", name)
                } else {
                    format!(" 📄 {} ", name)
                }
            }
            None => {
                if is_dirty {
                    " 📄 Untitled [MODIFIED] ".to_string()
                } else {
                    " 📄 Untitled ".to_string()
                }
            }
        };

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(if is_focused {
                Theme::TITLE_ACTIVE
            } else {
                Theme::TITLE_INACTIVE
            })
            .title(Span::styled(
                title_text,
                if is_dirty {
                    Theme::STATUS_BADGE_DIRTY
                } else if is_focused {
                    Theme::TITLE_ACTIVE
                } else {
                    Theme::TITLE_INACTIVE
                },
            ));

        let inner_area = block.inner(area);
        frame.render_widget(block, area);

        if inner_area.height == 0 || inner_area.width == 0 {
            return;
        }

        let total_lines = app.editor.line_count();
        let gutter_width = format!("{}", total_lines).len().max(2) + 2; // e.g. " 12 │ "

        let visible_height = inner_area.height as usize;
        let visible_width = (inner_area.width as usize).saturating_sub(gutter_width);

        let scroll_top = app.editor.buffer.scroll_top;
        let scroll_left = app.editor.buffer.scroll_left;

        // Render visible lines with syntax styling
        let mut rendered_lines = Vec::new();
        let mut inside_code = false;
        let mut inside_output = false;
        let mut is_output_collapsed = false;

        // First pass: determine state at scroll_top
        for i in 0..scroll_top {
            let line = app.editor.buffer.line_to_string(i);
            let trimmed = line.trim();
            if trimmed.starts_with("```output") {
                inside_output = true;
                inside_code = false;
                is_output_collapsed = trimmed.contains("[collapsed]");
            } else if trimmed.starts_with("```") {
                if inside_output || inside_code {
                    inside_output = false;
                    inside_code = false;
                    is_output_collapsed = false;
                } else {
                    inside_code = true;
                }
            }
        }

        // Second pass: render visible slice
        for i in scroll_top..(scroll_top + visible_height).min(total_lines) {
            let line_idx = i;
            let raw_line = app.editor.buffer.line_to_string(line_idx);
            let trimmed = raw_line.trim();

            let gutter_text = format!("{:>width$} │ ", line_idx + 1, width = gutter_width - 3);
            let mut spans: Vec<Span<'static>> = vec![Span::styled(gutter_text, Theme::TITLE_INACTIVE)];

            // Check block transitions
            if trimmed.starts_with("```output") {
                inside_output = true;
                inside_code = false;
                is_output_collapsed = trimmed.contains("[collapsed]");

                let badge = if is_output_collapsed {
                    format!("▶ {} (Press Ctrl+O to expand)", trimmed)
                } else {
                    format!("▼ {}", trimmed)
                };
                spans.push(Span::styled(badge, Theme::MD_OUTPUT_FENCE));
                rendered_lines.push(Line::from(spans));
                continue;
            } else if trimmed.starts_with("```") {
                if inside_output {
                    inside_output = false;
                    is_output_collapsed = false;
                    spans.push(Span::styled(trimmed.to_string(), Theme::MD_OUTPUT_FENCE));
                    rendered_lines.push(Line::from(spans));
                    continue;
                } else if inside_code {
                    inside_code = false;
                    spans.push(Span::styled(trimmed.to_string(), Theme::MD_CODE_FENCE));
                    rendered_lines.push(Line::from(spans));
                    continue;
                } else {
                    inside_code = true;
                    spans.push(Span::styled(raw_line.clone(), Theme::MD_CODE_FENCE));
                    rendered_lines.push(Line::from(spans));
                    continue;
                }
            }

            // If output is collapsed, hide content lines
            if inside_output && is_output_collapsed {
                spans.push(Span::styled("  ... (hidden) ...", Theme::TITLE_INACTIVE));
                rendered_lines.push(Line::from(spans));
                continue;
            }

            // Slice horizontal scroll
            let visible_content = if scroll_left < raw_line.chars().count() {
                raw_line.chars().skip(scroll_left).take(visible_width).collect::<String>()
            } else {
                String::new()
            };

            if inside_output {
                spans.push(Span::styled(visible_content, Theme::MD_OUTPUT_LINE));
            } else if inside_code {
                spans.push(Span::styled(visible_content, Theme::MD_CODE_LINE));
            } else {
                let line_spans = Self::parse_markdown_spans(&visible_content);
                spans.extend(line_spans);
            }

            rendered_lines.push(Line::from(spans));
        }

        let paragraph = Paragraph::new(rendered_lines);
        frame.render_widget(paragraph, inner_area);

        // Hardware cursor positioning
        if is_focused {
            let cursor_line = app.editor.buffer.cursor_line;
            let cursor_col = app.editor.buffer.cursor_col;

            if cursor_line >= scroll_top && cursor_line < scroll_top + visible_height {
                let y = inner_area.y + (cursor_line - scroll_top) as u16;
                let rel_col = cursor_col.saturating_sub(scroll_left);
                if rel_col < visible_width {
                    let x = inner_area.x + gutter_width as u16 + rel_col as u16;
                    frame.set_cursor_position((x, y));
                }
            }
        }
    }

    fn parse_markdown_spans(content: &str) -> Vec<Span<'static>> {
        let trimmed = content.trim_start();

        if trimmed.starts_with("# ") {
            return vec![Span::styled(content.to_string(), Theme::MD_H1)];
        } else if trimmed.starts_with("## ") {
            return vec![Span::styled(content.to_string(), Theme::MD_H2)];
        } else if trimmed.starts_with("### ") {
            return vec![Span::styled(content.to_string(), Theme::MD_H3)];
        } else if trimmed.starts_with("> ") {
            return vec![Span::styled(content.to_string(), Theme::MD_BLOCKQUOTE)];
        }

        // Highlight wiki-links [[...]]
        let mut spans = Vec::new();
        let mut remaining = content;

        while let Some(start) = remaining.find("[[") {
            if let Some(end) = remaining[start + 2..].find("]]") {
                let abs_end = start + 2 + end + 2;

                if start > 0 {
                    spans.push(Span::raw(remaining[..start].to_string()));
                }

                spans.push(Span::styled(remaining[start..abs_end].to_string(), Theme::MD_WIKILINK));
                remaining = &remaining[abs_end..];
            } else {
                break;
            }
        }

        if !remaining.is_empty() {
            if trimmed.starts_with("- ") || trimmed.starts_with("* ") {
                let indent_len = content.len() - trimmed.len();
                let (indent, rest) = content.split_at(indent_len);
                spans.push(Span::raw(indent.to_string()));
                spans.push(Span::styled(rest[..2].to_string(), Theme::MD_LIST_BULLET));
                spans.push(Span::raw(rest[2..].to_string()));
            } else {
                spans.push(Span::raw(remaining.to_string()));
            }
        }

        if spans.is_empty() {
            spans.push(Span::raw(content.to_string()));
        }

        spans
    }
}
