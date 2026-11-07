pub mod editor;
pub mod help;
pub mod palette;
pub mod search;
pub mod sidebar;
pub mod status_bar;
pub mod theme;

use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::Frame;

use crate::app::{App, AppMode};
use editor::EditorView;
use help::HelpView;
use palette::PaletteView;
use search::SearchView;
use sidebar::SidebarView;
use status_bar::StatusBarView;

pub struct UI;

impl UI {
    pub fn render(app: &App, frame: &mut Frame) {
        let size = frame.area();

        // Overall layout: Main workspace (Top) + Status bar (Bottom)
        let root_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Min(5),
                Constraint::Length(1),
            ])
            .split(size);

        let workspace_area = root_chunks[0];
        let status_area = root_chunks[1];

        // Workspace split: Sidebar (if visible) + Editor
        if app.sidebar_visible {
            let sidebar_width = (workspace_area.width * 25 / 100).clamp(24, 38);
            let h_chunks = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([
                    Constraint::Length(sidebar_width),
                    Constraint::Min(30),
                ])
                .split(workspace_area);

            SidebarView::render(app, frame, h_chunks[0]);
            EditorView::render(app, frame, h_chunks[1]);
        } else {
            EditorView::render(app, frame, workspace_area);
        }

        // Status bar
        StatusBarView::render(app, frame, status_area);

        // Modal overlays
        match app.mode {
            AppMode::CommandPalette => {
                PaletteView::render(app, frame, size);
            }
            AppMode::SearchResults => {
                SearchView::render(app, frame, size);
            }
            AppMode::Help => {
                HelpView::render(frame, size);
            }
            AppMode::Normal => {}
        }
    }
}
