use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem};
use ratatui::Frame;

use crate::app::{App, CollabMode, FocusPane};
use crate::collab::peer_color;
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
            .title(Span::styled(" Workspace ", if is_focused { Theme::TITLE_ACTIVE } else { Theme::TITLE_INACTIVE }));

        let inner_area = block.inner(area);
        frame.render_widget(block, area);

        let is_collab = app.collab_mode != CollabMode::Offline;

        // Split into Documents, Peers (if collab active), and Backlinks
        let chunks = if is_collab {
            Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Percentage(50),
                    Constraint::Percentage(25),
                    Constraint::Percentage(25),
                ])
                .split(inner_area)
        } else {
            Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Percentage(65),
                    Constraint::Percentage(35),
                ])
                .split(inner_area)
        };

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

        // 2. Connected Peers (if in collab mode)
        if is_collab {
            let mut peer_items = Vec::new();
            if app.peers.is_empty() {
                peer_items.push(ListItem::new(Line::from(Span::styled(
                    "  Waiting for peers...",
                    Theme::SIDEBAR_ITEM,
                ))));
            } else {
                for peer in app.peers.values() {
                    let color = peer_color(peer.color_index);
                    let is_me = Some(&peer.id) == app.my_peer_id.as_ref();

                    let spans = vec![
                        Span::styled("● ", Style::default().fg(color)),
                        Span::styled(
                            &peer.name,
                            if is_me {
                                Theme::TITLE_ACTIVE
                            } else {
                                Theme::SIDEBAR_ITEM
                            },
                        ),
                        Span::styled(if is_me { " (you)" } else { "" }, Theme::TITLE_INACTIVE),
                        Span::styled(
                            format!(" Ln {}, Col {}", peer.cursor_line + 1, peer.cursor_col + 1),
                            Theme::TITLE_INACTIVE,
                        ),
                    ];
                    peer_items.push(ListItem::new(Line::from(spans)));
                }
            }

            let peers_title = format!(" Connected Peers ({}) ", app.peers.len());
            let peers_list = List::new(peer_items).block(
                Block::default()
                    .borders(Borders::BOTTOM)
                    .border_style(Theme::TITLE_INACTIVE)
                    .title(peers_title),
            );
            frame.render_widget(peers_list, chunks[1]);
        }

        // 3. Backlinks
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

        let backlinks_chunk_idx = if is_collab { 2 } else { 1 };
        let backlinks_list = List::new(backlink_items).block(
            Block::default()
                .title(" Backlinks "),
        );
        frame.render_widget(backlinks_list, chunks[backlinks_chunk_idx]);
    }
}
