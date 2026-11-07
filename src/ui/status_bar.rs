use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::app::{App, FocusPane};
use crate::ui::theme::Theme;

pub struct StatusBarView;

impl StatusBarView {
    pub fn render(app: &App, frame: &mut Frame, area: Rect) {
        let (mode_text, mode_style) = if app.is_running_block {
            (" RUNNING... ", Theme::STATUS_BADGE_RUNNING)
        } else if app.editor.is_dirty() {
            (" MODIFIED ", Theme::STATUS_BADGE_DIRTY)
        } else {
            (" SKAT-OS ", Theme::STATUS_BADGE_NORMAL)
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

        let hints = " [Ctrl+K] Palette | [Ctrl+R] Run | [Ctrl+B] Sidebar | [F1] Help | [Ctrl+Q] Quit ";

        let left_spans = vec![
            Span::styled(mode_text, mode_style),
            Span::styled(focus_text, Theme::TITLE_ACTIVE),
            Span::styled(format!(" 📄 {} ", file_name), Theme::STATUS_BAR),
            Span::styled(
                if app.status_message.is_empty() {
                    String::new()
                } else {
                    format!(" | {} ", app.status_message)
                },
                Theme::TITLE_ACTIVE,
            ),
        ];

        let right_spans = vec![
            Span::styled(cursor_info, Theme::STATUS_BAR),
            Span::styled(hints, Theme::TITLE_INACTIVE),
        ];

        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
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
