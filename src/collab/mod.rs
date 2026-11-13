pub mod client;
pub mod protocol;
pub mod server;

pub use client::CollabClient;
pub use protocol::{CollabMessage, PeerInfo, peer_color};
pub use server::{CollabServer, CollabServerHandle};

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tokio::time::sleep;

    #[tokio::test]
    async fn test_collab_handshake_and_doc_sync() {
        let server = CollabServer::bind(
            "127.0.0.1:0",
            "# Multiplayer Playbook\n".to_string(),
            "runbook.md".to_string(),
        )
        .await
        .unwrap();

        let addr = server.local_addr.to_string();
        let client = CollabClient::connect(&addr, "Alice".to_string())
            .await
            .unwrap();

        assert_eq!(client.doc_name, "runbook.md");
        assert_eq!(client.initial_content, "# Multiplayer Playbook\n");
        assert_eq!(client.my_name, "Alice");
        assert!(client.is_connected());

        server.stop();
    }

    #[tokio::test]
    async fn test_collab_peer_join_and_leave() {
        let server = CollabServer::bind(
            "127.0.0.1:0",
            "Initial doc content\n".to_string(),
            "demo.md".to_string(),
        )
        .await
        .unwrap();

        let addr = server.local_addr.to_string();
        let mut alice = CollabClient::connect(&addr, "Alice".to_string())
            .await
            .unwrap();
        let bob = CollabClient::connect(&addr, "Bob".to_string())
            .await
            .unwrap();

        // Alice should receive Bob's PeerJoined
        let mut bob_joined = false;
        for _ in 0..20 {
            if let Some(CollabMessage::PeerJoined { peer }) = alice.try_recv()
                && peer.name == "Bob"
            {
                bob_joined = true;
                break;
            }
            sleep(Duration::from_millis(20)).await;
        }
        assert!(
            bob_joined,
            "Alice should have received Bob's PeerJoined message"
        );

        // Disconnect Bob
        let bob_id = bob.my_id.clone();
        drop(bob);

        // Alice should receive PeerLeft for Bob
        let mut bob_left = false;
        for _ in 0..20 {
            if let Some(CollabMessage::PeerLeft { peer_id }) = alice.try_recv()
                && peer_id == bob_id
            {
                bob_left = true;
                break;
            }
            sleep(Duration::from_millis(20)).await;
        }
        assert!(
            bob_left,
            "Alice should have received Bob's PeerLeft message"
        );

        // Verify Bob was cleaned up on the server
        {
            let s = server.state.lock().await;
            assert!(!s.peers.contains_key(&bob_id));
            assert!(!s.peer_senders.contains_key(&bob_id));
        }

        server.stop();
    }

    #[tokio::test]
    async fn test_collab_char_edit_and_task_toggle() {
        let server = CollabServer::bind(
            "127.0.0.1:0",
            "# Runbook\n- [ ] Task item\n".to_string(),
            "runbook.md".to_string(),
        )
        .await
        .unwrap();

        let addr = server.local_addr.to_string();
        let alice = CollabClient::connect(&addr, "Alice".to_string())
            .await
            .unwrap();
        let mut bob = CollabClient::connect(&addr, "Bob".to_string())
            .await
            .unwrap();

        // Alice inserts character '!' at pos 0
        alice.send_insert_char(0, '!');

        let mut insert_received = false;
        for _ in 0..20 {
            if let Some(CollabMessage::InsertChar { pos, char, .. }) = bob.try_recv()
                && pos == 0
                && char == '!'
            {
                insert_received = true;
                break;
            }
            sleep(Duration::from_millis(20)).await;
        }
        assert!(insert_received, "Bob should receive InsertChar");

        // Alice toggles task on line 1
        alice.send_toggle_task(1, true);

        let mut toggle_received = false;
        for _ in 0..20 {
            if let Some(CollabMessage::ToggleTask { line, checked, .. }) = bob.try_recv()
                && line == 1
                && checked
            {
                toggle_received = true;
                break;
            }
            sleep(Duration::from_millis(20)).await;
        }
        assert!(toggle_received, "Bob should receive ToggleTask");

        // Verify server's synchronized rope content
        {
            let s = server.state.lock().await;
            let content = s.content.to_string();
            assert!(content.contains("- [x] Task item"));
        }

        server.stop();
    }

    #[tokio::test]
    async fn test_cursor_move_deduplication() {
        let server =
            CollabServer::bind("127.0.0.1:0", "content".to_string(), "test.md".to_string())
                .await
                .unwrap();

        let addr = server.local_addr.to_string();
        let alice = CollabClient::connect(&addr, "Alice".to_string())
            .await
            .unwrap();
        let mut bob = CollabClient::connect(&addr, "Bob".to_string())
            .await
            .unwrap();

        // Alice moves cursor to line 3, col 7
        alice.send_cursor_move(3, 7);
        // Alice sends same cursor move again immediately (e.g. duplicate draw cycle)
        alice.send_cursor_move(3, 7);

        // Bob should receive exactly one cursor move
        let mut moves_count = 0;
        for _ in 0..20 {
            while let Some(msg) = bob.try_recv() {
                if let CollabMessage::CursorMove { line, col, .. } = msg
                    && line == 3
                    && col == 7
                {
                    moves_count += 1;
                }
            }
            sleep(Duration::from_millis(15)).await;
        }

        assert_eq!(
            moves_count, 1,
            "Duplicate cursor move should be deduplicated"
        );
        server.stop();
    }
}
