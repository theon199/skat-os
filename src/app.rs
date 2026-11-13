use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tokio::sync::mpsc;

use crate::collab::{CollabClient, CollabMessage, CollabServer, CollabServerHandle, PeerInfo};
use crate::editor::Editor;
use crate::palette::{CommandPalette, PaletteAction};
use crate::runner::{CodeBlock, ExecutionResult, Runner};
use crate::workspace::{SearchMatch, Workspace};

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum PendingAction {
    Quit,
    OpenDocument {
        path: PathBuf,
        target_line: Option<usize>,
    },
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum AppMode {
    Normal,
    CommandPalette,
    SearchResults,
    Help,
    ParamPrompt {
        param_name: String,
        input_value: String,
        param_line: usize,
        start_col: usize,
        end_col: usize,
        is_secret: bool,
    },
    TableInspectModal {
        block_id: String,
        row_index: usize,
    },
    ConfirmExecution {
        block_id: String,
        prompt: String,
        command: String,
        target_hosts: Vec<String>,
        danger_level: crate::guard::DangerLevel,
        keyword_required: Option<String>,
        keyword_input: String,
    },
    AuditLogModal {
        selected_index: usize,
        search_query: String,
        integrity_verified: bool,
    },
    UnsavedChangesModal {
        pending_action: PendingAction,
        doc_name: String,
    },
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum FocusPane {
    Sidebar,
    Editor,
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum CollabMode {
    Offline,
    Host { port: u16 },
    Client { addr: String },
}

pub struct RunningCommand {
    pub block_id: String,
    pub stdin_tx: mpsc::UnboundedSender<Vec<u8>>,
    pub cancel_tx: Option<tokio::sync::oneshot::Sender<()>>,
    pub accumulated_output: String,
}

pub enum AppEvent {
    CodeExecuted {
        block: CodeBlock,
        result: ExecutionResult,
    },
    StreamChunk {
        block_id: String,
        chunk: String,
    },
    StreamFinished {
        block: CodeBlock,
        result: ExecutionResult,
    },
    StreamInterrupted {
        block_id: String,
    },
    PlaybookStepStarted {
        block_id: String,
        block_name: Option<String>,
        index: usize,
        total: usize,
    },
    PlaybookStepChunk {
        block_id: String,
        chunk: String,
    },
    PlaybookStepFinished {
        block: CodeBlock,
        result: ExecutionResult,
        index: usize,
        total: usize,
    },
    PlaybookHalted {
        block_id: String,
        block_name: Option<String>,
        index: usize,
        total: usize,
        exit_code: i32,
    },
    PlaybookCompleted {
        total: usize,
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
    pub running_command: Option<RunningCommand>,
    pub remote_streams: HashMap<String, String>,
    pub search_query: String,
    pub search_matches: Vec<SearchMatch>,
    pub search_selected_index: usize,
    pub should_quit: bool,
    pub event_tx: mpsc::UnboundedSender<AppEvent>,
    pub event_rx: mpsc::UnboundedReceiver<AppEvent>,

    // Dynamic Runbook Parameters & Structured Tables
    pub params: HashMap<String, String>,
    pub table_states: HashMap<String, crate::table::TableData>,

    // Multi-Host Fleet Mesh & SSH Session Pool
    pub fleet_states: HashMap<String, crate::mesh::FleetOutput>,
    pub ssh_pool: crate::mesh::SshSessionPool,

    // Multiplayer Collaboration
    pub collab_mode: CollabMode,
    pub collab_client: Option<CollabClient>,
    pub server_handle: Option<CollabServerHandle>,
    pub peers: HashMap<String, PeerInfo>,
    pub my_peer_id: Option<String>,
}

impl App {
    pub fn new(root_dir: PathBuf) -> Result<Self> {
        let workspace = Workspace::new(root_dir)?;
        let mut editor = Editor::new();

        // Load first document if available (e.g. welcome.md)
        if let Some(first_doc) = workspace.docs.first()
            && let Ok(ed) = Editor::from_file(&first_doc.path)
        {
            editor = ed;
        }

        let (event_tx, event_rx) = mpsc::unbounded_channel();
        let params = crate::form::FormParser::collect_params(&editor.buffer);
        let table_states = crate::table::TableData::collect_tables(&editor.buffer);
        let mut fleet_states = HashMap::new();
        for b in crate::runner::Runner::all_code_blocks(&editor.buffer) {
            if let Some(out) = &b.existing_output
                && let Some(fleet) = crate::mesh::FleetOutput::parse(&out.output_text)
            {
                fleet_states.insert(b.id.clone(), fleet.clone());
                if let Some(n) = &b.name {
                    fleet_states.insert(n.clone(), fleet.clone());
                }
                if let Some(t) = &b.target
                    && let Some(g) = t.strip_prefix('@')
                {
                    fleet_states.insert(g.to_string(), fleet);
                }
            }
        }

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
            running_command: None,
            remote_streams: HashMap::new(),
            search_query: String::new(),
            search_matches: Vec::new(),
            search_selected_index: 0,
            should_quit: false,
            event_tx,
            event_rx,
            params,
            table_states,
            fleet_states,
            ssh_pool: crate::mesh::global_ssh_pool().clone(),
            collab_mode: CollabMode::Offline,
            collab_client: None,
            server_handle: None,
            peers: HashMap::new(),
            my_peer_id: None,
        })
    }

    pub fn refresh_params_and_tables(&mut self) {
        self.params = crate::form::FormParser::collect_params(&self.editor.buffer);
        let discovered = crate::table::TableData::collect_tables(&self.editor.buffer);
        for (k, v) in discovered {
            self.table_states.entry(k).or_insert(v);
        }
        for b in crate::runner::Runner::all_code_blocks(&self.editor.buffer) {
            if let Some(out) = &b.existing_output
                && let Some(fleet) = crate::mesh::FleetOutput::parse(&out.output_text)
            {
                self.fleet_states
                    .entry(b.id.clone())
                    .or_insert_with(|| fleet.clone());
                if let Some(n) = &b.name {
                    self.fleet_states
                        .entry(n.clone())
                        .or_insert_with(|| fleet.clone());
                }
                if let Some(t) = &b.target
                    && let Some(g) = t.strip_prefix('@')
                {
                    self.fleet_states.entry(g.to_string()).or_insert(fleet);
                }
            }
        }
    }

    pub fn set_param(&mut self, param_name: &str, value: &str) {
        self.params
            .insert(param_name.to_string(), value.to_string());
        crate::form::FormParser::update_param_in_buffer(&mut self.editor.buffer, param_name, value);
        if let Some(client) = &self.collab_client {
            client.send_param_update(param_name.to_string(), value.to_string());
        }
    }

    pub fn set_param_from_remote(&mut self, param_name: &str, value: &str) {
        self.params
            .insert(param_name.to_string(), value.to_string());
        crate::form::FormParser::update_param_in_buffer(&mut self.editor.buffer, param_name, value);
    }

    pub async fn new_host(root_dir: PathBuf, port: u16, host_name: String) -> Result<Self> {
        let mut app = Self::new(root_dir)?;
        let initial_content = app.editor.buffer.get_text();
        let doc_name = app
            .editor
            .file_path
            .as_ref()
            .and_then(|p| p.file_name())
            .map(|f| f.to_string_lossy().to_string())
            .unwrap_or_else(|| "welcome.md".to_string());

        let bind_addr = format!("0.0.0.0:{}", port);
        let server_handle = CollabServer::bind(&bind_addr, initial_content, doc_name).await?;
        let actual_port = server_handle.local_addr.port();

        let client =
            CollabClient::connect(&format!("127.0.0.1:{}", actual_port), host_name).await?;
        app.my_peer_id = Some(client.my_id.clone());
        for p in &client.initial_peers {
            app.peers.insert(p.id.clone(), p.clone());
        }

        app.collab_mode = CollabMode::Host { port: actual_port };
        app.collab_client = Some(client);
        app.server_handle = Some(server_handle);
        app.status_message = format!("Hosting collaboration room on port {}", actual_port);

        Ok(app)
    }

    pub async fn new_client(addr: &str, client_name: String) -> Result<Self> {
        let client = CollabClient::connect(addr, client_name).await?;
        let temp_dir = std::env::temp_dir().join("skat-os-client");
        let _ = std::fs::create_dir_all(&temp_dir);
        let mut app = Self::new(temp_dir)?;

        app.editor.buffer = crate::editor::TextBuffer::from_str(&client.initial_content);
        app.editor.file_path = Some(PathBuf::from(&client.doc_name));
        app.my_peer_id = Some(client.my_id.clone());
        for p in &client.initial_peers {
            app.peers.insert(p.id.clone(), p.clone());
        }

        app.collab_mode = CollabMode::Client {
            addr: addr.to_string(),
        };
        app.status_message = format!("Joined collaboration room at {}", addr);
        app.collab_client = Some(client);

        Ok(app)
    }

    pub fn broadcast_doc_sync(&mut self) {
        if let Some(client) = &self.collab_client {
            let doc_name = self
                .editor
                .file_path
                .as_ref()
                .and_then(|p| p.file_name())
                .map(|f| f.to_string_lossy().to_string())
                .unwrap_or_else(|| "Untitled.md".to_string());
            let content = self.editor.buffer.get_text();
            client.send(CollabMessage::DocSync {
                content,
                doc_name,
                version: 0,
                your_id: client.my_id.clone(),
                peers: self.peers.values().cloned().collect(),
            });
        }
    }

    pub fn open_document(&mut self, path: &Path) {
        self.open_document_with_line(path, None);
    }

    pub fn open_document_with_line(&mut self, path: &Path, target_line: Option<usize>) {
        let is_different = self.editor.file_path.as_deref() != Some(path);
        if is_different && self.editor.is_dirty() {
            let doc_name = self
                .editor
                .file_path
                .as_ref()
                .and_then(|p| p.file_name())
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "current document".to_string());
            self.mode = AppMode::UnsavedChangesModal {
                pending_action: PendingAction::OpenDocument {
                    path: path.to_path_buf(),
                    target_line,
                },
                doc_name,
            };
            return;
        }

        self.force_open_document(path);
        if let Some(line) = target_line {
            self.editor.buffer.cursor_line = line.min(self.editor.line_count().saturating_sub(1));
            self.editor.buffer.cursor_col = 0;
        }
    }

    pub fn force_open_document(&mut self, path: &Path) {
        if let Ok(new_editor) = Editor::from_file(path) {
            self.editor = new_editor;
            self.focus = FocusPane::Editor;
            self.refresh_params_and_tables();
            let file_name = path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            self.status_message = format!("Opened {}", file_name);
            if matches!(self.collab_mode, CollabMode::Host { .. }) {
                self.broadcast_doc_sync();
            }
        }
    }

    pub fn handle_app_events(&mut self) {
        // 1. Internal async event processing
        while let Ok(event) = self.event_rx.try_recv() {
            match event {
                AppEvent::CodeExecuted { block, result } => {
                    self.is_running_block = false;
                    Runner::apply_output(&mut self.editor.buffer, &block, &result);
                    self.status_message =
                        format!("Execution finished [exit: {}]", result.exit_code);

                    if let Some(tbl) = crate::table::TableData::parse(&result.stdout) {
                        self.table_states.insert(block.id.clone(), tbl.clone());
                        if let Some(name) = &block.name {
                            self.table_states.insert(name.clone(), tbl);
                        }
                    }

                    if let Some(fleet) = crate::mesh::FleetOutput::parse(&result.stdout) {
                        self.fleet_states.insert(block.id.clone(), fleet.clone());
                        if let Some(name) = &block.name {
                            self.fleet_states.insert(name.clone(), fleet.clone());
                        }
                        if let Some(target) = &block.target
                            && let Some(group) = target.strip_prefix('@')
                        {
                            self.fleet_states.insert(group.to_string(), fleet);
                        }
                    }

                    if let Some(client) = &self.collab_client {
                        let mut output_content = String::new();
                        if !result.stdout.is_empty() {
                            output_content.push_str(&result.stdout);
                        }
                        if !result.stderr.is_empty() {
                            if !output_content.is_empty() && !output_content.ends_with('\n') {
                                output_content.push('\n');
                            }
                            output_content.push_str(&result.stderr);
                        }
                        client.send_output_update(
                            block.id.clone(),
                            output_content,
                            result.exit_code,
                        );
                    }
                }
                AppEvent::StreamChunk { block_id, chunk } => {
                    if let Some(ref mut cmd) = self.running_command {
                        if cmd.block_id == block_id {
                            Runner::append_stream_chunk(
                                &mut self.editor.buffer,
                                &block_id,
                                &chunk,
                                &mut cmd.accumulated_output,
                            );
                        } else {
                            Runner::append_stream_chunk(
                                &mut self.editor.buffer,
                                &block_id,
                                &chunk,
                                &mut String::new(),
                            );
                        }
                    } else {
                        Runner::append_stream_chunk(
                            &mut self.editor.buffer,
                            &block_id,
                            &chunk,
                            &mut String::new(),
                        );
                    }

                    if let Some(b) = Runner::find_code_block_by_id(&self.editor.buffer, &block_id)
                        && let Some(out) = Runner::find_following_output_block(
                            &self.editor.buffer,
                            b.end_fence_line,
                        )
                    {
                        self.editor.buffer.cursor_line = out.end_line;
                    }

                    if let Some(client) = &self.collab_client {
                        client.send_stream_chunk(block_id, chunk);
                    }
                }
                AppEvent::StreamFinished { block, result } => {
                    self.running_command = None;
                    self.is_running_block = false;
                    Runner::apply_output(&mut self.editor.buffer, &block, &result);
                    self.status_message =
                        format!("Execution finished [exit: {}]", result.exit_code);

                    if let Some(tbl) = crate::table::TableData::parse(&result.stdout) {
                        self.table_states.insert(block.id.clone(), tbl.clone());
                        if let Some(name) = &block.name {
                            self.table_states.insert(name.clone(), tbl);
                        }
                    }

                    if let Some(fleet) = crate::mesh::FleetOutput::parse(&result.stdout) {
                        self.fleet_states.insert(block.id.clone(), fleet.clone());
                        if let Some(name) = &block.name {
                            self.fleet_states.insert(name.clone(), fleet.clone());
                        }
                        if let Some(target) = &block.target
                            && let Some(group) = target.strip_prefix('@')
                        {
                            self.fleet_states.insert(group.to_string(), fleet);
                        }
                    }

                    if let Some(client) = &self.collab_client {
                        client.send_stream_end(block.id.clone(), result.exit_code);
                    }
                }
                AppEvent::StreamInterrupted { block_id } => {
                    self.running_command = None;
                    self.is_running_block = false;
                    Runner::apply_streaming_status(
                        &mut self.editor.buffer,
                        &block_id,
                        crate::runner::OutputStatus::Interrupted,
                        "",
                    );
                    self.status_message = "Command interrupted [Ctrl+C]".to_string();

                    if let Some(client) = &self.collab_client {
                        client.send_stream_end(block_id, 130);
                    }
                }
                AppEvent::PlaybookStepStarted {
                    block_id,
                    block_name,
                    index,
                    total,
                } => {
                    let display_name = block_name.unwrap_or_else(|| block_id.clone());
                    self.status_message =
                        format!("Running step {}/{}: {}...", index + 1, total, display_name);
                    if let Some(client) = &self.collab_client {
                        client.send_stream_start(block_id);
                    }
                }
                AppEvent::PlaybookStepChunk { block_id, chunk } => {
                    if let Some(ref mut cmd) = self.running_command {
                        if cmd.block_id == block_id {
                            Runner::append_stream_chunk(
                                &mut self.editor.buffer,
                                &block_id,
                                &chunk,
                                &mut cmd.accumulated_output,
                            );
                        } else {
                            Runner::append_stream_chunk(
                                &mut self.editor.buffer,
                                &block_id,
                                &chunk,
                                &mut String::new(),
                            );
                        }
                    } else {
                        Runner::append_stream_chunk(
                            &mut self.editor.buffer,
                            &block_id,
                            &chunk,
                            &mut String::new(),
                        );
                    }

                    if let Some(b) = Runner::find_code_block_by_id(&self.editor.buffer, &block_id)
                        && let Some(out) = Runner::find_following_output_block(
                            &self.editor.buffer,
                            b.end_fence_line,
                        )
                    {
                        self.editor.buffer.cursor_line = out.end_line;
                    }

                    if let Some(client) = &self.collab_client {
                        client.send_stream_chunk(block_id, chunk);
                    }
                }
                AppEvent::PlaybookStepFinished {
                    block,
                    result,
                    index: _,
                    total: _,
                } => {
                    let mut output_content = String::new();
                    if !result.stdout.is_empty() {
                        output_content.push_str(&result.stdout);
                    }
                    if !result.stderr.is_empty() {
                        if !output_content.is_empty() && !output_content.ends_with('\n') {
                            output_content.push('\n');
                        }
                        output_content.push_str(&result.stderr);
                    }
                    if output_content.is_empty() {
                        output_content = "(No output)".to_string();
                    }

                    Runner::apply_output_by_id(
                        &mut self.editor.buffer,
                        &block.id,
                        &output_content,
                        result.exit_code,
                    );

                    if let Some(tbl) = crate::table::TableData::parse(&result.stdout) {
                        self.table_states.insert(block.id.clone(), tbl.clone());
                        if let Some(name) = &block.name {
                            self.table_states.insert(name.clone(), tbl);
                        }
                    }

                    if let Some(fleet) = crate::mesh::FleetOutput::parse(&result.stdout) {
                        self.fleet_states.insert(block.id.clone(), fleet.clone());
                        if let Some(name) = &block.name {
                            self.fleet_states.insert(name.clone(), fleet.clone());
                        }
                        if let Some(target) = &block.target
                            && let Some(group) = target.strip_prefix('@')
                        {
                            self.fleet_states.insert(group.to_string(), fleet);
                        }
                    }

                    if let Some(client) = &self.collab_client {
                        client.send_output_update(
                            block.id.clone(),
                            output_content,
                            result.exit_code,
                        );
                    }
                }
                AppEvent::PlaybookHalted {
                    block_id,
                    block_name,
                    index,
                    total,
                    exit_code,
                } => {
                    self.is_running_block = false;
                    let display_name = block_name.unwrap_or(block_id);
                    self.status_message = format!(
                        "Playbook halted at step {}/{} ({}) [exit: {}]",
                        index + 1,
                        total,
                        display_name,
                        exit_code
                    );
                }
                AppEvent::PlaybookCompleted { total } => {
                    self.is_running_block = false;
                    self.status_message =
                        format!("Playbook completed successfully ({} steps)", total);
                }
                AppEvent::StatusMessage(msg) => {
                    self.status_message = msg;
                }
            }
        }

        // 2. Incoming multiplayer network messages
        let mut incoming_messages = Vec::new();
        let mut disconnected = false;
        if let Some(client) = &mut self.collab_client {
            while let Some(msg) = client.try_recv() {
                incoming_messages.push(msg);
            }
            if !client.is_connected() && incoming_messages.is_empty() {
                disconnected = true;
            }
        }
        if disconnected {
            if matches!(self.collab_mode, CollabMode::Client { .. }) {
                self.status_message = "Disconnected from host: connection closed".to_string();
                self.collab_mode = CollabMode::Offline;
                self.collab_client = None;
            } else if matches!(self.collab_mode, CollabMode::Host { .. }) {
                self.collab_client = None;
            }
        }

        for msg in incoming_messages {
            match msg {
                CollabMessage::DocSync {
                    content,
                    doc_name,
                    version: _,
                    your_id: _,
                    peers,
                } => {
                    self.editor.buffer = crate::editor::TextBuffer::from_str(&content);
                    self.editor.file_path = Some(PathBuf::from(doc_name));
                    self.editor.buffer.cursor_line = 0;
                    self.editor.buffer.cursor_col = 0;
                    for p in peers {
                        self.peers.insert(p.id.clone(), p);
                    }
                    self.refresh_params_and_tables();
                    self.status_message = "Document synchronized with room".to_string();
                }
                CollabMessage::PeerJoined { peer } => {
                    let name = peer.name.clone();
                    self.peers.insert(peer.id.clone(), peer);
                    self.status_message = format!("Peer {} joined", name);
                }
                CollabMessage::PeerLeft { peer_id } => {
                    if let Some(p) = self.peers.remove(&peer_id) {
                        self.status_message = format!("Peer {} left the room", p.name);
                    }
                }
                CollabMessage::InsertChar { peer_id, pos, char } => {
                    self.editor.buffer.apply_remote_insert(pos, char);
                    if let Some(peer) = self.peers.get_mut(&peer_id) {
                        let (l, c) = self.editor.buffer.char_to_line_col(pos + 1);
                        peer.cursor_line = l;
                        peer.cursor_col = c;
                    }
                }
                CollabMessage::DeleteChar { peer_id, pos } => {
                    self.editor.buffer.apply_remote_delete(pos);
                    if let Some(peer) = self.peers.get_mut(&peer_id) {
                        let (l, c) = self.editor.buffer.char_to_line_col(pos);
                        peer.cursor_line = l;
                        peer.cursor_col = c;
                    }
                }
                CollabMessage::CursorMove { peer_id, line, col } => {
                    if let Some(peer) = self.peers.get_mut(&peer_id) {
                        peer.cursor_line = line;
                        peer.cursor_col = col;
                    }
                }
                CollabMessage::RunBlock { peer_id, block_id } => {
                    let peer_name = self
                        .peers
                        .get(&peer_id)
                        .map(|p| p.name.as_str())
                        .unwrap_or("Peer");
                    self.status_message = format!("{} is running block {}...", peer_name, block_id);
                }
                CollabMessage::OutputUpdate {
                    peer_id,
                    block_id,
                    text,
                    exit_code,
                } => {
                    let peer_name = self
                        .peers
                        .get(&peer_id)
                        .map(|p| p.name.as_str())
                        .unwrap_or("Peer");
                    Runner::apply_output_by_id(
                        &mut self.editor.buffer,
                        &block_id,
                        &text,
                        exit_code,
                    );
                    self.status_message =
                        format!("Output updated [exit: {}] by {}", exit_code, peer_name);
                }
                CollabMessage::ToggleTask {
                    peer_id,
                    line,
                    checked,
                } => {
                    let peer_name = self
                        .peers
                        .get(&peer_id)
                        .map(|p| p.name.as_str())
                        .unwrap_or("Peer");
                    if self.editor.buffer.set_task_checked(line, checked) {
                        self.status_message = format!(
                            "Task {} by {}",
                            if checked { "completed" } else { "reopened" },
                            peer_name
                        );
                    }
                }
                CollabMessage::StreamStart { peer_id, block_id } => {
                    let peer_name = self
                        .peers
                        .get(&peer_id)
                        .map(|p| p.name.as_str())
                        .unwrap_or("Peer");
                    self.remote_streams.insert(block_id.clone(), String::new());
                    Runner::apply_streaming_status(
                        &mut self.editor.buffer,
                        &block_id,
                        crate::runner::OutputStatus::Running,
                        "",
                    );
                    self.status_message =
                        format!("{} started streaming block {}...", peer_name, block_id);
                }
                CollabMessage::StreamChunk {
                    peer_id,
                    block_id,
                    chunk,
                } => {
                    let peer_name = self
                        .peers
                        .get(&peer_id)
                        .map(|p| p.name.as_str())
                        .unwrap_or("Peer");
                    let mut acc = self.remote_streams.remove(&block_id).unwrap_or_default();
                    Runner::append_stream_chunk(
                        &mut self.editor.buffer,
                        &block_id,
                        &chunk,
                        &mut acc,
                    );
                    self.remote_streams.insert(block_id.clone(), acc);

                    if let Some(b) = Runner::find_code_block_by_id(&self.editor.buffer, &block_id)
                        && let Some(out) = Runner::find_following_output_block(
                            &self.editor.buffer,
                            b.end_fence_line,
                        )
                    {
                        self.editor.buffer.cursor_line = out.end_line;
                    }

                    self.status_message =
                        format!("{} is streaming block {}...", peer_name, block_id);
                }
                CollabMessage::StreamEnd {
                    peer_id,
                    block_id,
                    exit_code,
                } => {
                    let peer_name = self
                        .peers
                        .get(&peer_id)
                        .map(|p| p.name.as_str())
                        .unwrap_or("Peer");
                    let acc = self.remote_streams.remove(&block_id).unwrap_or_default();
                    let status = if exit_code == 130 {
                        crate::runner::OutputStatus::Interrupted
                    } else {
                        crate::runner::OutputStatus::Exit(exit_code)
                    };
                    let processed = crate::ansi::process_terminal_stream(&acc);
                    Runner::apply_streaming_status(
                        &mut self.editor.buffer,
                        &block_id,
                        status,
                        &processed,
                    );
                    self.status_message = format!(
                        "Block {} finished [exit: {}] by {}",
                        block_id, exit_code, peer_name
                    );
                }
                CollabMessage::ParamUpdate {
                    peer_id,
                    param_name,
                    value,
                } => {
                    let peer_name = self
                        .peers
                        .get(&peer_id)
                        .map(|p| p.name.clone())
                        .unwrap_or_else(|| "Peer".to_string());
                    self.set_param_from_remote(&param_name, &value);
                    self.status_message = format!(
                        "Parameter '{}' set to '{}' by {}",
                        param_name, value, peer_name
                    );
                }
                CollabMessage::ApprovalRequest {
                    peer_id,
                    block_id,
                    command_preview: _,
                    required_approvals,
                } => {
                    let peer_name = self
                        .peers
                        .get(&peer_id)
                        .map(|p| p.name.as_str())
                        .unwrap_or("Peer");
                    self.status_message = format!(
                        "Peer {} requested approval for block {} (needs {}). Casting vote...",
                        peer_name, block_id, required_approvals
                    );
                    if let Some(client) = &self.collab_client {
                        client.send_approval_vote(block_id.clone());
                    }
                }
                CollabMessage::ApprovalVote {
                    peer_id: _,
                    block_id,
                    approver_name,
                } => {
                    self.status_message = format!(
                        "Approval vote received from {} for block {}",
                        approver_name, block_id
                    );
                }
                CollabMessage::ApprovalStatus {
                    block_id,
                    approvals_count,
                    required_approvals,
                    approved,
                } => {
                    if approved {
                        self.status_message = format!(
                            "Consensus approval reached ({}/{})! Dispatching block {}...",
                            approvals_count, required_approvals, block_id
                        );
                        self.execute_block_by_id(&block_id);
                    } else {
                        self.status_message = format!(
                            "Block {} approvals: {}/{}",
                            block_id, approvals_count, required_approvals
                        );
                    }
                }
                _ => {}
            }
        }
    }

    pub fn handle_key_event(&mut self, key: KeyEvent) {
        match self.mode {
            AppMode::CommandPalette => self.handle_palette_key(key),
            AppMode::SearchResults => self.handle_search_key(key),
            AppMode::Help => self.handle_help_key(key),
            AppMode::ParamPrompt { .. } => self.handle_param_prompt_key(key),
            AppMode::TableInspectModal { .. } => self.handle_table_inspect_key(key),
            AppMode::ConfirmExecution { .. } => self.handle_confirm_execution_key(key),
            AppMode::AuditLogModal { .. } => self.handle_audit_log_key(key),
            AppMode::UnsavedChangesModal { .. } => self.handle_unsaved_changes_key(key),
            AppMode::Normal => self.handle_normal_key(key),
        }
    }

    fn handle_unsaved_changes_key(&mut self, key: KeyEvent) {
        if let AppMode::UnsavedChangesModal { pending_action, .. } = self.mode.clone() {
            match key.code {
                KeyCode::Char('s') | KeyCode::Char('S') | KeyCode::Enter => {
                    self.save_current_document();
                    match pending_action {
                        PendingAction::Quit => {
                            self.should_quit = true;
                        }
                        PendingAction::OpenDocument { path, target_line } => {
                            self.force_open_document(&path);
                            if let Some(line) = target_line {
                                self.editor.buffer.cursor_line =
                                    line.min(self.editor.line_count().saturating_sub(1));
                                self.editor.buffer.cursor_col = 0;
                            }
                            self.mode = AppMode::Normal;
                        }
                    }
                }
                KeyCode::Char('d') | KeyCode::Char('D') => match pending_action {
                    PendingAction::Quit => {
                        self.should_quit = true;
                    }
                    PendingAction::OpenDocument { path, target_line } => {
                        self.force_open_document(&path);
                        if let Some(line) = target_line {
                            self.editor.buffer.cursor_line =
                                line.min(self.editor.line_count().saturating_sub(1));
                            self.editor.buffer.cursor_col = 0;
                        }
                        self.mode = AppMode::Normal;
                    }
                },
                KeyCode::Char('c') | KeyCode::Char('C') | KeyCode::Esc => {
                    self.status_message = "Action cancelled".to_string();
                    self.mode = AppMode::Normal;
                }
                _ => {}
            }
        }
    }

    fn handle_param_prompt_key(&mut self, key: KeyEvent) {
        if let AppMode::ParamPrompt {
            param_name,
            mut input_value,
            param_line,
            start_col,
            end_col,
            is_secret,
        } = self.mode.clone()
        {
            match key.code {
                KeyCode::Esc => {
                    self.mode = AppMode::Normal;
                    self.status_message = "Cancelled parameter edit".to_string();
                }
                KeyCode::Enter => {
                    self.mode = AppMode::Normal;
                    self.set_param(&param_name, &input_value);
                    self.status_message = if is_secret {
                        format!("Secret parameter '{}' updated", param_name)
                    } else {
                        format!("Parameter '{}' set to '{}'", param_name, input_value)
                    };
                }
                KeyCode::Backspace => {
                    input_value.pop();
                    self.mode = AppMode::ParamPrompt {
                        param_name,
                        input_value,
                        param_line,
                        start_col,
                        end_col,
                        is_secret,
                    };
                }
                KeyCode::Char(c) => {
                    input_value.push(c);
                    self.mode = AppMode::ParamPrompt {
                        param_name,
                        input_value,
                        param_line,
                        start_col,
                        end_col,
                        is_secret,
                    };
                }
                _ => {}
            }
        }
    }

    fn handle_confirm_execution_key(&mut self, key: KeyEvent) {
        if let AppMode::ConfirmExecution {
            block_id,
            prompt,
            command,
            target_hosts,
            danger_level,
            keyword_required,
            mut keyword_input,
        } = self.mode.clone()
        {
            match key.code {
                KeyCode::Esc => {
                    self.mode = AppMode::Normal;
                    self.status_message = "Execution cancelled".to_string();
                }
                KeyCode::Enter => {
                    if let Some(ref kw) = keyword_required
                        && !keyword_input.trim().eq_ignore_ascii_case(kw.trim())
                    {
                        self.status_message = format!(
                            "Confirmation keyword mismatch! Please type '{}' to confirm.",
                            kw
                        );
                        return;
                    }
                    self.mode = AppMode::Normal;
                    self.execute_block_by_id(&block_id);
                }
                KeyCode::Backspace => {
                    keyword_input.pop();
                    self.mode = AppMode::ConfirmExecution {
                        block_id,
                        prompt,
                        command,
                        target_hosts,
                        danger_level,
                        keyword_required,
                        keyword_input,
                    };
                }
                KeyCode::Char(c) => {
                    keyword_input.push(c);
                    self.mode = AppMode::ConfirmExecution {
                        block_id,
                        prompt,
                        command,
                        target_hosts,
                        danger_level,
                        keyword_required,
                        keyword_input,
                    };
                }
                _ => {}
            }
        }
    }

    fn handle_audit_log_key(&mut self, key: KeyEvent) {
        if let AppMode::AuditLogModal {
            mut selected_index,
            mut search_query,
            integrity_verified,
        } = self.mode.clone()
        {
            match key.code {
                KeyCode::Esc => {
                    self.mode = AppMode::Normal;
                }
                KeyCode::Up => {
                    if selected_index > 0 {
                        selected_index -= 1;
                        self.mode = AppMode::AuditLogModal {
                            selected_index,
                            search_query,
                            integrity_verified,
                        };
                    }
                }
                KeyCode::Down => {
                    selected_index += 1;
                    self.mode = AppMode::AuditLogModal {
                        selected_index,
                        search_query,
                        integrity_verified,
                    };
                }
                KeyCode::Backspace => {
                    search_query.pop();
                    self.mode = AppMode::AuditLogModal {
                        selected_index: 0,
                        search_query,
                        integrity_verified,
                    };
                }
                KeyCode::Char(c) => {
                    search_query.push(c);
                    self.mode = AppMode::AuditLogModal {
                        selected_index: 0,
                        search_query,
                        integrity_verified,
                    };
                }
                KeyCode::Enter => {
                    self.mode = AppMode::Normal;
                }
                _ => {}
            }
        }
    }

    fn handle_table_inspect_key(&mut self, key: KeyEvent) {
        if let AppMode::TableInspectModal {
            block_id,
            mut row_index,
        } = self.mode.clone()
        {
            let total_rows = self
                .table_states
                .get(&block_id)
                .map(|t| t.rows.len())
                .unwrap_or(0);

            match key.code {
                KeyCode::Esc | KeyCode::Enter | KeyCode::Char('q') => {
                    self.mode = AppMode::Normal;
                }
                KeyCode::Down | KeyCode::Char('j')
                    if total_rows > 0 && row_index + 1 < total_rows =>
                {
                    row_index += 1;
                    if let Some(tbl) = self.table_states.get_mut(&block_id) {
                        tbl.selected_row = row_index;
                    }
                    self.mode = AppMode::TableInspectModal {
                        block_id,
                        row_index,
                    };
                }
                KeyCode::Up | KeyCode::Char('k') if row_index > 0 => {
                    row_index -= 1;
                    if let Some(tbl) = self.table_states.get_mut(&block_id) {
                        tbl.selected_row = row_index;
                    }
                    self.mode = AppMode::TableInspectModal {
                        block_id,
                        row_index,
                    };
                }
                _ => {}
            }
        }
    }

    fn handle_normal_key(&mut self, key: KeyEvent) {
        // Playbook execution shortcut: Ctrl+Shift+R or F5 triggers "Run All Blocks"
        let is_ctrl_shift_r = key.modifiers.contains(KeyModifiers::CONTROL)
            && ((key.modifiers.contains(KeyModifiers::SHIFT)
                && (key.code == KeyCode::Char('r') || key.code == KeyCode::Char('R')))
                || key.code == KeyCode::Char('R'));
        if is_ctrl_shift_r || key.code == KeyCode::F(5) {
            self.trigger_run_all_blocks();
            return;
        }

        // Global shortcuts
        if key.modifiers.contains(KeyModifiers::CONTROL) {
            match key.code {
                KeyCode::Char('q') => {
                    if self.editor.is_dirty() {
                        let doc_name = self
                            .editor
                            .file_path
                            .as_ref()
                            .and_then(|p| p.file_name())
                            .map(|n| n.to_string_lossy().to_string())
                            .unwrap_or_else(|| "current document".to_string());
                        self.mode = AppMode::UnsavedChangesModal {
                            pending_action: PendingAction::Quit,
                            doc_name,
                        };
                        return;
                    }
                    self.should_quit = true;
                    return;
                }
                KeyCode::Char('c') => {
                    if let Some(cmd) = self.running_command.take() {
                        if let Some(cancel) = cmd.cancel_tx {
                            let _ = cancel.send(());
                        }
                        self.is_running_block = false;
                        let final_text =
                            crate::ansi::process_terminal_stream(&cmd.accumulated_output);
                        Runner::apply_streaming_status(
                            &mut self.editor.buffer,
                            &cmd.block_id,
                            crate::runner::OutputStatus::Interrupted,
                            &final_text,
                        );
                        self.status_message = "Command interrupted [Ctrl+C]".to_string();
                        if let Some(client) = &self.collab_client {
                            client.send_stream_end(cmd.block_id, 130);
                        }
                        return;
                    } else {
                        self.should_quit = true;
                        return;
                    }
                }
                KeyCode::Char('k') => {
                    self.mode = AppMode::CommandPalette;
                    self.palette
                        .reset(&self.workspace, self.editor.file_path.as_ref());
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
                    if key.modifiers.contains(KeyModifiers::SHIFT) {
                        self.trigger_run_all_blocks();
                    } else {
                        self.trigger_run_block();
                    }
                    return;
                }
                KeyCode::Char('R') => {
                    self.trigger_run_all_blocks();
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

        // If a command is actively running, forward interactive keystrokes to stdin
        if self.is_running_block
            && let Some(ref cmd) = self.running_command
        {
            match key.code {
                KeyCode::Enter => {
                    let _ = cmd.stdin_tx.send(b"\n".to_vec());
                    return;
                }
                KeyCode::Backspace => {
                    let _ = cmd.stdin_tx.send(b"\x7f".to_vec());
                    return;
                }
                KeyCode::Char(c) => {
                    let _ = cmd.stdin_tx.send(c.to_string().into_bytes());
                    return;
                }
                KeyCode::Tab => {
                    let _ = cmd.stdin_tx.send(b"\t".to_vec());
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
                    let line = self.editor.buffer.cursor_line;
                    let col = self.editor.buffer.cursor_col;
                    if let Some(next_param) =
                        crate::form::FormParser::find_next(&self.editor.buffer, line, col)
                    {
                        self.editor.buffer.cursor_line = next_param.line;
                        self.editor.buffer.cursor_col = next_param.start_col;
                        self.status_message = format!(
                            "Focused parameter '{}' ({})",
                            next_param.name, next_param.value
                        );
                    } else {
                        let pos = self.editor.buffer.char_index_at_cursor();
                        self.editor.buffer.insert_str("  ");
                        if let Some(client) = &self.collab_client {
                            client.send_insert_char(pos, ' ');
                            client.send_insert_char(pos + 1, ' ');
                            client.send_cursor_move(
                                self.editor.buffer.cursor_line,
                                self.editor.buffer.cursor_col,
                            );
                        }
                    }
                }
                return;
            }
            KeyCode::BackTab => {
                if self.focus == FocusPane::Editor {
                    let line = self.editor.buffer.cursor_line;
                    let col = self.editor.buffer.cursor_col;
                    if let Some(prev_param) =
                        crate::form::FormParser::find_prev(&self.editor.buffer, line, col)
                    {
                        self.editor.buffer.cursor_line = prev_param.line;
                        self.editor.buffer.cursor_col = prev_param.start_col;
                        self.status_message = format!(
                            "Focused parameter '{}' ({})",
                            prev_param.name, prev_param.value
                        );
                    }
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
        let old_line = self.editor.buffer.cursor_line;
        let old_col = self.editor.buffer.cursor_col;

        match key.code {
            KeyCode::Left | KeyCode::Char('[') => {
                let line = self.editor.buffer.cursor_line;
                if let Some(out_span) =
                    Runner::find_enclosing_output_block(&self.editor.buffer, line)
                {
                    let blocks = Runner::all_code_blocks(&self.editor.buffer);
                    if let Some(block) = blocks.iter().find(|b| {
                        b.end_fence_line + 1 == out_span.start_line
                            || b.end_fence_line + 2 == out_span.start_line
                    }) && let Some(fleet) = self.fleet_states.get_mut(&block.id)
                    {
                        if fleet.selected_tab > 0 {
                            fleet.selected_tab -= 1;
                        } else if !fleet.hosts.is_empty() {
                            fleet.selected_tab = fleet.hosts.len() - 1;
                        }
                        self.status_message = format!(
                            "Fleet '{}': Host {}/{} ({})",
                            block.name.as_deref().unwrap_or(&block.id),
                            fleet.selected_tab + 1,
                            fleet.hosts.len(),
                            fleet
                                .hosts
                                .get(fleet.selected_tab)
                                .map(|h| h.host.as_str())
                                .unwrap_or("")
                        );
                        if let Some(name) = &block.name {
                            let cloned = fleet.clone();
                            self.fleet_states.insert(name.clone(), cloned);
                        }
                        return;
                    }
                }
                if key.code == KeyCode::Left {
                    if key.modifiers.contains(KeyModifiers::ALT) {
                        self.editor.buffer.move_word_left();
                    } else {
                        self.editor.buffer.move_left();
                    }
                } else if key.code == KeyCode::Char('[') {
                    // Normal char insert if not inside fleet block
                    let pos = self.editor.buffer.char_index_at_cursor();
                    self.editor.buffer.insert_char('[');
                    if let Some(client) = &self.collab_client {
                        client.send_insert_char(pos, '[');
                    }
                }
            }
            KeyCode::Right | KeyCode::Char(']') => {
                let line = self.editor.buffer.cursor_line;
                if let Some(out_span) =
                    Runner::find_enclosing_output_block(&self.editor.buffer, line)
                {
                    let blocks = Runner::all_code_blocks(&self.editor.buffer);
                    if let Some(block) = blocks.iter().find(|b| {
                        b.end_fence_line + 1 == out_span.start_line
                            || b.end_fence_line + 2 == out_span.start_line
                    }) && let Some(fleet) = self.fleet_states.get_mut(&block.id)
                    {
                        if !fleet.hosts.is_empty() {
                            fleet.selected_tab = (fleet.selected_tab + 1) % fleet.hosts.len();
                        }
                        self.status_message = format!(
                            "Fleet '{}': Host {}/{} ({})",
                            block.name.as_deref().unwrap_or(&block.id),
                            fleet.selected_tab + 1,
                            fleet.hosts.len(),
                            fleet
                                .hosts
                                .get(fleet.selected_tab)
                                .map(|h| h.host.as_str())
                                .unwrap_or("")
                        );
                        if let Some(name) = &block.name {
                            let cloned = fleet.clone();
                            self.fleet_states.insert(name.clone(), cloned);
                        }
                        return;
                    }
                }
                if key.code == KeyCode::Right {
                    if key.modifiers.contains(KeyModifiers::ALT) {
                        self.editor.buffer.move_word_right();
                    } else {
                        self.editor.buffer.move_right();
                    }
                } else if key.code == KeyCode::Char(']') {
                    // Normal char insert if not inside fleet block
                    let pos = self.editor.buffer.char_index_at_cursor();
                    self.editor.buffer.insert_char(']');
                    if let Some(client) = &self.collab_client {
                        client.send_insert_char(pos, ']');
                    }
                }
            }
            KeyCode::Up => {
                let line = self.editor.buffer.cursor_line;
                if let Some(out_span) =
                    Runner::find_enclosing_output_block(&self.editor.buffer, line)
                {
                    let blocks = Runner::all_code_blocks(&self.editor.buffer);
                    if let Some(block) = blocks.iter().find(|b| {
                        b.end_fence_line + 1 == out_span.start_line
                            || b.end_fence_line + 2 == out_span.start_line
                    }) && let Some(tbl) = self.table_states.get_mut(&block.id)
                    {
                        tbl.select_prev_row();
                        let r_idx = tbl.selected_row;
                        let total = tbl.rows.len();
                        self.status_message = format!(
                            "Table '{}': Row {}/{} selected",
                            block.name.as_deref().unwrap_or(&block.id),
                            r_idx + 1,
                            total
                        );
                        if let Some(name) = &block.name {
                            let cloned = tbl.clone();
                            self.table_states.insert(name.clone(), cloned);
                        }
                        return;
                    }
                }
                self.editor.buffer.move_up();
            }
            KeyCode::Down => {
                let line = self.editor.buffer.cursor_line;
                if let Some(out_span) =
                    Runner::find_enclosing_output_block(&self.editor.buffer, line)
                {
                    let blocks = Runner::all_code_blocks(&self.editor.buffer);
                    if let Some(block) = blocks.iter().find(|b| {
                        b.end_fence_line + 1 == out_span.start_line
                            || b.end_fence_line + 2 == out_span.start_line
                    }) && let Some(tbl) = self.table_states.get_mut(&block.id)
                    {
                        tbl.select_next_row();
                        let r_idx = tbl.selected_row;
                        let total = tbl.rows.len();
                        self.status_message = format!(
                            "Table '{}': Row {}/{} selected",
                            block.name.as_deref().unwrap_or(&block.id),
                            r_idx + 1,
                            total
                        );
                        if let Some(name) = &block.name {
                            let cloned = tbl.clone();
                            self.table_states.insert(name.clone(), cloned);
                        }
                        return;
                    }
                }
                self.editor.buffer.move_down();
            }
            KeyCode::Home => self.editor.buffer.move_home(),
            KeyCode::End => self.editor.buffer.move_end(),
            KeyCode::PageUp => self.editor.buffer.page_up(20),
            KeyCode::PageDown => self.editor.buffer.page_down(20),
            KeyCode::Backspace => {
                if key.modifiers.contains(KeyModifiers::ALT) {
                    let col_before = self.editor.buffer.cursor_col;
                    let pos = self.editor.buffer.char_index_at_cursor();
                    self.editor.buffer.delete_word();
                    let deleted = col_before.saturating_sub(self.editor.buffer.cursor_col);
                    if let Some(client) = &self.collab_client {
                        for i in 0..deleted {
                            client.send_delete_char(pos.saturating_sub(1 + i));
                        }
                    }
                } else {
                    let pos = self.editor.buffer.char_index_at_cursor();
                    if pos > 0 {
                        self.editor.buffer.backspace();
                        if let Some(client) = &self.collab_client {
                            client.send_delete_char(pos - 1);
                        }
                    }
                }
            }
            KeyCode::Delete => {
                let pos = self.editor.buffer.char_index_at_cursor();
                if pos < self.editor.buffer.rope.len_chars() {
                    self.editor.buffer.delete();
                    if let Some(client) = &self.collab_client {
                        client.send_delete_char(pos);
                    }
                }
            }
            KeyCode::Enter => {
                let line = self.editor.buffer.cursor_line;
                let col = self.editor.buffer.cursor_col;

                // 1. Check if cursor is on a parameterized form widget
                if let Some(param) =
                    crate::form::FormParser::find_at_cursor(&self.editor.buffer, line, col)
                {
                    match param.param_type {
                        crate::form::FormParamType::Toggle => {
                            if let Some((name, new_val)) =
                                crate::form::FormParser::toggle_or_cycle_at_cursor(
                                    &mut self.editor.buffer,
                                )
                            {
                                self.set_param(&name, &new_val);
                                self.status_message = format!("Toggled {}: {}", name, new_val);
                                return;
                            }
                        }
                        crate::form::FormParamType::Select { .. } => {
                            if let Some((name, new_val)) =
                                crate::form::FormParser::toggle_or_cycle_at_cursor(
                                    &mut self.editor.buffer,
                                )
                            {
                                self.set_param(&name, &new_val);
                                self.status_message = format!("Selected {}: {}", name, new_val);
                                return;
                            }
                        }
                        crate::form::FormParamType::Input | crate::form::FormParamType::Secret => {
                            self.mode = AppMode::ParamPrompt {
                                param_name: param.name.clone(),
                                input_value: param.value.clone(),
                                param_line: param.line,
                                start_col: param.start_col,
                                end_col: param.end_col,
                                is_secret: matches!(
                                    param.param_type,
                                    crate::form::FormParamType::Secret
                                ),
                            };
                            return;
                        }
                    }
                }

                // 2. Check if cursor is inside a table output block -> open inspection modal
                if let Some(out_span) =
                    Runner::find_enclosing_output_block(&self.editor.buffer, line)
                {
                    let blocks = Runner::all_code_blocks(&self.editor.buffer);
                    if let Some(block) = blocks.iter().find(|b| {
                        b.end_fence_line + 1 == out_span.start_line
                            || b.end_fence_line + 2 == out_span.start_line
                    }) && let Some(tbl) = self.table_states.get(&block.id)
                    {
                        self.mode = AppMode::TableInspectModal {
                            block_id: block.id.clone(),
                            row_index: tbl.selected_row,
                        };
                        return;
                    }
                }

                let pos = self.editor.buffer.char_index_at_cursor();
                self.editor.buffer.insert_newline();
                if let Some(client) = &self.collab_client {
                    client.send_insert_char(pos, '\n');
                    let indent = self.editor.buffer.cursor_col;
                    for i in 0..indent {
                        client.send_insert_char(pos + 1 + i, ' ');
                    }
                }
            }
            KeyCode::Char(' ') => {
                if self.editor.buffer.is_cursor_on_task_item() {
                    let line = self.editor.buffer.cursor_line;
                    if let Some(new_checked) = self.editor.buffer.toggle_task_at_cursor() {
                        self.status_message = if new_checked {
                            "Task marked completed".to_string()
                        } else {
                            "Task marked incomplete".to_string()
                        };
                        if let Some(client) = &self.collab_client {
                            client.send_toggle_task(line, new_checked);
                        }
                        return;
                    }
                }

                // Check parameter toggling or cycling on Space
                let line = self.editor.buffer.cursor_line;
                let col = self.editor.buffer.cursor_col;
                if let Some(param) =
                    crate::form::FormParser::find_at_cursor(&self.editor.buffer, line, col)
                {
                    match param.param_type {
                        crate::form::FormParamType::Toggle => {
                            if let Some((name, new_val)) =
                                crate::form::FormParser::toggle_or_cycle_at_cursor(
                                    &mut self.editor.buffer,
                                )
                            {
                                self.set_param(&name, &new_val);
                                self.status_message = format!("Toggled {}: {}", name, new_val);
                                return;
                            }
                        }
                        crate::form::FormParamType::Select { .. } => {
                            if let Some((name, new_val)) =
                                crate::form::FormParser::toggle_or_cycle_at_cursor(
                                    &mut self.editor.buffer,
                                )
                            {
                                self.set_param(&name, &new_val);
                                self.status_message = format!("Selected {}: {}", name, new_val);
                                return;
                            }
                        }
                        crate::form::FormParamType::Input | crate::form::FormParamType::Secret => {
                            self.mode = AppMode::ParamPrompt {
                                param_name: param.name.clone(),
                                input_value: param.value.clone(),
                                param_line: param.line,
                                start_col: param.start_col,
                                end_col: param.end_col,
                                is_secret: matches!(
                                    param.param_type,
                                    crate::form::FormParamType::Secret
                                ),
                            };
                            return;
                        }
                    }
                }

                let pos = self.editor.buffer.char_index_at_cursor();
                self.editor.buffer.insert_char(' ');
                if let Some(client) = &self.collab_client {
                    client.send_insert_char(pos, ' ');
                }
            }
            KeyCode::Char(c) => {
                let pos = self.editor.buffer.char_index_at_cursor();
                self.editor.buffer.insert_char(c);
                if let Some(client) = &self.collab_client {
                    client.send_insert_char(pos, c);
                }
            }
            _ => {}
        }

        // Broadcast cursor movement if changed
        if (
            self.editor.buffer.cursor_line,
            self.editor.buffer.cursor_col,
        ) != (old_line, old_col)
            && let Some(client) = &self.collab_client
        {
            client.send_cursor_move(
                self.editor.buffer.cursor_line,
                self.editor.buffer.cursor_col,
            );
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
                self.palette
                    .backspace(&self.workspace, self.editor.file_path.as_ref());
            }
            KeyCode::Enter => {
                if let Some(item) = self.palette.selected_item().cloned() {
                    self.execute_palette_action(item.action);
                }
            }
            KeyCode::Char(c) => {
                self.palette
                    .insert_char(c, &self.workspace, self.editor.file_path.as_ref());
            }
            _ => {}
        }
    }

    pub fn execute_palette_action(&mut self, action: PaletteAction) {
        match action {
            PaletteAction::OpenDocument(path) => {
                self.open_document(&path);
                if !matches!(self.mode, AppMode::UnsavedChangesModal { .. }) {
                    self.mode = AppMode::Normal;
                }
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
                if !matches!(self.mode, AppMode::UnsavedChangesModal { .. }) {
                    self.mode = AppMode::Normal;
                }
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
            PaletteAction::RunAllBlocks => {
                self.mode = AppMode::Normal;
                self.trigger_run_all_blocks();
            }
            PaletteAction::RunUpToCursor => {
                self.mode = AppMode::Normal;
                self.trigger_run_up_to_cursor();
            }
            PaletteAction::ToggleTaskItem => {
                self.mode = AppMode::Normal;
                self.toggle_task_at_cursor();
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
                    self.force_open_document(&p);
                } else {
                    self.editor = Editor::new();
                }
                self.status_message = "Document deleted".to_string();
                self.mode = AppMode::Normal;
            }
            PaletteAction::ShowHelp => {
                self.mode = AppMode::Help;
            }
            PaletteAction::ViewAuditLog => {
                let verified = crate::audit::AuditRecorder::new(&self.workspace.root_dir)
                    .and_then(|r| r.verify_integrity())
                    .unwrap_or(true);
                self.mode = AppMode::AuditLogModal {
                    selected_index: 0,
                    search_query: String::new(),
                    integrity_verified: verified,
                };
            }
            PaletteAction::Quit => {
                if self.editor.is_dirty() {
                    let doc_name = self
                        .editor
                        .file_path
                        .as_ref()
                        .and_then(|p| p.file_name())
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_else(|| "current document".to_string());
                    self.mode = AppMode::UnsavedChangesModal {
                        pending_action: PendingAction::Quit,
                        doc_name,
                    };
                } else {
                    self.should_quit = true;
                }
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
                if !self.search_matches.is_empty()
                    && self.search_selected_index + 1 < self.search_matches.len()
                {
                    self.search_selected_index += 1;
                }
            }
            KeyCode::Enter => {
                if let Some(m) = self.search_matches.get(self.search_selected_index) {
                    let path = m.file_path.clone();
                    let line = m.line_number.saturating_sub(1);
                    self.open_document_with_line(&path, Some(line));
                    if !matches!(self.mode, AppMode::UnsavedChangesModal { .. }) {
                        self.mode = AppMode::Normal;
                    }
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

    pub fn toggle_task_at_cursor(&mut self) {
        let line = self.editor.buffer.cursor_line;
        self.toggle_task_at_line(line);
    }

    pub fn toggle_task_at_line(&mut self, line: usize) {
        if let Some(new_checked) = self.editor.buffer.toggle_task_at_line(line) {
            self.status_message = if new_checked {
                "Task marked completed".to_string()
            } else {
                "Task marked incomplete".to_string()
            };
            if let Some(client) = &self.collab_client {
                client.send_toggle_task(line, new_checked);
            }
        } else {
            self.status_message = "No task checkbox on this line".to_string();
        }
    }

    pub fn trigger_run_block(&mut self) {
        if self.is_running_block {
            self.status_message = "A command or playbook is already running...".to_string();
            return;
        }

        let cursor_line = self.editor.buffer.cursor_line;
        if let Some(block) = Runner::find_code_block(&self.editor.buffer, cursor_line) {
            // 1. Multiplayer consensus approval gate
            if block.approvals > 1
                && let Some(client) = &self.collab_client
            {
                client.send_approval_request(block.id.clone(), block.code.clone(), block.approvals);
                self.status_message = format!(
                    "Multiplayer approval consensus requested for block '{}' (requires {} approvals)",
                    block.id, block.approvals
                );
                return;
            }

            // 2. Interactive safety confirmation gate
            if block.requires_confirmation() {
                let prompt = block
                    .effective_confirm_prompt()
                    .unwrap_or_else(|| "Confirm execution?".to_string());
                let sec = block.analyze_security();
                let keyword_required =
                    if block.blast_radius.as_deref() == Some("critical") || block.danger {
                        if let Some(t) = &block.target {
                            if let Some(group) = t.strip_prefix('@') {
                                Some(group.to_string())
                            } else {
                                Some("CONFIRM".to_string())
                            }
                        } else {
                            Some("CONFIRM".to_string())
                        }
                    } else {
                        None
                    };

                let target_hosts = if let Some(t) = &block.target {
                    vec![t.clone()]
                } else {
                    vec!["localhost".to_string()]
                };

                self.mode = AppMode::ConfirmExecution {
                    block_id: block.id.clone(),
                    prompt,
                    command: block.code.chars().take(200).collect(),
                    target_hosts,
                    danger_level: sec.danger_level,
                    keyword_required,
                    keyword_input: String::new(),
                };
                return;
            }

            self.execute_block(block);
        } else {
            self.status_message = "No code block found under cursor".to_string();
        }
    }

    pub fn execute_block_by_id(&mut self, target_id: &str) {
        if let Some(block) = Runner::find_code_block_by_id(&self.editor.buffer, target_id) {
            self.execute_block(block);
        }
    }

    pub fn execute_block(&mut self, block: CodeBlock) {
        self.is_running_block = true;
        let display_name = block.name.as_deref().unwrap_or(&block.language);
        self.status_message = format!("Executing {} block...", display_name);

        if let Some(client) = &self.collab_client {
            client.send_stream_start(block.id.clone());
        }

        // Immediately mark output block as running
        Runner::apply_streaming_status(
            &mut self.editor.buffer,
            &block.id,
            crate::runner::OutputStatus::Running,
            "",
        );

        let initial_outputs = Runner::collect_existing_outputs(&self.editor.buffer);
        self.refresh_params_and_tables();
        let interpolated_code = Runner::interpolate_variables_with_context(
            &block.code,
            &initial_outputs,
            &self.params,
            &self.table_states,
        );
        let mut block_to_run = block.clone();
        block_to_run.code = interpolated_code;
        if let Some(t) = &block.target {
            block_to_run.target = Some(Runner::interpolate_variables_with_context(
                t,
                &initial_outputs,
                &self.params,
                &self.table_states,
            ));
        }
        if let Some(p) = &block.proxy {
            block_to_run.proxy = Some(Runner::interpolate_variables_with_context(
                p,
                &initial_outputs,
                &self.params,
                &self.table_states,
            ));
        }
        let env_vars = Runner::build_environment_variables_with_context(
            &initial_outputs,
            &self.params,
            &self.table_states,
        );

        let root_dir = self.workspace.root_dir.clone();
        let tx = self.event_tx.clone();
        let block_id = block.id.clone();
        let block_clone = block.clone();
        let pool_c = self.ssh_pool.clone();
        let buf_c = self.editor.buffer.clone();

        let (stdin_pipe_tx, mut stdin_pipe_rx) = tokio::sync::mpsc::unbounded_channel::<Vec<u8>>();
        let (cancel_pipe_tx, cancel_pipe_rx) = tokio::sync::oneshot::channel::<()>();

        self.running_command = Some(RunningCommand {
            block_id: block_id.clone(),
            stdin_tx: stdin_pipe_tx,
            cancel_tx: Some(cancel_pipe_tx),
            accumulated_output: String::new(),
        });

        let tx_chunk = tx.clone();
        let id_chunk = block_id.clone();
        let on_chunk = move |chunk: String| {
            let _ = tx_chunk.send(AppEvent::StreamChunk {
                block_id: id_chunk.clone(),
                chunk,
            });
        };

        let tx_end = tx.clone();
        let on_end = move |result: ExecutionResult| {
            let _ = tx_end.send(AppEvent::StreamFinished {
                block: block_clone,
                result,
            });
        };

        tokio::spawn(async move {
            match Runner::execute_streaming_with_pool(
                &block_to_run,
                &root_dir,
                &env_vars,
                &pool_c,
                Some(&buf_c),
                on_chunk,
                on_end,
            )
            .await
            {
                Ok(handle) => {
                    tokio::spawn(async move {
                        while let Some(bytes) = stdin_pipe_rx.recv().await {
                            if handle.stdin_tx.send(bytes).is_err() {
                                break;
                            }
                        }
                    });
                    if let Some(actual_cancel) = handle.cancel_tx {
                        tokio::spawn(async move {
                            if cancel_pipe_rx.await.is_ok() {
                                let _ = actual_cancel.send(());
                            }
                        });
                    }
                }
                Err(e) => {
                    let _ = tx.send(AppEvent::StatusMessage(format!("Execution failed: {}", e)));
                    let _ = tx.send(AppEvent::StreamFinished {
                        block,
                        result: ExecutionResult {
                            stdout: String::new(),
                            stderr: format!("Spawn error: {}", e),
                            exit_code: -1,
                            timed_out: false,
                        },
                    });
                }
            }
        });
    }

    pub fn trigger_run_all_blocks(&mut self) {
        if self.is_running_block {
            self.status_message = "A command or playbook is already running...".to_string();
            return;
        }

        let blocks = Runner::all_code_blocks(&self.editor.buffer);
        if blocks.is_empty() {
            self.status_message = "No code blocks found in document".to_string();
            return;
        }

        self.run_playbook_sequence(blocks);
    }

    pub fn trigger_run_up_to_cursor(&mut self) {
        if self.is_running_block {
            self.status_message = "A command or playbook is already running...".to_string();
            return;
        }

        let cursor_line = self.editor.buffer.cursor_line;
        let blocks: Vec<CodeBlock> = Runner::all_code_blocks(&self.editor.buffer)
            .into_iter()
            .filter(|b| b.start_fence_line <= cursor_line)
            .collect();

        if blocks.is_empty() {
            self.status_message = "No code blocks found up to cursor position".to_string();
            return;
        }

        self.run_playbook_sequence(blocks);
    }

    pub fn run_playbook_sequence(&mut self, blocks: Vec<CodeBlock>) {
        self.is_running_block = true;
        let total = blocks.len();
        self.status_message = format!("Starting playbook ({} steps)...", total);

        let initial_outputs = Runner::collect_existing_outputs(&self.editor.buffer);
        self.refresh_params_and_tables();
        let params_clone = self.params.clone();
        let tables_clone = self.table_states.clone();
        let root_dir = self.workspace.root_dir.clone();
        let tx = self.event_tx.clone();
        let pool_clone = self.ssh_pool.clone();
        let buf_clone = self.editor.buffer.clone();

        tokio::spawn(async move {
            let mut outputs = initial_outputs;
            let mut tables = tables_clone;
            let params = params_clone;

            for (i, block) in blocks.into_iter().enumerate() {
                let _ = tx.send(AppEvent::PlaybookStepStarted {
                    block_id: block.id.clone(),
                    block_name: block.name.clone(),
                    index: i,
                    total,
                });

                let interpolated_code = Runner::interpolate_variables_with_context(
                    &block.code,
                    &outputs,
                    &params,
                    &tables,
                );
                let mut block_to_run = block.clone();
                block_to_run.code = interpolated_code;
                if let Some(t) = &block.target {
                    block_to_run.target = Some(Runner::interpolate_variables_with_context(
                        t, &outputs, &params, &tables,
                    ));
                }
                if let Some(p) = &block.proxy {
                    block_to_run.proxy = Some(Runner::interpolate_variables_with_context(
                        p, &outputs, &params, &tables,
                    ));
                }

                let env_vars =
                    Runner::build_environment_variables_with_context(&outputs, &params, &tables);

                let tx_chunk = tx.clone();
                let b_id = block.id.clone();
                let (res_tx, res_rx) = tokio::sync::oneshot::channel();

                let handle_res = Runner::execute_streaming_with_pool(
                    &block_to_run,
                    &root_dir,
                    &env_vars,
                    &pool_clone,
                    Some(&buf_clone),
                    move |chunk| {
                        let _ = tx_chunk.send(AppEvent::PlaybookStepChunk {
                            block_id: b_id.clone(),
                            chunk,
                        });
                    },
                    move |result| {
                        let _ = res_tx.send(result);
                    },
                )
                .await;

                match handle_res {
                    Ok(_handle) => {
                        match res_rx.await {
                            Ok(result) => {
                                outputs.insert(format!("block_{}", i), result.clone());
                                outputs.insert(block.id.clone(), result.clone());
                                if let Some(name) = &block.name {
                                    outputs.insert(name.clone(), result.clone());
                                }
                                if let Some(target) = &block_to_run.target
                                    && let Some(group) = target.strip_prefix('@')
                                {
                                    outputs.insert(group.to_string(), result.clone());
                                }

                                if let Some(tbl) = crate::table::TableData::parse(&result.stdout) {
                                    tables.insert(format!("block_{}", i), tbl.clone());
                                    tables.insert(block.id.clone(), tbl.clone());
                                    if let Some(name) = &block.name {
                                        tables.insert(name.clone(), tbl);
                                    }
                                }

                                let exit_code = result.exit_code;
                                let _ = tx.send(AppEvent::PlaybookStepFinished {
                                    block: block.clone(),
                                    result,
                                    index: i,
                                    total,
                                });

                                // Fail-fast safeguard: halt sequence if exit_code != 0
                                if exit_code != 0 {
                                    let _ = tx.send(AppEvent::PlaybookHalted {
                                        block_id: block.id.clone(),
                                        block_name: block.name.clone(),
                                        index: i,
                                        total,
                                        exit_code,
                                    });
                                    return;
                                }
                            }
                            Err(_) => {
                                let _ = tx.send(AppEvent::PlaybookHalted {
                                    block_id: block.id.clone(),
                                    block_name: block.name.clone(),
                                    index: i,
                                    total,
                                    exit_code: -1,
                                });
                                return;
                            }
                        }
                    }
                    Err(e) => {
                        let _ = tx.send(AppEvent::StatusMessage(format!(
                            "Step {} failed to spawn: {}",
                            i + 1,
                            e
                        )));
                        let _ = tx.send(AppEvent::PlaybookHalted {
                            block_id: block.id.clone(),
                            block_name: block.name.clone(),
                            index: i,
                            total,
                            exit_code: -1,
                        });
                        return;
                    }
                }
            }

            let _ = tx.send(AppEvent::PlaybookCompleted { total });
        });
    }
}
