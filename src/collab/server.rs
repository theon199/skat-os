use anyhow::Result;
use ropey::Rope;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, Mutex};
use tokio::task::JoinHandle;

use crate::collab::protocol::{CollabMessage, PeerInfo};
use crate::guard::ApprovalConsensus;

pub struct ServerState {
    pub content: Rope,
    pub doc_name: String,
    pub version: u64,
    pub peers: HashMap<String, PeerInfo>,
    pub peer_senders: HashMap<String, mpsc::UnboundedSender<CollabMessage>>,
    pub next_color_idx: usize,
    pub peer_counter: usize,
    pub pending_approvals: HashMap<String, ApprovalConsensus>,
}

impl ServerState {
    pub fn new(initial_content: String, doc_name: String) -> Self {
        Self {
            content: Rope::from_str(&initial_content),
            doc_name,
            version: 0,
            peers: HashMap::new(),
            peer_senders: HashMap::new(),
            next_color_idx: 0,
            peer_counter: 0,
            pending_approvals: HashMap::new(),
        }
    }

    pub fn broadcast(&self, sender_id: &str, msg: &CollabMessage) {
        for (peer_id, tx) in &self.peer_senders {
            if peer_id != sender_id {
                let _ = tx.send(msg.clone());
            }
        }
    }

    pub fn broadcast_to_all(&self, msg: &CollabMessage) {
        for tx in self.peer_senders.values() {
            let _ = tx.send(msg.clone());
        }
    }
}

pub struct CollabServerHandle {
    pub local_addr: SocketAddr,
    pub state: Arc<Mutex<ServerState>>,
    server_task: JoinHandle<()>,
}

impl CollabServerHandle {
    pub fn stop(&self) {
        self.server_task.abort();
    }
}

pub struct CollabServer;

impl CollabServer {
    pub async fn bind(addr: &str, initial_content: String, doc_name: String) -> Result<CollabServerHandle> {
        let listener = TcpListener::bind(addr).await?;
        let local_addr = listener.local_addr()?;
        let state = Arc::new(Mutex::new(ServerState::new(initial_content, doc_name)));

        let state_clone = Arc::clone(&state);
        let server_task = tokio::spawn(async move {
            loop {
                match listener.accept().await {
                    Ok((socket, remote_addr)) => {
                        let state = Arc::clone(&state_clone);
                        tokio::spawn(async move {
                            if let Err(e) = Self::handle_connection(socket, remote_addr, state).await {
                                eprintln!("[CollabServer] Error handling {}: {}", remote_addr, e);
                            }
                        });
                    }
                    Err(_) => {
                        break;
                    }
                }
            }
        });

        Ok(CollabServerHandle {
            local_addr,
            state,
            server_task,
        })
    }

