use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PeerInfo {
    pub id: String,
    pub name: String,
    pub color_index: usize,
    pub cursor_line: usize,
    pub cursor_col: usize,
}

impl PeerInfo {
    pub fn new(id: String, name: String, color_index: usize) -> Self {
        Self {
            id,
            name,
            color_index,
            cursor_line: 0,
            cursor_col: 0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CollabMessage {
    /// Initial request from a joining client to the server
    JoinRequest { name: String },

    /// Full state sync sent from server to joining client (and on reconnect/file switch)
    DocSync {
        content: String,
        doc_name: String,
        version: u64,
        your_id: String,
        peers: Vec<PeerInfo>,
    },

    /// Broadcast when a new peer joins the room
    PeerJoined { peer: PeerInfo },

    /// Broadcast when a peer disconnects
    PeerLeft { peer_id: String },

    /// Single character inserted at character offset `pos` in the document
    InsertChar {
        peer_id: String,
        pos: usize,
        char: char,
    },

    /// Single character deleted at character offset `pos` in the document
    DeleteChar { peer_id: String, pos: usize },

    /// Cursor movement event to line and column
    CursorMove {
        peer_id: String,
        line: usize,
        col: usize,
    },

    /// Trigger command block execution
    RunBlock { peer_id: String, block_id: String },

    /// Synchronized live output update for a code block
    OutputUpdate {
        peer_id: String,
        block_id: String,
        text: String,
        exit_code: i32,
    },

    /// Synchronized task checklist item toggle
    ToggleTask {
        peer_id: String,
        line: usize,
        checked: bool,
    },

    /// Real-time live output streaming: stream started
    StreamStart { peer_id: String, block_id: String },

    /// Real-time live output streaming: incremental chunk
    StreamChunk {
        peer_id: String,
        block_id: String,
        chunk: String,
    },

    /// Real-time live output streaming: stream finished with exit code
    StreamEnd {
        peer_id: String,
        block_id: String,
        exit_code: i32,
    },

    /// Multiplayer form parameter synchronization
    ParamUpdate {
        peer_id: String,
        param_name: String,
        value: String,
    },

    /// Request approval from peers to execute a protected/dangerous block
    ApprovalRequest {
        peer_id: String,
        block_id: String,
        command_preview: String,
        required_approvals: usize,
    },

    /// Peer casted an approval vote
    ApprovalVote {
        peer_id: String,
        block_id: String,
        approver_name: String,
    },

    /// Broadcast of current approval status for a block
    ApprovalStatus {
        block_id: String,
        approvals_count: usize,
        required_approvals: usize,
        approved: bool,
    },
}

pub fn peer_color(index: usize) -> ratatui::style::Color {
    const PALETTE: &[ratatui::style::Color] = &[
        ratatui::style::Color::Cyan,
        ratatui::style::Color::Magenta,
        ratatui::style::Color::Yellow,
        ratatui::style::Color::Green,
        ratatui::style::Color::LightBlue,
        ratatui::style::Color::LightRed,
        ratatui::style::Color::LightCyan,
        ratatui::style::Color::LightMagenta,
    ];
    PALETTE[index % PALETTE.len()]
}
