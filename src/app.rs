use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::path::{Path, PathBuf};
use tokio::sync::mpsc;

use crate::editor::Editor;
use crate::palette::{CommandPalette, PaletteAction};
use crate::runner::{CodeBlock, ExecutionResult, Runner};
use crate::workspace::{SearchMatch, Workspace};

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum AppMode {
    Normal,
    CommandPalette,
    SearchResults,
    Help,
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum FocusPane {
    Sidebar,
    Editor,
}

pub enum AppEvent {
    CodeExecuted {
        block: CodeBlock,
        result: ExecutionResult,
    },
    StatusMessage(String),
}

pub struct App {
    pub workspace: Workspace,
    pub editor: Editor,
    pub palette: CommandPalette,
    pub mode: AppMode,
    pub focus: FocusPane,
    pub sidebar_visible: bool,
    pub sidebar_selected_index: usize,
    pub status_message: String,
    pub is_running_block: bool,
    pub search_query: String,
    pub search_matches: Vec<SearchMatch>,
    pub search_selected_index: usize,
    pub should_quit: bool,
    pub event_tx: mpsc::UnboundedSender<AppEvent>,
    pub event_rx: mpsc::UnboundedReceiver<AppEvent>,
}

impl App {
    pub fn new(root_dir: PathBuf) -> Result<Self> {
        let workspace = Workspace::new(root_dir)?;
        let mut editor = Editor::new();

        // Load first document if available (e.g. welcome.md)
        if let Some(first_doc) = workspace.docs.first() {
            if let Ok(ed) = Editor::from_file(&first_doc.path) {
                editor = ed;
            }
        }

        let (event_tx, event_rx) = mpsc::unbounded_channel();

        Ok(Self {
            workspace,
            editor,
            palette: CommandPalette::new(),
            mode: AppMode::Normal,
            focus: FocusPane::Editor,
            sidebar_visible: true,
            sidebar_selected_index: 0,
            status_message: "Welcome to skat-os".to_string(),
            is_running_block: false,
            search_query: String::new(),
            search_matches: Vec::new(),
            search_selected_index: 0,
            should_quit: false,
            event_tx,
            event_rx,
        })
    }

    pub fn open_document(&mut self, path: &Path) {
        if let Ok(new_editor) = Editor::from_file(path) {
            self.editor = new_editor;
            self.focus = FocusPane::Editor;
            self.status_message = format!("Opened {}", path.file_name().unwrap_or_default().to_string_lossy());
        }
    }

    pub fn handle_app_events(&mut self) {
        while let Ok(event) = self.event_rx.try_recv() {
            match event {
                AppEvent::CodeExecuted { block, result } => {
                    self.is_running_block = false;
                    Runner::apply_output(&mut self.editor.buffer, &block, &result);
                    self.status_message = format!("Execution finished [exit: {}]", result.exit_code);
                }
                AppEvent::StatusMessage(msg) => {
                    self.status_message = msg;
                }
            }
        }
    }

    pub fn handle_key_event(&mut self, key: KeyEvent) {
        match self.mode {
            AppMode::CommandPalette => self.handle_palette_key(key),
            AppMode::SearchResults => self.handle_search_key(key),
            AppMode::Help => self.handle_help_key(key),
            AppMode::Normal => self.handle_normal_key(key),
        }
    }

    fn handle_normal_key(&mut self, key: KeyEvent) {
        // Global shortcuts
        if key.modifiers.contains(KeyModifiers::CONTROL) {
            match key.code {
                KeyCode::Char('q') | KeyCode::Char('c') => {
                    self.should_quit = true;
                    return;
                }
                KeyCode::Char('k') => {
                    self.mode = AppMode::CommandPalette;
                    self.palette.reset(&self.workspace, self.editor.file_path.as_ref());
                    return;
                }
                KeyCode::Char('b') => {
                    self.sidebar_visible = !self.sidebar_visible;
                    if !self.sidebar_visible && self.focus == FocusPane::Sidebar {
                        self.focus = FocusPane::Editor;
                    }
                    return;
                }
                KeyCode::Char('w') => {
                    if self.sidebar_visible {
                        self.focus = match self.focus {
                            FocusPane::Editor => FocusPane::Sidebar,
                            FocusPane::Sidebar => FocusPane::Editor,
                        };
                    }
                    return;
                }
                KeyCode::Char('s') => {
                    self.save_current_document();
                    return;
                }
                KeyCode::Char('r') => {
                    self.trigger_run_block();
                    return;
                }
                KeyCode::Char('o') => {
                    let cursor_line = self.editor.buffer.cursor_line;
                    if Runner::toggle_output_collapse(&mut self.editor.buffer, cursor_line) {
                        self.status_message = "Toggled output collapse".to_string();
                    }
                    return;
                }
                KeyCode::Char('z') => {
                    if self.editor.buffer.undo() {
                        self.status_message = "Undo".to_string();
                    }
                    return;
                }
                KeyCode::Char('y') => {
                    if self.editor.buffer.redo() {
                        self.status_message = "Redo".to_string();
                    }
                    return;
                }
                _ => {}
            }
        }

        match key.code {
            KeyCode::F(1) => {
                self.mode = AppMode::Help;
                return;
            }
            KeyCode::Tab => {
                if self.focus == FocusPane::Sidebar {
                    self.focus = FocusPane::Editor;
                } else {
                    self.editor.buffer.insert_str("  ");
                }
                return;
            }
            _ => {}
        }

        // Pane-specific keys
        match self.focus {
            FocusPane::Sidebar => self.handle_sidebar_key(key),
            FocusPane::Editor => self.handle_editor_key(key),
        }
    }

    fn handle_sidebar_key(&mut self, key: KeyEvent) {
        let total_docs = self.workspace.docs.len();
        match key.code {
            KeyCode::Up => {
                if self.sidebar_selected_index > 0 {
                    self.sidebar_selected_index -= 1;
                }
            }
            KeyCode::Down => {
                if total_docs > 0 && self.sidebar_selected_index + 1 < total_docs {
                    self.sidebar_selected_index += 1;
                }
            }
            KeyCode::Enter => {
                if let Some(doc) = self.workspace.docs.get(self.sidebar_selected_index) {
                    let path = doc.path.clone();
                    self.open_document(&path);
                }
            }
            KeyCode::Esc => {
                self.focus = FocusPane::Editor;
            }
            _ => {}
        }
    }

    fn handle_editor_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Left => {
                if key.modifiers.contains(KeyModifiers::ALT) {
                    self.editor.buffer.move_word_left();
                } else {
                    self.editor.buffer.move_left();
                }
            }
            KeyCode::Right => {
                if key.modifiers.contains(KeyModifiers::ALT) {
                    self.editor.buffer.move_word_right();
                } else {
                    self.editor.buffer.move_right();
                }
            }
            KeyCode::Up => self.editor.buffer.move_up(),
            KeyCode::Down => self.editor.buffer.move_down(),
            KeyCode::Home => self.editor.buffer.move_home(),
            KeyCode::End => self.editor.buffer.move_end(),
            KeyCode::PageUp => self.editor.buffer.page_up(20),
            KeyCode::PageDown => self.editor.buffer.page_down(20),
            KeyCode::Backspace => {
                if key.modifiers.contains(KeyModifiers::ALT) {
                    self.editor.buffer.delete_word();
                } else {
                    self.editor.buffer.backspace();
                }
            }
            KeyCode::Delete => self.editor.buffer.delete(),
            KeyCode::Enter => self.editor.buffer.insert_newline(),
            KeyCode::Char(c) => self.editor.buffer.insert_char(c),
            _ => {}
        }
    }

    fn handle_palette_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => {
                self.mode = AppMode::Normal;
            }
            KeyCode::Up => {
                self.palette.move_up();
            }
            KeyCode::Down => {
                self.palette.move_down();
            }
            KeyCode::Backspace => {
                self.palette.backspace(&self.workspace, self.editor.file_path.as_ref());
            }
            KeyCode::Enter => {
                if let Some(item) = self.palette.selected_item().cloned() {
                    self.execute_palette_action(item.action);
                }
            }
            KeyCode::Char(c) => {
                self.palette.insert_char(c, &self.workspace, self.editor.file_path.as_ref());
            }
            _ => {}
        }
    }

    fn execute_palette_action(&mut self, action: PaletteAction) {
        match action {
            PaletteAction::OpenDocument(path) => {
                self.open_document(&path);
                self.mode = AppMode::Normal;
            }
            PaletteAction::CreateDocument(name) => {
                match self.workspace.create_document(&name) {
                    Ok(path) => {
                        self.open_document(&path);
                        self.status_message = format!("Created {}", name);
                    }
                    Err(e) => {
                        self.status_message = format!("Error creating doc: {}", e);
                    }
                }
                self.mode = AppMode::Normal;
            }
            PaletteAction::SearchText(query) => {
                self.search_query = query.clone();
                self.search_matches = self.workspace.search_content(&query);
                self.search_selected_index = 0;
                self.mode = AppMode::SearchResults;
            }
            PaletteAction::SaveDocument => {
                self.save_current_document();
                self.mode = AppMode::Normal;
            }
            PaletteAction::RunCodeBlock => {
                self.mode = AppMode::Normal;
                self.trigger_run_block();
            }
            PaletteAction::ToggleOutputCollapse => {
                let cursor_line = self.editor.buffer.cursor_line;
                Runner::toggle_output_collapse(&mut self.editor.buffer, cursor_line);
                self.mode = AppMode::Normal;
            }
            PaletteAction::ToggleSidebar => {
                self.sidebar_visible = !self.sidebar_visible;
                self.mode = AppMode::Normal;
            }
            PaletteAction::DeleteDocument(path) => {
                let _ = self.workspace.delete_document(&path);
                if let Some(first) = self.workspace.docs.first() {
                    let p = first.path.clone();
                    self.open_document(&p);
                } else {
                    self.editor = Editor::new();
                }
                self.status_message = "Document deleted".to_string();
                self.mode = AppMode::Normal;
            }
            PaletteAction::ShowHelp => {
                self.mode = AppMode::Help;
            }
            PaletteAction::Quit => {
                self.should_quit = true;
            }
        }
    }

    fn handle_search_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => {
                self.mode = AppMode::Normal;
            }
            KeyCode::Up => {
                if self.search_selected_index > 0 {
                    self.search_selected_index -= 1;
                }
            }
            KeyCode::Down => {
                if !self.search_matches.is_empty() && self.search_selected_index + 1 < self.search_matches.len() {
                    self.search_selected_index += 1;
                }
            }
            KeyCode::Enter => {
                if let Some(m) = self.search_matches.get(self.search_selected_index) {
                    let path = m.file_path.clone();
                    let line = m.line_number.saturating_sub(1);
                    self.open_document(&path);
                    self.editor.buffer.cursor_line = line.min(self.editor.line_count().saturating_sub(1));
                    self.editor.buffer.cursor_col = 0;
                    self.mode = AppMode::Normal;
                }
            }
            _ => {}
        }
    }

    fn handle_help_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc | KeyCode::F(1) | KeyCode::Enter | KeyCode::Char('q') => {
                self.mode = AppMode::Normal;
            }
            _ => {}
        }
    }

    pub fn save_current_document(&mut self) {
        match self.editor.save() {
            Ok(()) => {
                self.status_message = "Document saved successfully".to_string();
                let _ = self.workspace.refresh();
            }
            Err(e) => {
                self.status_message = format!("Error saving: {}", e);
            }
        }
    }

    pub fn trigger_run_block(&mut self) {
        if self.is_running_block {
            self.status_message = "A command is already running...".to_string();
            return;
        }

        let cursor_line = self.editor.buffer.cursor_line;
        if let Some(block) = Runner::find_code_block(&self.editor.buffer, cursor_line) {
            self.is_running_block = true;
            self.status_message = format!("Executing {} block...", block.language);

            let root_dir = self.workspace.root_dir.clone();
            let tx = self.event_tx.clone();

            tokio::spawn(async move {
                match Runner::execute(&block, &root_dir).await {
                    Ok(result) => {
                        let _ = tx.send(AppEvent::CodeExecuted { block, result });
                    }
                    Err(e) => {
                        let _ = tx.send(AppEvent::StatusMessage(format!("Execution failed: {}", e)));
                    }
                }
            });
        } else {
            self.status_message = "No code block found under cursor".to_string();
        }
    }
}
