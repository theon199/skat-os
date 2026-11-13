use ratatui::layout::{Constraint, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{BarChart, Block, Borders, Cell, Paragraph, Row, Sparkline, Table, Tabs};
use ratatui::Frame;

use crate::app::{App, FocusPane};
use crate::collab::{peer_color, PeerInfo};
use crate::runner::Runner;
use crate::ui::theme::Theme;

enum OutputWidget {
    Table {
        rect: Rect,
        headers: Vec<String>,
        rows: Vec<Vec<String>>,
        selected_row: usize,
    },
    Sparkline {
        rect: Rect,
        values: Vec<u64>,
        max_val: u64,
    },
    BarChart {
        rect: Rect,
        bars: Vec<(String, u64)>,
        max_val: u64,
    },
    Fleet {
        rect: Rect,
        fleet: crate::mesh::FleetOutput,
    },
}

pub struct EditorView;

struct PeerRenderContext<'a> {
    scroll_left: usize,
    visible_width: usize,
    peers: &'a [&'a PeerInfo],
    is_markdown: bool,
    line_idx: usize,
    cursor_line: usize,
    cursor_col: usize,
}

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

        // Detect dynamic canvas output widgets (Tables, Sparklines, BarCharts)
        let blocks = crate::runner::Runner::all_code_blocks(&app.editor.buffer);
        let mut output_widgets = Vec::new();
        let mut widget_line_ranges: Vec<(usize, usize)> = Vec::new();

        for b in &blocks {
            if let Some(out) = &b.existing_output
                && !out.is_collapsed
            {
                let content_start = out.start_line + 1;
                let content_end = out.end_line;
                if content_end > scroll_top && content_start < scroll_top + visible_height && content_end > content_start {
                    let start_y = content_start.saturating_sub(scroll_top);
                    let end_y = content_end.min(scroll_top + visible_height).saturating_sub(scroll_top);
                    if end_y > start_y {
                        let rect = Rect::new(
                            inner_area.x + gutter_width as u16,
                            inner_area.y + start_y as u16,
                            visible_width as u16,
                            (end_y - start_y) as u16,
                        );

                        if let Some(fleet) = app
                            .fleet_states
                            .get(&b.id)
                            .or_else(|| b.name.as_ref().and_then(|n| app.fleet_states.get(n)))
                            .cloned()
                            .or_else(|| crate::mesh::FleetOutput::parse(&out.output_text))
                        {
                            output_widgets.push(OutputWidget::Fleet { rect, fleet });
                            widget_line_ranges.push((content_start, content_end));
                        } else if out.format == crate::runner::BlockFormat::Sparkline {
                            let chart_data = crate::chart::ChartData::parse(&out.output_text);
                            if !chart_data.values.is_empty() {
                                output_widgets.push(OutputWidget::Sparkline {
                                    rect,
                                    values: chart_data.values,
                                    max_val: chart_data.max_value,
                                });
                                widget_line_ranges.push((content_start, content_end));
                            }
                        } else if out.format == crate::runner::BlockFormat::Chart {
                            let chart_data = crate::chart::ChartData::parse(&out.output_text);
                            if !chart_data.bars.is_empty() {
                                output_widgets.push(OutputWidget::BarChart {
                                    rect,
                                    bars: chart_data.bars,
                                    max_val: chart_data.max_value,
                                });
                                widget_line_ranges.push((content_start, content_end));
                            } else if !chart_data.values.is_empty() {
                                output_widgets.push(OutputWidget::Sparkline {
                                    rect,
                                    values: chart_data.values,
                                    max_val: chart_data.max_value,
                                });
                                widget_line_ranges.push((content_start, content_end));
                            }
                        } else {
                            let tbl = app
                                .table_states
                                .get(&b.id)
                                .or_else(|| b.name.as_ref().and_then(|n| app.table_states.get(n)))
                                .cloned()
                                .or_else(|| {
                                    if out.format == crate::runner::BlockFormat::Table
                                        || out.output_text.contains(',')
                                        || out.output_text.contains('|')
                                    {
                                        crate::table::TableData::parse(&out.output_text)
                                    } else {
                                        None
                                    }
                                });
                            if let Some(t) = tbl {
                                output_widgets.push(OutputWidget::Table {
                                    rect,
                                    headers: t.headers,
                                    rows: t.rows,
                                    selected_row: t.selected_row,
                                });
                                widget_line_ranges.push((content_start, content_end));
                            }
                        }
                    }
                }
            }
        }

        // Render visible lines with syntax styling
        let mut rendered_lines = Vec::new();
        let mut inside_code = false;
        let mut inside_output = false;
        let mut is_output_collapsed = false;

        // First pass: determine state at scroll_top without unnecessary string allocations
        for i in 0..scroll_top {
            let line_slice = app.editor.buffer.rope.line(i);
            let chars = line_slice.chars().skip_while(|c| c.is_whitespace() && *c != '\n');
            if chars.clone().take(9).eq("```output".chars()) {
                inside_output = true;
                inside_code = false;
                let line_str = line_slice.to_string();
                is_output_collapsed = line_str.contains("[collapsed]");
            } else if chars.take(3).eq("```".chars()) {
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
                } else if trimmed.contains("[running]") {
                    format!("⏳ {}", trimmed)
                } else if trimmed.contains("[interrupted]") {
                    format!("⏹ {}", trimmed)
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
                    let header_rest = trimmed.trim_start_matches('`').trim();
                    let attrs = Runner::parse_fence_attributes(header_rest);
                    if attrs.danger || attrs.blast_radius.as_deref() == Some("critical") {
                        spans.push(Span::styled(" ⚠ DANGER / BLAST RADIUS: CRITICAL ⚠ ", Theme::DANGER_BANNER));
                    } else if attrs.blast_radius.as_deref() == Some("high") {
                        spans.push(Span::styled(" ⚠ HIGH BLAST RADIUS ⚠ ", Theme::CONFIRM_MODAL_TITLE_WARN));
                    }
                    if attrs.dry_run {
                        spans.push(Span::styled(" [DRY RUN] ", Theme::DRY_RUN_BADGE));
                    }
                    if attrs.approvals > 1 {
                        spans.push(Span::styled(format!(" [APPROVALS: {}] ", attrs.approvals), Theme::STATUS_BADGE_HOST));
                    }
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

            // If line is rendered by an interactive output widget, leave space clean
            let is_in_widget = widget_line_ranges.iter().any(|(s, e)| line_idx >= *s && line_idx < *e);
            if is_in_widget {
                rendered_lines.push(Line::from(spans));
                continue;
            }

            // Slice horizontal scroll
            let visible_content = if scroll_left < raw_line.chars().count() {
                raw_line.chars().skip(scroll_left).take(visible_width).collect::<String>()
            } else {
                String::new()
            };

            // Collect remote peers on this line
            let mut peers_on_line: Vec<&PeerInfo> = app
                .peers
                .values()
                .filter(|p| Some(&p.id) != app.my_peer_id.as_ref() && p.cursor_line == line_idx)
                .collect();
            peers_on_line.sort_by_key(|p| p.cursor_col);

            let cursor_line = app.editor.buffer.cursor_line;
            let cursor_col = app.editor.buffer.cursor_col;

            let is_markdown = !inside_output && !inside_code;
            let content_spans = Self::render_content_with_peers(
                &visible_content,
                PeerRenderContext {
                    scroll_left,
                    visible_width,
                    peers: &peers_on_line,
                    is_markdown,
                    line_idx,
                    cursor_line,
                    cursor_col,
                },
            );
            spans.extend(content_spans);

            rendered_lines.push(Line::from(spans));
        }

        let paragraph = Paragraph::new(rendered_lines);
        frame.render_widget(paragraph, inner_area);

        // Render dynamic canvas output widgets over their respective line areas
        for widget in output_widgets {
            match widget {
                OutputWidget::Table {
                    rect,
                    headers,
                    rows,
                    selected_row,
                } => {
                    let col_count = headers.len().max(1);
                    let pct = (100 / col_count).max(1) as u16;
                    let widths: Vec<Constraint> = (0..col_count).map(|_| Constraint::Percentage(pct)).collect();
                    let header_cells = headers
                        .iter()
                        .map(|h| Cell::from(h.as_str()).style(Theme::TABLE_HEADER));
                    let header_row = Row::new(header_cells).bottom_margin(1);

                    let table_rows = rows.iter().enumerate().map(|(r_idx, row)| {
                        let is_selected = r_idx == selected_row;
                        let style = if is_selected {
                            Theme::TABLE_ROW_SELECTED
                        } else if r_idx % 2 == 0 {
                            Theme::TABLE_ROW_EVEN
                        } else {
                            Theme::TABLE_ROW_ODD
                        };
                        let cells = row.iter().enumerate().map(|(c_idx, cell)| {
                            if is_selected && c_idx == 0 {
                                Cell::from(format!("▶ {}", cell)).style(style)
                            } else {
                                Cell::from(cell.as_str()).style(style)
                            }
                        });
                        Row::new(cells).style(style)
                    });

                    let table = Table::new(table_rows, &widths)
                        .header(header_row)
                        .block(
                            Block::default()
                                .borders(Borders::ALL)
                                .border_style(Theme::TITLE_ACTIVE)
                                .title(" 📊 Structured Output Table "),
                        );
                    frame.render_widget(table, rect);
                }
                OutputWidget::Sparkline {
                    rect,
                    values,
                    max_val,
                } => {
                    let spark = Sparkline::default()
                        .data(&values)
                        .max(max_val)
                        .style(Theme::CHART_SPARKLINE)
                        .block(
                            Block::default()
                                .borders(Borders::ALL)
                                .border_style(Theme::TITLE_ACTIVE)
                                .title(" 📈 Live Metric Sparkline "),
                        );
                    frame.render_widget(spark, rect);
                }
                OutputWidget::BarChart {
                    rect,
                    bars,
                    max_val,
                } => {
                    let bar_items: Vec<(&str, u64)> =
                        bars.iter().map(|(l, v)| (l.as_str(), *v)).collect();
                    let barchart = BarChart::default()
                        .data(&bar_items)
                        .bar_width(6)
                        .bar_gap(1)
                        .max(max_val)
                        .bar_style(Theme::CHART_BAR)
                        .value_style(Theme::CHART_BAR_VALUE)
                        .block(
                            Block::default()
                                .borders(Borders::ALL)
                                .border_style(Theme::TITLE_ACTIVE)
                                .title(" 📊 Metric Bar Chart "),
                        );
                    frame.render_widget(barchart, rect);
                }
                OutputWidget::Fleet { rect, fleet } => {
                    let total_hosts = fleet.hosts.len();
                    if total_hosts == 0 || rect.height < 2 {
                        continue;
                    }

                    let sel_tab = fleet.selected_tab.min(total_hosts.saturating_sub(1));
                    let tab_titles: Vec<Line> = fleet
                        .hosts
                        .iter()
                        .enumerate()
                        .map(|(idx, h)| {
                            let symbol = if h.exit_code == 0 { "✓" } else { "✖" };
                            let label = format!(" [{}: {} exit {}] ", h.host, symbol, h.exit_code);
                            if idx == sel_tab {
                                Line::from(vec![
                                    Span::styled("▶", Theme::FLEET_TAB_ACTIVE),
                                    Span::styled(label, Theme::FLEET_TAB_ACTIVE),
                                ])
                            } else {
                                let style = if h.exit_code == 0 {
                                    Theme::FLEET_HOST_OK
                                } else {
                                    Theme::FLEET_HOST_ERR
                                };
                                Line::from(vec![Span::styled(label, style)])
                            }
                        })
                        .collect();

                    let tabs = Tabs::new(tab_titles)
                        .select(sel_tab)
                        .style(Theme::FLEET_TAB_INACTIVE)
                        .highlight_style(Theme::FLEET_TAB_ACTIVE)
                        .divider(" ");

                    let tab_height = 1.min(rect.height);
                    let tab_area = Rect::new(rect.x, rect.y, rect.width, tab_height);
                    frame.render_widget(tabs, tab_area);

                    if rect.height > tab_height {
                        let body_area = Rect::new(
                            rect.x,
                            rect.y + tab_height,
                            rect.width,
                            rect.height - tab_height,
                        );

                        if let Some(host_out) = fleet.hosts.get(sel_tab) {
                            let mut text_lines = Vec::new();
                            let combined_text = if !host_out.stdout.is_empty() && !host_out.stderr.is_empty() {
                                format!("{}\n{}", host_out.stdout, host_out.stderr)
                            } else if !host_out.stdout.is_empty() {
                                host_out.stdout.clone()
                            } else if !host_out.stderr.is_empty() {
                                host_out.stderr.clone()
                            } else {
                                "(No output)".to_string()
                            };

                            for l in combined_text.lines().take(body_area.height.saturating_sub(2) as usize) {
                                let spans = crate::ansi::parse_ansi_spans(l);
                                text_lines.push(Line::from(spans));
                            }

                            let title = format!(
                                " 🌐 Host: {} (Exit: {}) [Tab {}/{}: Left/Right or [/] to switch] ",
                                host_out.host,
                                host_out.exit_code,
                                sel_tab + 1,
                                total_hosts
                            );

                            let border_style = if host_out.exit_code == 0 {
                                Theme::TITLE_ACTIVE
                            } else {
                                Theme::FLEET_HOST_ERR
                            };

                            let block = Block::default()
                                .borders(Borders::ALL)
                                .border_style(border_style)
                                .title(title);

                            let paragraph = Paragraph::new(text_lines).block(block);
                            frame.render_widget(paragraph, body_area);
                        }
                    }
                }
            }
        }

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

    fn parse_markdown_spans_with_focus(
        content: &str,
        line_idx: usize,
        cursor_line: usize,
        cursor_col: usize,
    ) -> Vec<Span<'static>> {
        let params = crate::form::FormParser::parse_line(line_idx, content);
        if params.is_empty() {
            return Self::parse_markdown_spans(content);
        }

        let mut spans = Vec::new();
        let mut last_idx = 0;

        for p in params {
            if p.start_col > last_idx && p.start_col <= content.len() {
                let segment = &content[last_idx..p.start_col];
                spans.extend(Self::parse_markdown_spans(segment));
            }

            let is_focused = (cursor_line == line_idx) && (cursor_col >= p.start_col && cursor_col <= p.end_col);

            let (badge_text, normal_style) = match &p.param_type {
                crate::form::FormParamType::Input => {
                    (format!(" ✎ {}: {} ", p.name, p.value), Theme::PARAM_INPUT)
                }
                crate::form::FormParamType::Secret => {
                    (format!(" 🔒 {}: {} ", p.name, p.display_value()), Theme::PARAM_SECRET)
                }
                crate::form::FormParamType::Select { .. } => {
                    (format!(" ▾ {}: {} ▾ ", p.name, p.value), Theme::PARAM_SELECT)
                }
                crate::form::FormParamType::Toggle => {
                    if p.value == "true" {
                        (format!(" ▣ {}: true ", p.name), Theme::PARAM_TOGGLE_ON)
                    } else {
                        (format!(" ⬚ {}: false ", p.name), Theme::PARAM_TOGGLE_OFF)
                    }
                }
            };

            let style = if is_focused {
                Theme::PARAM_FOCUSED
            } else {
                normal_style
            };

            spans.push(Span::styled(badge_text, style));
            last_idx = p.end_col;
        }

        if last_idx < content.len() {
            spans.extend(Self::parse_markdown_spans(&content[last_idx..]));
        }

        spans
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

        // Markdown task items: - [ ] or * [ ] or - [x] or * [x]
        if let Some(task) = crate::editor::buffer::TextBuffer::parse_task_item(content) {
            let mut spans = Vec::new();
            let prefix_end = task.prefix_end_col.min(content.len());
            let (prefix, rest) = content.split_at(prefix_end);
            let marker_start = task.marker_col.min(prefix.len());
            let (indent, marker_and_check) = prefix.split_at(marker_start);

            if !indent.is_empty() {
                spans.push(Span::raw(indent.to_string()));
            }

            if task.is_checked {
                spans.push(Span::styled(marker_and_check.to_string(), Theme::MD_TASK_CHECKED));
                spans.push(Span::styled(rest.to_string(), Theme::MD_TASK_COMPLETED_TEXT));
            } else {
                spans.push(Span::styled(marker_and_check.to_string(), Theme::MD_TASK_UNCHECKED));
                spans.push(Span::raw(rest.to_string()));
            }
            return spans;
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

    fn render_content_with_peers(
        content: &str,
        ctx: PeerRenderContext,
    ) -> Vec<Span<'static>> {
        if ctx.peers.is_empty() {
            if ctx.is_markdown {
                return Self::parse_markdown_spans_with_focus(content, ctx.line_idx, ctx.cursor_line, ctx.cursor_col);
            } else {
                return crate::ansi::parse_ansi_spans(content);
            }
        }

        let mut result = Vec::new();
        let chars: Vec<char> = content.chars().collect();
        let line_len = chars.len();

        let mut current_col = 0;
        for peer in ctx.peers {
            let p_col = peer.cursor_col.saturating_sub(ctx.scroll_left);
            let color = peer_color(peer.color_index);
            let badge_style = Style::default().bg(color).fg(Color::Black).add_modifier(Modifier::BOLD);
            let cursor_char_style = Style::default().bg(color).fg(Color::Black).add_modifier(Modifier::BOLD);

            if p_col > current_col {
                let chunk_end = p_col.min(line_len);
                if chunk_end > current_col {
                    let chunk: String = chars[current_col..chunk_end].iter().collect();
                    if ctx.is_markdown {
                        result.extend(Self::parse_markdown_spans_with_focus(&chunk, ctx.line_idx, ctx.cursor_line, ctx.cursor_col));
                    } else {
                        result.extend(crate::ansi::parse_ansi_spans(&chunk));
                    }
                    current_col = chunk_end;
                }
            }

            // Render badge
            result.push(Span::styled(format!("[{}]", peer.name), badge_style));

            // Render cursor character
            if p_col < line_len {
                result.push(Span::styled(chars[p_col].to_string(), cursor_char_style));
                current_col = (p_col + 1).max(current_col);
            } else {
                result.push(Span::styled("▌", badge_style));
            }
        }

        // Remainder of line
        if current_col < line_len {
            let chunk: String = chars[current_col..].iter().collect();
            if ctx.is_markdown {
                result.extend(Self::parse_markdown_spans_with_focus(&chunk, ctx.line_idx, ctx.cursor_line, ctx.cursor_col));
            } else {
                result.extend(crate::ansi::parse_ansi_spans(&chunk));
            }
        }

        // If any peers are past the end of the line
        for peer in ctx.peers {
            let p_col = peer.cursor_col.saturating_sub(ctx.scroll_left);
            if p_col > line_len && p_col < ctx.visible_width {
                let color = peer_color(peer.color_index);
                let badge_style = Style::default().bg(color).fg(Color::Black).add_modifier(Modifier::BOLD);
                result.push(Span::styled(format!(" [{}]▌", peer.name), badge_style));
            }
        }

        result
    }
}
