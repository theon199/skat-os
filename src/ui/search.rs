use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, Paragraph};
use ratatui::Frame;

use crate::app::App;
use crate::ui::theme::Theme;

pub struct SearchView;

impl SearchView {
    pub fn render(app: &App, frame: &mut Frame, area: Rect) {
        let modal_width = (area.width * 75 / 100).max(50).min(area.width);
        let modal_height = (area.height * 70 / 100).max(14).min(area.height);

        let x = (area.width.saturating_sub(modal_width)) / 2;
        let y = (area.height.saturating_sub(modal_height)) / 2;
        let modal_area = Rect::new(x, y, modal_width, modal_height);

        frame.render_widget(Clear, modal_area);

        let title = format!(" 🔍 Search Results: \"{}\" ", app.search_query);
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Theme::TITLE_ACTIVE)
            .title(Span::styled(title, Theme::TITLE_ACTIVE));

        let inner_area = block.inner(modal_area);
        frame.render_widget(block, modal_area);

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Min(4),    // Matches list
                Constraint::Length(1), // Footer
            ])
            .split(inner_area);

        let mut list_items = Vec::new();
        for (idx, m) in app.search_matches.iter().enumerate() {
            let is_selected = idx == app.search_selected_index;
            let prefix = if is_selected { "▶ " } else { "  " };

            let style = if is_selected {
                Theme::PALETTE_SELECTED
            } else {
                Theme::SIDEBAR_ITEM
            };

            let loc_style = if is_selected {
                Theme::PALETTE_SELECTED
            } else {
                Theme::TITLE_ACTIVE
            };

            let line = Line::from(vec![
                Span::styled(prefix, style),
                Span::styled(format!("{}:{}  ", m.relative_path, m.line_number), loc_style),
                Span::styled(&m.line_content, style),
            ]);

            list_items.push(ListItem::new(line));
        }

        if list_items.is_empty() {
            list_items.push(ListItem::new(Line::from(Span::styled(
                "  No occurrences found across workspace.",
                Theme::TITLE_INACTIVE,
            ))));
        }

        let list = List::new(list_items);
        frame.render_widget(list, chunks[0]);

        let footer = Paragraph::new(Line::from(vec![
            Span::styled(
                " [↑/↓] Select match  [Enter] Jump to line  [Esc] Close ",
                Theme::TITLE_INACTIVE,
            ),
        ]))
        .alignment(ratatui::layout::Alignment::Center);
        frame.render_widget(footer, chunks[1]);
    }
}
