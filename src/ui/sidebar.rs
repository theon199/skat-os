use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem};
use ratatui::Frame;

use crate::app::{App, FocusPane};
use crate::ui::theme::Theme;

pub struct SidebarView;

impl SidebarView {
    pub fn render(app: &App, frame: &mut Frame, area: Rect) {
        let is_focused = app.focus == FocusPane::Sidebar;

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(if is_focused {
                Theme::TITLE_ACTIVE
            } else {
                Theme::TITLE_INACTIVE
            })
            .title(Span::styled(" 📁 Workspace ", if is_focused { Theme::TITLE_ACTIVE } else { Theme::TITLE_INACTIVE }));

        let inner_area = block.inner(area);
        frame.render_widget(block, area);

        // Split into Documents list (top) and Backlinks list (bottom)
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Percentage(65),
                Constraint::Percentage(35),
            ])
            .split(inner_area);

        // 1. Documents list
        let mut doc_items = Vec::new();
        for (idx, doc) in app.workspace.docs.iter().enumerate() {
            let is_active_doc = app
                .editor
                .file_path
                .as_ref()
                .map(|p| p == &doc.path)
                .unwrap_or(false);

            let is_selected_in_sidebar = is_focused && idx == app.sidebar_selected_index;

            let prefix = if is_selected_in_sidebar {
                "▶ "
            } else if is_active_doc {
                "● "
            } else {
                "  "
            };

            let style = if is_selected_in_sidebar {
                Theme::SIDEBAR_ITEM_SELECTED
            } else if is_active_doc {
                Theme::TITLE_ACTIVE
            } else {
                Theme::SIDEBAR_ITEM
            };

            let line = Line::from(vec![
                Span::styled(prefix, style),
                Span::styled(&doc.title, style),
            ]);

            doc_items.push(ListItem::new(line));
        }

        let doc_list = List::new(doc_items).block(
            Block::default()
                .borders(Borders::BOTTOM)
                .border_style(Theme::TITLE_INACTIVE)
                .title(" Documents "),
        );
        frame.render_widget(doc_list, chunks[0]);

        // 2. Backlinks
        let current_backlinks = if let Some(path) = &app.editor.file_path {
            app.workspace.get_backlinks_for(path)
        } else {
            Vec::new()
        };

        let mut backlink_items = Vec::new();
        if current_backlinks.is_empty() {
            backlink_items.push(ListItem::new(Line::from(Span::styled(
                "  (no incoming links)",
                Theme::SIDEBAR_ITEM,
            ))));
        } else {
            for link in current_backlinks {
                backlink_items.push(ListItem::new(Line::from(vec![
                    Span::styled("  <- ", Theme::SIDEBAR_BACKLINK),
                    Span::styled(link, Theme::SIDEBAR_ITEM),
                ])));
            }
        }

        let backlinks_list = List::new(backlink_items).block(
            Block::default()
                .title(" Backlinks "),
        );
        frame.render_widget(backlinks_list, chunks[1]);
    }
}
