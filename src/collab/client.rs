use anyhow::{Context, Result};
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tokio::time::timeout;

use crate::collab::protocol::{CollabMessage, PeerInfo};

pub struct CollabClient {
    pub my_id: String,
    pub my_name: String,
    pub doc_name: String,
    pub initial_content: String,
    pub initial_peers: Vec<PeerInfo>,
    outgoing_tx: mpsc::UnboundedSender<CollabMessage>,
    pub incoming_rx: mpsc::UnboundedReceiver<CollabMessage>,
}

impl CollabClient {
    pub async fn connect(addr: &str, name: String) -> Result<Self> {
        let connect_fut = TcpStream::connect(addr);
        let stream = timeout(Duration::from_secs(5), connect_fut)
            .await
            .context("Connection to host timed out")??;

        let (read_half, mut write_half) = stream.into_split();
        let mut reader = BufReader::new(read_half);

        // 1. Send JoinRequest
        let join_req = CollabMessage::JoinRequest { name: name.clone() };
        let join_line = serde_json::to_string(&join_req)? + "\n";
        write_half.write_all(join_line.as_bytes()).await?;
        write_half.flush().await?;

        // 2. Await DocSync
        let mut sync_line = String::new();
        let read_sync_fut = reader.read_line(&mut sync_line);
        let n = timeout(Duration::from_secs(5), read_sync_fut)
            .await
            .context("Timed out waiting for DocSync from host")??;

        if n == 0 {
            return Err(anyhow::anyhow!("Host closed connection before sending DocSync"));
        }

        let sync_msg: CollabMessage = serde_json::from_str(sync_line.trim())
            .context("Failed to parse DocSync message from host")?;

        let (my_id, doc_name, initial_content, initial_peers) = match sync_msg {
            CollabMessage::DocSync {
                content,
                doc_name,
                version: _,
                your_id,
                peers,
            } => (your_id, doc_name, content, peers),
            _ => return Err(anyhow::anyhow!("Expected DocSync message, got unexpected message")),
        };

        // 3. Channels for communication with App event loop
        let (outgoing_tx, mut outgoing_rx) = mpsc::unbounded_channel::<CollabMessage>();
        let (incoming_tx, incoming_rx) = mpsc::unbounded_channel::<CollabMessage>();

        // 4. Spawn writer task
        tokio::spawn(async move {
            while let Some(msg) = outgoing_rx.recv().await {
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

        // 5. Spawn reader task
        tokio::spawn(async move {
            let mut line = String::new();
            loop {
                line.clear();
                match reader.read_line(&mut line).await {
                    Ok(0) => {
                        // Connection closed
                        break;
                    }
                    Ok(_) => {
                        let trimmed = line.trim();
                        if !trimmed.is_empty() {
                            if let Ok(msg) = serde_json::from_str::<CollabMessage>(trimmed) {
                                if incoming_tx.send(msg).is_err() {
                                    break;
                                }
                            }
                        }
                    }
                    Err(_) => {
                        break;
                    }
                }
            }
        });

        Ok(Self {
            my_id,
            my_name: name,
            doc_name,
            initial_content,
            initial_peers,
            outgoing_tx,
            incoming_rx,
        })
    }

    pub fn send(&self, msg: CollabMessage) {
        let _ = self.outgoing_tx.send(msg);
    }

    pub fn try_recv(&mut self) -> Option<CollabMessage> {
        self.incoming_rx.try_recv().ok()
    }

    pub fn send_insert_char(&self, pos: usize, c: char) {
        self.send(CollabMessage::InsertChar {
            peer_id: self.my_id.clone(),
            pos,
            char: c,
        });
    }

    pub fn send_delete_char(&self, pos: usize) {
        self.send(CollabMessage::DeleteChar {
            peer_id: self.my_id.clone(),
            pos,
        });
    }

    pub fn send_cursor_move(&self, line: usize, col: usize) {
        self.send(CollabMessage::CursorMove {
            peer_id: self.my_id.clone(),
            line,
            col,
        });
    }

    pub fn send_run_block(&self, block_id: String) {
        self.send(CollabMessage::RunBlock {
            peer_id: self.my_id.clone(),
            block_id,
        });
    }

    pub fn send_output_update(&self, block_id: String, text: String, exit_code: i32) {
        self.send(CollabMessage::OutputUpdate {
            peer_id: self.my_id.clone(),
            block_id,
            text,
            exit_code,
        });
    }

    pub fn send_toggle_task(&self, line: usize, checked: bool) {
        self.send(CollabMessage::ToggleTask {
            peer_id: self.my_id.clone(),
            line,
            checked,
        });
    }

    pub fn send_stream_start(&self, block_id: String) {
        self.send(CollabMessage::StreamStart {
            peer_id: self.my_id.clone(),
            block_id,
        });
    }

    pub fn send_stream_chunk(&self, block_id: String, chunk: String) {
        self.send(CollabMessage::StreamChunk {
            peer_id: self.my_id.clone(),
            block_id,
            chunk,
        });
    }

    pub fn send_stream_end(&self, block_id: String, exit_code: i32) {
        self.send(CollabMessage::StreamEnd {
            peer_id: self.my_id.clone(),
            block_id,
            exit_code,
        });
    }

    pub fn send_param_update(&self, param_name: String, value: String) {
        self.send(CollabMessage::ParamUpdate {
            peer_id: self.my_id.clone(),
            param_name,
            value,
        });
    }

    pub fn send_approval_request(&self, block_id: String, command_preview: String, required_approvals: usize) {
        self.send(CollabMessage::ApprovalRequest {
            peer_id: self.my_id.clone(),
            block_id,
            command_preview,
            required_approvals,
        });
    }

    pub fn send_approval_vote(&self, block_id: String) {
        self.send(CollabMessage::ApprovalVote {
            peer_id: self.my_id.clone(),
            block_id,
            approver_name: self.my_name.clone(),
        });
    }
}
