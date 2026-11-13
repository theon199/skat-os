pub mod editor;
pub mod help;
pub mod palette;
pub mod search;
pub mod sidebar;
pub mod status_bar;
pub mod theme;

use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

use crate::app::{App, AppMode};
use crate::guard::DangerLevel;
use crate::ui::editor::EditorView;
use crate::ui::help::HelpView;
use crate::ui::palette::PaletteView;
use crate::ui::search::SearchView;
use crate::ui::sidebar::SidebarView;
use crate::ui::status_bar::StatusBarView;
use crate::ui::theme::Theme;

pub struct ConfirmModalParams<'a> {
    pub prompt: &'a str,
    pub command: &'a str,
    pub target_hosts: &'a [String],
    pub danger_level: DangerLevel,
    pub keyword_required: Option<&'a str>,
    pub keyword_input: &'a str,
}

pub struct UI;

impl UI {
    pub fn render(app: &App, frame: &mut Frame) {
        let size = frame.area();

        // Top level layout: main workspace + status bar
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Min(1),    // Workspace (Sidebar + Editor)
                Constraint::Length(1), // Status bar
            ])
            .split(size);

        let workspace_area = chunks[0];
        let status_area = chunks[1];

        // Workspace layout: sidebar (if visible) + editor
        if app.sidebar_visible {
            let sidebar_width = (workspace_area.width * 25 / 100).clamp(24, 38);
            let h_chunks = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Length(sidebar_width), Constraint::Min(30)])
                .split(workspace_area);

            SidebarView::render(app, frame, h_chunks[0]);
            EditorView::render(app, frame, h_chunks[1]);
        } else {
            EditorView::render(app, frame, workspace_area);
        }

        // Status bar
        StatusBarView::render(app, frame, status_area);

        // Modal overlays
        match &app.mode {
            AppMode::CommandPalette => {
                PaletteView::render(app, frame, size);
            }
            AppMode::SearchResults => {
                SearchView::render(app, frame, size);
            }
            AppMode::Help => {
                HelpView::render(frame, size);
            }
            AppMode::ParamPrompt {
                param_name,
                input_value,
                is_secret,
                ..
            } => {
                Self::render_param_prompt(param_name, input_value, *is_secret, frame, size);
            }
            AppMode::TableInspectModal {
                block_id,
                row_index,
            } => {
                Self::render_table_inspect_modal(app, block_id, *row_index, frame, size);
            }
            AppMode::ConfirmExecution {
                prompt,
                command,
                target_hosts,
                danger_level,
                keyword_required,
                keyword_input,
                ..
            } => {
                Self::render_confirm_execution_modal(
                    ConfirmModalParams {
                        prompt,
                        command,
                        target_hosts,
                        danger_level: *danger_level,
                        keyword_required: keyword_required.as_deref(),
                        keyword_input,
                    },
                    frame,
                    size,
                );
            }
            AppMode::AuditLogModal {
                selected_index,
                search_query,
                integrity_verified,
            } => {
                Self::render_audit_log_modal(
                    app,
                    *selected_index,
                    search_query,
                    *integrity_verified,
                    frame,
                    size,
                );
            }
            AppMode::UnsavedChangesModal {
                pending_action,
                doc_name,
            } => {
                Self::render_unsaved_changes_modal(doc_name, pending_action, frame, size);
            }
            AppMode::Normal => {}
        }
    }

    fn render_param_prompt(
        param_name: &str,
        input_value: &str,
        is_secret: bool,
        frame: &mut Frame,
        area: Rect,
    ) {
        let width = 56.min(area.width.saturating_sub(4)).max(30);
        let height = 5.min(area.height.saturating_sub(2));

        let x = area.x + (area.width.saturating_sub(width)) / 2;
        let y = area.y + (area.height.saturating_sub(height)) / 2;
        let popup_area = Rect::new(x, y, width, height);

        frame.render_widget(Clear, popup_area);

        let title_text = if is_secret {
            format!(" Edit Secret Parameter: {} ", param_name)
        } else {
            format!(" Edit Parameter: {} ", param_name)
        };

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(if is_secret {
                Theme::PARAM_SECRET
            } else {
                Theme::TITLE_ACTIVE
            })
            .title(Span::styled(
                title_text,
                if is_secret {
                    Theme::PARAM_SECRET
                } else {
                    Theme::TITLE_ACTIVE
                },
            ));

        let inner = block.inner(popup_area);
        frame.render_widget(block, popup_area);

        let display_val = if is_secret {
            "•".repeat(input_value.len())
        } else {
            input_value.to_string()
        };

        let lines = vec![
            Line::from(vec![
                Span::raw("Value: "),
                Span::styled(format!("{}_", display_val), Theme::PALETTE_SELECTED),
            ]),
            Line::from(""),
            Line::from(vec![Span::styled(
                "[Enter] Apply    [Esc] Cancel",
                Theme::TITLE_INACTIVE,
            )]),
        ];

        let p = Paragraph::new(lines);
        frame.render_widget(p, inner);

        // Cursor
        let cursor_x =
            inner.x + 7 + (display_val.chars().count() as u16).min(inner.width.saturating_sub(8));
        let cursor_y = inner.y;
        frame.set_cursor_position((cursor_x, cursor_y));
    }

    fn render_confirm_execution_modal(params: ConfirmModalParams, frame: &mut Frame, area: Rect) {
        let width = 72.min(area.width.saturating_sub(4)).max(40);
        let height = if params.keyword_required.is_some() {
            12
        } else {
            9
        }
        .min(area.height.saturating_sub(2));

        let x = area.x + (area.width.saturating_sub(width)) / 2;
        let y = area.y + (area.height.saturating_sub(height)) / 2;
        let popup_area = Rect::new(x, y, width, height);

        frame.render_widget(Clear, popup_area);

        let is_critical = params.danger_level >= DangerLevel::High;
        let border_style = if is_critical {
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)
        } else {
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD)
        };

        let title_str = if is_critical {
            format!(" SAFETY CONFIRMATION GATE [{}] ", params.danger_level)
        } else {
            " SAFETY CONFIRMATION GATE ".to_string()
        };

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(border_style)
            .title(Span::styled(title_str, border_style));

        let inner = block.inner(popup_area);
        frame.render_widget(block, popup_area);

        let targets_str = if params.target_hosts.is_empty() {
            "localhost".to_string()
        } else {
            params.target_hosts.join(", ")
        };

        let cmd_preview: String = params
            .command
            .lines()
            .next()
            .unwrap_or(params.command)
            .chars()
            .take(55)
            .collect();

        let mut lines = vec![
            Line::from(vec![
                Span::styled(
                    "Warning: ",
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(params.prompt.to_string(), Style::default().fg(Color::White)),
            ]),
            Line::from(vec![
                Span::styled("Target Hosts: ", Style::default().fg(Color::Cyan)),
                Span::styled(targets_str, Style::default().fg(Color::LightCyan)),
            ]),
            Line::from(vec![
                Span::styled("Command: ", Style::default().fg(Color::Green)),
                Span::styled(cmd_preview, Style::default().fg(Color::LightGreen)),
            ]),
            Line::from(""),
        ];

        if let Some(kw) = params.keyword_required {
            lines.push(Line::from(vec![
                Span::styled(
                    format!("Type '{}' to confirm: ", kw),
                    Style::default()
                        .fg(Color::LightRed)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!("[{}]_", params.keyword_input),
                    Theme::PALETTE_SELECTED,
                ),
            ]));
            lines.push(Line::from(""));
        }

        lines.push(Line::from(vec![
            Span::styled(" [Enter] Confirm Execution ", Theme::STATUS_BADGE_DIRTY),
            Span::raw("    "),
            Span::styled(" [Esc] Cancel ", Theme::TITLE_INACTIVE),
        ]));

        let p = Paragraph::new(lines);
        frame.render_widget(p, inner);
    }

    fn render_audit_log_modal(
        app: &App,
        selected_index: usize,
        search_query: &str,
        integrity_verified: bool,
        frame: &mut Frame,
        area: Rect,
    ) {
        let width = 94.min(area.width.saturating_sub(4)).max(50);
        let height = 22.min(area.height.saturating_sub(2)).max(12);

        let x = area.x + (area.width.saturating_sub(width)) / 2;
        let y = area.y + (area.height.saturating_sub(height)) / 2;
        let popup_area = Rect::new(x, y, width, height);

        frame.render_widget(Clear, popup_area);

        let status_title = if integrity_verified {
            "SHA-256 Verified"
        } else {
            "TAMPER DETECTED"
        };

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(if integrity_verified {
                Theme::TITLE_ACTIVE
            } else {
                Theme::DANGER_BANNER
            })
            .title(Span::styled(
                format!(" Execution Audit Log Timeline [{}] ", status_title),
                if integrity_verified {
                    Theme::TITLE_ACTIVE
                } else {
                    Theme::DANGER_BANNER
                },
            ));

        let inner = block.inner(popup_area);
        frame.render_widget(block, popup_area);

        // Load audit entries from disk
        let entries = crate::audit::AuditRecorder::new(&app.workspace.root_dir)
            .and_then(|r| r.load_entries())
            .unwrap_or_default();

        let q_lower = search_query.to_lowercase();
        let filtered: Vec<&crate::audit::AuditEntry> = entries
            .iter()
            .rev() // Show newest first
            .filter(|e| {
                if q_lower.is_empty() {
                    return true;
                }
                e.command.to_lowercase().contains(&q_lower)
                    || e.block_id.to_lowercase().contains(&q_lower)
                    || e.collaborator.to_lowercase().contains(&q_lower)
                    || e.target_hosts
                        .iter()
                        .any(|h| h.to_lowercase().contains(&q_lower))
                    || e.hash.to_lowercase().contains(&q_lower)
            })
            .collect();

        let mut lines = Vec::new();

        // Search bar
        lines.push(Line::from(vec![
            Span::styled(
                "Search: ",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(format!("[{}]_", search_query), Theme::PALETTE_SELECTED),
            Span::raw(format!("  ({} entries)", filtered.len())),
        ]));
        lines.push(Line::from(""));

        // Entry rows
        let available_rows = inner.height.saturating_sub(7) as usize;
        let start_idx = if selected_index >= available_rows {
            selected_index - available_rows + 1
        } else {
            0
        };

        if filtered.is_empty() {
            lines.push(Line::from(Span::styled(
                "  No audit log entries found.",
                Style::default().fg(Color::DarkGray),
            )));
        } else {
            for (idx, entry) in filtered
                .iter()
                .enumerate()
                .skip(start_idx)
                .take(available_rows)
            {
                let is_sel = idx == selected_index;
                let status_badge = if entry.exit_code == 0 {
                    Span::styled(
                        " [OK] ",
                        Style::default()
                            .fg(Color::Green)
                            .add_modifier(Modifier::BOLD),
                    )
                } else {
                    Span::styled(
                        format!(" [ERR:{}] ", entry.exit_code),
                        Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
                    )
                };

                let ts_short: String = entry.timestamp.chars().take(19).collect();
                let hosts_str = entry.target_hosts.join(",");
                let cmd_short: String = entry
                    .command
                    .lines()
                    .next()
                    .unwrap_or(&entry.command)
                    .chars()
                    .take(28)
                    .collect();
                let hash_short: String = entry.hash.chars().take(8).collect();

                let row_spans = vec![
                    if is_sel {
                        Span::styled("▶ ", Theme::PALETTE_SELECTED)
                    } else {
                        Span::raw("  ")
                    },
                    status_badge,
                    Span::styled(format!("{} ", ts_short), Style::default().fg(Color::Gray)),
                    Span::styled(
                        format!("{} ", entry.collaborator),
                        Style::default().fg(Color::Magenta),
                    ),
                    Span::styled(format!("@{} ", hosts_str), Style::default().fg(Color::Cyan)),
                    Span::styled(
                        format!("'{}' ", cmd_short),
                        Style::default().fg(Color::LightGreen),
                    ),
                    Span::styled(
                        format!("#{}", hash_short),
                        Style::default().fg(Color::Yellow),
                    ),
                ];

                let row_style = if is_sel {
                    Style::default().bg(Color::Rgb(40, 50, 70))
                } else {
                    Style::default()
                };

                lines.push(Line::from(row_spans).style(row_style));
            }
        }

        // Selected entry detail footer
        lines.push(Line::from(""));
        if let Some(selected_entry) = filtered.get(selected_index) {
            lines.push(Line::from(vec![
                Span::styled("Command: ", Style::default().fg(Color::Yellow)),
                Span::styled(
                    selected_entry.command.clone(),
                    Style::default().fg(Color::White),
                ),
            ]));
            lines.push(Line::from(vec![
                Span::styled("Hash: ", Style::default().fg(Color::Cyan)),
                Span::styled(
                    selected_entry.hash.clone(),
                    Style::default().fg(Color::LightCyan),
                ),
                Span::raw(" | "),
                Span::styled("Prev: ", Style::default().fg(Color::Gray)),
                Span::styled(
                    selected_entry
                        .prev_hash
                        .chars()
                        .take(16)
                        .collect::<String>()
                        + "...",
                    Style::default().fg(Color::Gray),
                ),
            ]));
        } else {
            lines.push(Line::from(""));
            lines.push(Line::from(""));
        }

        lines.push(Line::from(vec![Span::styled(
            " [▲/▼] Navigate    [Esc] Close    [Enter] Select ",
            Theme::PALETTE_SELECTED,
        )]));

        let p = Paragraph::new(lines);
        frame.render_widget(p, inner);
    }

    fn render_table_inspect_modal(
        app: &App,
        block_id: &str,
        row_index: usize,
        frame: &mut Frame,
        area: Rect,
    ) {
        let tbl_opt = app.table_states.get(block_id).or_else(|| {
            // Check if any block has this id as name
            app.table_states
                .iter()
                .find(|(k, _)| k.eq_ignore_ascii_case(block_id))
                .map(|(_, v)| v)
        });

        let (headers, row, total_rows) = if let Some(tbl) = tbl_opt {
            let r = tbl.rows.get(row_index).cloned().unwrap_or_default();
            (tbl.headers.clone(), r, tbl.rows.len())
        } else {
            (Vec::new(), Vec::new(), 0)
        };

        let width = (area.width * 70 / 100)
            .clamp(40, 80)
            .min(area.width.saturating_sub(2));
        let height = (headers.len() as u16 + 5)
            .clamp(8, 25)
            .min(area.height.saturating_sub(2));

        let x = area.x + (area.width.saturating_sub(width)) / 2;
        let y = area.y + (area.height.saturating_sub(height)) / 2;
        let popup_area = Rect::new(x, y, width, height);

        frame.render_widget(Clear, popup_area);

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Theme::TITLE_ACTIVE)
            .title(Span::styled(
                format!(
                    " Row Inspection: {} (Row {}/{}) ",
                    block_id,
                    row_index + 1,
                    total_rows
                ),
                Theme::TITLE_ACTIVE,
            ));

        let inner = block.inner(popup_area);
        frame.render_widget(block, popup_area);

        let mut lines = Vec::new();
        for (i, h) in headers.iter().enumerate() {
            let val = row.get(i).cloned().unwrap_or_default();
            lines.push(Line::from(vec![
                Span::styled(format!("{:>18}: ", h), Theme::TABLE_HEADER),
                Span::styled(val, Style::default().fg(Color::White)),
            ]));
        }

        lines.push(Line::from(""));
        lines.push(Line::from(vec![Span::styled(
            " [▲/▼] Previous/Next Row  │  [Enter/Esc/q] Close Modal ",
            Theme::PALETTE_SELECTED,
        )]));

        let paragraph = Paragraph::new(lines);
        frame.render_widget(paragraph, inner);
    }

    fn render_unsaved_changes_modal(
        doc_name: &str,
        pending_action: &crate::app::PendingAction,
        frame: &mut Frame,
        area: Rect,
    ) {
        let width = 64.min(area.width.saturating_sub(4)).max(36);
        let height = 7.min(area.height.saturating_sub(2));

        let x = area.x + (area.width.saturating_sub(width)) / 2;
        let y = area.y + (area.height.saturating_sub(height)) / 2;
        let popup_area = Rect::new(x, y, width, height);

        frame.render_widget(Clear, popup_area);

        let action_str = match pending_action {
            crate::app::PendingAction::Quit => "quitting skat-os",
            crate::app::PendingAction::OpenDocument { .. } => "switching documents",
        };

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            )
            .title(Span::styled(
                " Unsaved Changes ",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ));

        let inner = block.inner(popup_area);
        frame.render_widget(block, popup_area);

        let lines = vec![
            Line::from(vec![
                Span::raw(" '"),
                Span::styled(
                    doc_name,
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(format!("' has unsaved changes before {}!", action_str)),
            ]),
            Line::from(""),
            Line::from(vec![
                Span::styled(
                    " [S] ",
                    Style::default()
                        .fg(Color::Green)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw("Save & Proceed   "),
                Span::styled(
                    "[D] ",
                    Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
                ),
                Span::raw("Discard   "),
                Span::styled(
                    "[C / Esc] ",
                    Style::default()
                        .fg(Color::DarkGray)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw("Cancel"),
            ]),
        ];

        frame.render_widget(Paragraph::new(lines), inner);
    }
}
