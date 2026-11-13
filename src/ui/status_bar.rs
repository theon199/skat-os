use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::app::{App, CollabMode, FocusPane};
use crate::ui::theme::Theme;

pub struct StatusBarView;

impl StatusBarView {
    pub fn render(app: &App, frame: &mut Frame, area: Rect) {
        let (mode_text, mode_style) = if app.is_running_block {
            (" RUNNING... ".to_string(), Theme::STATUS_BADGE_RUNNING)
        } else {
            match &app.collab_mode {
                CollabMode::Host { port } => (format!(" 🌐 HOST:{} ", port), Theme::STATUS_BADGE_HOST),
                CollabMode::Client { .. } => (" 🔗 JOINED ".to_string(), Theme::STATUS_BADGE_CLIENT),
                CollabMode::Offline => {
                    if app.editor.is_dirty() {
                        (" MODIFIED ".to_string(), Theme::STATUS_BADGE_DIRTY)
                    } else {
                        (" SKAT-OS ".to_string(), Theme::STATUS_BADGE_NORMAL)
                    }
                }
            }
        };

        let focus_text = match app.focus {
            FocusPane::Editor => " EDITOR ",
            FocusPane::Sidebar => " SIDEBAR ",
        };

        let file_name = match &app.editor.file_path {
            Some(p) => p
                .file_name()
                .map(|f| f.to_string_lossy().to_string())
                .unwrap_or_else(|| "Untitled".to_string()),
            None => "Untitled".to_string(),
        };

        let cursor_info = format!(
            " Ln {}, Col {} | {} lines ",
            app.editor.buffer.cursor_line + 1,
            app.editor.buffer.cursor_col + 1,
            app.editor.line_count()
        );

        let hints = if app.is_running_block {
            " [Ctrl+C] Cancel Stream | [Ctrl+Q] Quit "
        } else {
            " [Ctrl+K] Palette | [Ctrl+R] Run | [Ctrl+B] Sidebar | [F1] Help | [Ctrl+Q] Quit "
        };

        let mut left_spans = vec![
            Span::styled(mode_text, mode_style),
            Span::styled(focus_text, Theme::TITLE_ACTIVE),
        ];

        // Connected Peers badge
        if app.collab_mode != CollabMode::Offline {
            let other_peers: Vec<String> = app
                .peers
                .values()
                .filter(|p| Some(&p.id) != app.my_peer_id.as_ref())
                .map(|p| p.name.clone())
                .collect();

            let peer_badge = if other_peers.is_empty() {
                " 👥 Connected Peers: 1 (you) ".to_string()
            } else {
                format!(
                    " 👥 Connected Peers: {} [{}] ",
                    other_peers.len() + 1,
                    other_peers.join(", ")
                )
            };
            left_spans.push(Span::styled(peer_badge, Theme::COLLAB_PEER_BADGE));
        }

        // Live Tasks completion badge
        if let Some((completed, total)) = app.editor.buffer.task_stats() {
            let pct = if total > 0 { (completed * 100) / total } else { 0 };
            let task_badge = format!(" ✓ Tasks: {}/{} ({}%) ", completed, total, pct);
            let task_style = if completed == total {
                Theme::STATUS_BADGE_TASKS_COMPLETE
            } else {
                Theme::STATUS_BADGE_TASKS
            };
            left_spans.push(Span::styled(task_badge, task_style));
        }

        // Execution Audit Status Badge
        left_spans.push(Span::styled(" 🛡 AUDIT: ON ", Theme::STATUS_BADGE_AUDIT));

        left_spans.push(Span::styled(format!(" 📄 {} ", file_name), Theme::STATUS_BAR));
        left_spans.push(Span::styled(
            if app.status_message.is_empty() {
                String::new()
            } else {
                format!(" | {} ", app.status_message)
            },
            Theme::TITLE_ACTIVE,
        ));

        let right_spans = vec![
            Span::styled(cursor_info, Theme::STATUS_BAR),
            Span::styled(hints, Theme::TITLE_INACTIVE),
        ];

        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
            .split(area);

        frame.render_widget(Paragraph::new(Line::from(left_spans)).style(Theme::STATUS_BAR), chunks[0]);
        frame.render_widget(
            Paragraph::new(Line::from(right_spans))
                .style(Theme::STATUS_BAR)
                .alignment(ratatui::layout::Alignment::Right),
            chunks[1],
        );
    }
}
