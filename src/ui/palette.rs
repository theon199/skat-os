use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, Paragraph};
use ratatui::Frame;

use crate::app::App;
use crate::ui::theme::Theme;

pub struct PaletteView;

impl PaletteView {
    pub fn render(app: &App, frame: &mut Frame, area: Rect) {
        // Calculate centered modal area
        let modal_width = (area.width * 65 / 100).max(40).min(area.width);
        let modal_height = (area.height * 60 / 100).max(12).min(area.height);

        let x = (area.width.saturating_sub(modal_width)) / 2;
        let y = (area.height.saturating_sub(modal_height)) / 2;
        let modal_area = Rect::new(x, y, modal_width, modal_height);

        // Clear background underneath modal
        frame.render_widget(Clear, modal_area);

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Theme::TITLE_ACTIVE)
            .title(Span::styled(" ⚡ Command Palette ", Theme::TITLE_ACTIVE));

        let inner_area = block.inner(modal_area);
        frame.render_widget(block, modal_area);

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3), // Input box
                Constraint::Min(4),    // Items list
                Constraint::Length(1), // Footer hint
            ])
            .split(inner_area);

        // 1. Input Box
        let query_text = format!(" > {}_", app.palette.query);
        let input_box = Paragraph::new(Line::from(vec![
            Span::styled(query_text, Theme::PALETTE_INPUT),
        ]))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Theme::TITLE_ACTIVE)
                .title(" Search documents or commands "),
        );
        frame.render_widget(input_box, chunks[0]);

        // 2. Items List
        let mut list_items = Vec::new();
        for (idx, item) in app.palette.items.iter().enumerate() {
            let is_selected = idx == app.palette.selected_index;

            let prefix = if is_selected { "▶ " } else { "  " };
            let style = if is_selected {
                Theme::PALETTE_SELECTED
            } else {
                Theme::SIDEBAR_ITEM
            };

            let subtext_style = if is_selected {
                Theme::PALETTE_SELECTED
            } else {
                Theme::TITLE_INACTIVE
            };

            let line = Line::from(vec![
                Span::styled(prefix, style),
                Span::styled(&item.label, style),
                Span::styled(format!("  — {}", item.subtext), subtext_style),
            ]);

            list_items.push(ListItem::new(line));
        }

        if list_items.is_empty() {
            list_items.push(ListItem::new(Line::from(Span::styled(
                "  No matching documents or commands",
                Theme::TITLE_INACTIVE,
            ))));
        }

        let list = List::new(list_items);
        frame.render_widget(list, chunks[1]);

        // 3. Footer
        let footer = Paragraph::new(Line::from(vec![
            Span::styled(
                " [↑/↓] Select  [Enter] Execute  [Esc] Close ",
                Theme::TITLE_INACTIVE,
            ),
        ]))
        .alignment(ratatui::layout::Alignment::Center);
        frame.render_widget(footer, chunks[2]);
    }
}
