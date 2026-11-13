use anyhow::{Context, Result};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
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
    last_cursor: AtomicU64,
    connected: Arc<AtomicBool>,
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
        let mut join_bytes = serde_json::to_vec(&join_req)?;
        join_bytes.push(b'\n');
        write_half.write_all(&join_bytes).await?;
        write_half.flush().await?;

        // 2. Await DocSync
        let mut sync_line = String::new();
        let read_sync_fut = reader.read_line(&mut sync_line);
        let n = timeout(Duration::from_secs(5), read_sync_fut)
            .await
            .context("Timed out waiting for DocSync from host")??;

        if n == 0 {
            return Err(anyhow::anyhow!(
                "Host closed connection before sending DocSync"
            ));
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
            _ => {
                return Err(anyhow::anyhow!(
                    "Expected DocSync message, got unexpected message"
                ));
            }
        };

        // 3. Channels for communication with App event loop
        let (outgoing_tx, mut outgoing_rx) = mpsc::unbounded_channel::<CollabMessage>();
        let (incoming_tx, incoming_rx) = mpsc::unbounded_channel::<CollabMessage>();
        let connected = Arc::new(AtomicBool::new(true));

        // 4. Spawn writer task
        let connected_w = Arc::clone(&connected);
        tokio::spawn(async move {
            while let Some(msg) = outgoing_rx.recv().await {
                if let Ok(mut bytes) = serde_json::to_vec(&msg) {
                    bytes.push(b'\n');
                    if write_half.write_all(&bytes).await.is_err()
                        || write_half.flush().await.is_err()
                    {
                        connected_w.store(false, Ordering::SeqCst);
                        break;
                    }
                }
            }
        });

        // 5. Spawn reader task
        let connected_r = Arc::clone(&connected);
        tokio::spawn(async move {
            let mut line = String::new();
            loop {
                line.clear();
                match reader.read_line(&mut line).await {
                    Ok(0) => {
                        // Connection closed cleanly by remote host
                        connected_r.store(false, Ordering::SeqCst);
                        break;
                    }
                    Ok(_) => {
                        let trimmed = line.trim();
                        if !trimmed.is_empty()
                            && let Ok(msg) = serde_json::from_str::<CollabMessage>(trimmed)
                            && incoming_tx.send(msg).is_err()
                        {
                            connected_r.store(false, Ordering::SeqCst);
                            break;
                        }
                    }
                    Err(_) => {
                        connected_r.store(false, Ordering::SeqCst);
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
            last_cursor: AtomicU64::new(u64::MAX),
            connected,
        })
    }

    pub fn is_connected(&self) -> bool {
        self.connected.load(Ordering::SeqCst)
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
        let packed = ((line as u64) << 32) | (col as u32 as u64);
        if self.last_cursor.swap(packed, Ordering::Relaxed) == packed {
            return;
        }
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

    pub fn send_approval_request(
        &self,
        block_id: String,
        command_preview: String,
        required_approvals: usize,
    ) {
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