    async fn handle_connection(
        socket: TcpStream,
        _remote_addr: SocketAddr,
        state: Arc<Mutex<ServerState>>,
    ) -> Result<()> {
        let (read_half, mut write_half) = socket.into_split();
        let mut reader = BufReader::new(read_half);

        // 1. Await JoinRequest
        let mut first_line = String::new();
        let bytes_read = reader.read_line(&mut first_line).await?;
        if bytes_read == 0 {
            return Ok(());
        }

        let join_msg: CollabMessage = serde_json::from_str(first_line.trim())?;
        let client_name = match join_msg {
            CollabMessage::JoinRequest { name } => name,
            _ => return Err(anyhow::anyhow!("Expected JoinRequest as first message")),
        };

        // 2. Register peer
        let (peer_id, _color_idx, doc_sync_msg, _peer_joined_msg, mut peer_rx) = {
            let mut s = state.lock().await;
            s.peer_counter += 1;
            let id = format!("peer_{}_{}", client_name.to_lowercase().replace(' ', "_"), s.peer_counter);
            let color = s.next_color_idx;
            s.next_color_idx += 1;

            let peer_info = PeerInfo::new(id.clone(), client_name.clone(), color);
            s.peers.insert(id.clone(), peer_info.clone());

            let (tx, rx) = mpsc::unbounded_channel();
            s.peer_senders.insert(id.clone(), tx);

            let sync = CollabMessage::DocSync {
                content: s.content.to_string(),
                doc_name: s.doc_name.clone(),
                version: s.version,
                your_id: id.clone(),
                peers: s.peers.values().cloned().collect(),
            };

            let joined = CollabMessage::PeerJoined { peer: peer_info };
            s.broadcast(&id, &joined);

            (id, color, sync, joined, rx)
        };

        // 3. Send DocSync to the new client
        let sync_line = serde_json::to_string(&doc_sync_msg)? + "\n";
        write_half.write_all(sync_line.as_bytes()).await?;
        write_half.flush().await?;

        // 4. Spawn writer task for this client
        let writer_task = tokio::spawn(async move {
            while let Some(msg) = peer_rx.recv().await {
                if let Ok(line) = serde_json::to_string(&msg) {
                    if write_half.write_all((line + "\n").as_bytes()).await.is_err() {
                        break;
                    }
                    if write_half.flush().await.is_err() {
                        break;
                    }
                }
            }
        });

        // 5. Read incoming operations loop
        let mut line_buf = String::new();
        loop {
            line_buf.clear();
            let n = reader.read_line(&mut line_buf).await?;
            if n == 0 {
                // Client disconnected
                break;
            }

            let trimmed = line_buf.trim();
            if trimmed.is_empty() {
                continue;
            }

            if let Ok(msg) = serde_json::from_str::<CollabMessage>(trimmed) {
                let mut s = state.lock().await;
                match &msg {
                    CollabMessage::InsertChar { peer_id: _, pos, char } => {
                        let rope_len = s.content.len_chars();
                        let idx = (*pos).min(rope_len);
                        s.content.insert_char(idx, *char);
                        s.version += 1;
                        s.broadcast(&peer_id, &msg);
                    }
                    CollabMessage::DeleteChar { peer_id: _, pos } => {
                        let rope_len = s.content.len_chars();
                        if *pos < rope_len {
                            s.content.remove(*pos..(*pos + 1));
                            s.version += 1;
                            s.broadcast(&peer_id, &msg);
                        }
                    }
                    CollabMessage::CursorMove { peer_id: _, line, col } => {
                        if let Some(peer) = s.peers.get_mut(&peer_id) {
                            peer.cursor_line = *line;
                            peer.cursor_col = *col;
                        }
                        s.broadcast(&peer_id, &msg);
                    }
                    CollabMessage::RunBlock { .. } => {
                        s.broadcast(&peer_id, &msg);
                    }
                    CollabMessage::OutputUpdate { peer_id: _, block_id: _, text: _, exit_code: _ } => {
                        s.broadcast(&peer_id, &msg);
                    }
                    CollabMessage::ToggleTask { peer_id: _, line, checked } => {
                        if *line < s.content.len_lines() {
                            let line_str = s.content.line(*line).to_string();
                            if let Some(info) = crate::editor::buffer::TextBuffer::parse_task_item(&line_str) {
                                let line_start = s.content.line_to_char(*line);
                                let char_pos = line_start + info.check_char_col;
                                if char_pos < s.content.len_chars() {
                                    s.content.remove(char_pos..char_pos + 1);
                                    s.content.insert_char(char_pos, if *checked { 'x' } else { ' ' });
                                    s.version += 1;
                                }
                            }
                        }
                        s.broadcast(&peer_id, &msg);
                    }
                    CollabMessage::StreamStart { .. } => {
                        s.broadcast(&peer_id, &msg);
                    }
                    CollabMessage::StreamChunk { .. } => {
                        s.broadcast(&peer_id, &msg);
                    }
                    CollabMessage::StreamEnd { .. } => {
                        s.broadcast(&peer_id, &msg);
                    }
                    CollabMessage::ParamUpdate { peer_id: _, param_name, value } => {
                        crate::form::FormParser::update_param_in_rope(&mut s.content, param_name, value);
                        s.version += 1;
                        s.broadcast(&peer_id, &msg);
                    }
                    CollabMessage::ApprovalRequest { peer_id: _, block_id, command_preview, required_approvals } => {
                        let mut consensus = ApprovalConsensus::new(
                            block_id.clone(),
                            command_preview.clone(),
                            *required_approvals,
                        );
                        consensus.add_approval(client_name.clone());
                        let count = consensus.current_count();
                        let req = consensus.required_approvals;
                        let approved = consensus.is_approved();
                        s.pending_approvals.insert(block_id.clone(), consensus);
                        s.broadcast(&peer_id, &msg);
                        let status_msg = CollabMessage::ApprovalStatus {
                            block_id: block_id.clone(),
                            approvals_count: count,
                            required_approvals: req,
                            approved,
                        };
                        s.broadcast_to_all(&status_msg);
                    }
                    CollabMessage::ApprovalVote { peer_id: _, block_id, approver_name } => {
                        let mut status_msg = None;
                        if let Some(consensus) = s.pending_approvals.get_mut(block_id) {
                            let approved = consensus.add_approval(approver_name.clone());
                            status_msg = Some(CollabMessage::ApprovalStatus {
                                block_id: block_id.clone(),
                                approvals_count: consensus.current_count(),
                                required_approvals: consensus.required_approvals,
                                approved,
                            });
                        }
                        s.broadcast(&peer_id, &msg);
                        if let Some(status) = status_msg {
                            s.broadcast_to_all(&status);
                        }
                    }
                    _ => {}
                }
            }
        }

        // 6. Cleanup on disconnect
        writer_task.abort();
        {
            let mut s = state.lock().await;
            s.peers.remove(&peer_id);
            s.peer_senders.remove(&peer_id);
            let left_msg = CollabMessage::PeerLeft { peer_id: peer_id.clone() };
            s.broadcast_to_all(&left_msg);
        }

        Ok(())
    }
}
