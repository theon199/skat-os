use fuzzy_matcher::FuzzyMatcher;
use fuzzy_matcher::skim::SkimMatcherV2;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub enum PaletteAction {
    OpenDocument(PathBuf),
    CreateDocument(String),
    SearchText(String),
    SaveDocument,
    RunCodeBlock,
    RunAllBlocks,
    RunUpToCursor,
    ToggleTaskItem,
    ToggleOutputCollapse,
    ToggleSidebar,
    DeleteDocument(PathBuf),
    ShowHelp,
    ViewAuditLog,
    Quit,
}

#[derive(Debug, Clone)]
pub struct PaletteItem {
    pub label: String,
    pub subtext: String,
    pub action: PaletteAction,
    pub score: i64,
}

pub struct CommandPalette {
    pub query: String,
    pub selected_index: usize,
    pub items: Vec<PaletteItem>,
    pub is_search_mode: bool,
    matcher: SkimMatcherV2,
}

impl Default for CommandPalette {
    fn default() -> Self {
        Self::new()
    }
}

impl CommandPalette {
    pub fn new() -> Self {
        Self {
            query: String::new(),
            selected_index: 0,
            items: Vec::new(),
            is_search_mode: false,
            matcher: SkimMatcherV2::default(),
        }
    }

    pub fn reset(
        &mut self,
        workspace: &crate::workspace::Workspace,
        current_file: Option<&PathBuf>,
    ) {
        self.query.clear();
        self.selected_index = 0;
        self.is_search_mode = false;
        self.rebuild_items(workspace, current_file);
    }

    pub fn insert_char(
        &mut self,
        c: char,
        workspace: &crate::workspace::Workspace,
        current_file: Option<&PathBuf>,
    ) {
        self.query.push(c);
        self.rebuild_items(workspace, current_file);
    }

    pub fn backspace(
        &mut self,
        workspace: &crate::workspace::Workspace,
        current_file: Option<&PathBuf>,
    ) {
        self.query.pop();
        self.rebuild_items(workspace, current_file);
    }

    pub fn move_up(&mut self) {
        if self.selected_index > 0 {
            self.selected_index -= 1;
        }
    }

    pub fn move_down(&mut self) {
        if !self.items.is_empty() && self.selected_index + 1 < self.items.len() {
            self.selected_index += 1;
        }
    }

    pub fn selected_item(&self) -> Option<&PaletteItem> {
        self.items.get(self.selected_index)
    }

    pub fn rebuild_items(
        &mut self,
        workspace: &crate::workspace::Workspace,
        current_file: Option<&PathBuf>,
    ) {
        let mut candidates = Vec::new();
        let q = self.query.trim();

        // 1. Add all documents with fuzzy matching
        let mut document_matched = false;
        for doc in &workspace.docs {
            let search_string = format!("{} {}", doc.title, doc.relative_path);
            let score = if q.is_empty() {
                Some(100)
            } else {
                self.matcher.fuzzy_match(&search_string, q).map(|s| s + 500)
            };

            if let Some(s) = score {
                document_matched = true;
                candidates.push(PaletteItem {
                    label: doc.title.clone(),
                    subtext: doc.relative_path.clone(),
                    action: PaletteAction::OpenDocument(doc.path.clone()),
                    score: s,
                });
            }
        }

        // 2. System actions
        let system_actions = vec![
            (
                "Save Document",
                "Save active buffer to disk (Ctrl+S)",
                PaletteAction::SaveDocument,
            ),
            (
                "Run Code Block",
                "Execute code block under cursor (Ctrl+R)",
                PaletteAction::RunCodeBlock,
            ),
            (
                "Run All Blocks (Playbook)",
                "Execute all code blocks sequentially from top to bottom (Ctrl+Shift+R / F5)",
                PaletteAction::RunAllBlocks,
            ),
            (
                "Run Up To Cursor",
                "Execute all code blocks from line 0 up to current cursor position",
                PaletteAction::RunUpToCursor,
            ),
            (
                "Toggle Task Checkbox",
                "Toggle checklist item under cursor [ ] <-> [x] (Space)",
                PaletteAction::ToggleTaskItem,
            ),
            (
                "Toggle Output Block",
                "Collapse or expand output block (Ctrl+O)",
                PaletteAction::ToggleOutputCollapse,
            ),
            (
                "Toggle Sidebar",
                "Show/hide file and backlinks sidebar (Ctrl+B)",
                PaletteAction::ToggleSidebar,
            ),
            (
                "View Audit Log",
                "View immutable execution audit trail timeline and cryptographic hash verification",
                PaletteAction::ViewAuditLog,
            ),
            (
                "Help & Keybindings",
                "View cheatsheet and shortcuts (F1)",
                PaletteAction::ShowHelp,
            ),
            (
                "Quit skat-os",
                "Exit document operating system (Ctrl+Q)",
                PaletteAction::Quit,
            ),
        ];

        let mut system_matched = false;
        for (label, subtext, action) in system_actions {
            let score = if q.is_empty() {
                Some(10)
            } else {
                self.matcher.fuzzy_match(label, q).map(|s| s + 200)
            };

            if let Some(s) = score {
                if !q.is_empty() {
                    system_matched = true;
                }
                candidates.push(PaletteItem {
                    label: label.to_string(),
                    subtext: subtext.to_string(),
                    action,
                    score: s,
                });
            }
        }

        // 3. Offer "Create new document" and "Search in docs"
        if !q.is_empty() {
            let doc_name = if q.ends_with(".md") {
                q.to_string()
            } else {
                format!("{}.md", q)
            };

            // If a document or system action matched, prioritize matched item; otherwise place create on top
            let create_score = if document_matched || system_matched {
                50
            } else {
                1000
            };
            candidates.push(PaletteItem {
                label: format!("+ Create document: {}", doc_name),
                subtext: "Create and open new markdown note".to_string(),
                action: PaletteAction::CreateDocument(doc_name),
                score: create_score,
            });

            candidates.push(PaletteItem {
                label: format!("? Search in docs: \"{}\"", q),
                subtext: "Find all occurrences across workspace".to_string(),
                action: PaletteAction::SearchText(q.to_string()),
                score: 40,
            });
        }

        // Optional delete current document
        if let Some(path) = current_file {
            let label = "Delete Current Document";
            let score = if q.is_empty() {
                Some(5)
            } else {
                self.matcher.fuzzy_match(label, q)
            };
            if let Some(s) = score {
                candidates.push(PaletteItem {
                    label: label.to_string(),
                    subtext: path.to_string_lossy().to_string(),
                    action: PaletteAction::DeleteDocument(path.clone()),
                    score: s,
                });
            }
        }

        // Sort candidates by score descending
        candidates.sort_by_key(|a| std::cmp::Reverse(a.score));

        self.items = candidates;
        if self.selected_index >= self.items.len() {
            self.selected_index = 0;
        }
    }
}
