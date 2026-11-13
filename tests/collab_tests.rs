use std::path::PathBuf;
use std::time::Duration;
use tokio::time::sleep;

use skat_os::cli::{CliCommand, parse_cli_args};
use skat_os::collab::{CollabClient, CollabMessage, CollabServer};
use skat_os::editor::buffer::TextBuffer;
use skat_os::runner::Runner;

#[test]
fn test_cli_argument_parsing() {
    // 1. Default invocation
    let cmd = parse_cli_args(&["skat-os".to_string()]).unwrap();
    assert_eq!(
        cmd,
        CliCommand::Local {
            workspace: PathBuf::from("./docs")
        }
    );

    // 2. Custom workspace
    let cmd = parse_cli_args(&["skat-os".to_string(), "./my_notes".to_string()]).unwrap();
    assert_eq!(
        cmd,
        CliCommand::Local {
            workspace: PathBuf::from("./my_notes")
        }
    );

    // 3. Host with defaults
    let cmd = parse_cli_args(&["skat-os".to_string(), "host".to_string()]).unwrap();
    match cmd {
        CliCommand::Host {
            port,
            workspace,
            headless,
            ..
        } => {
            assert_eq!(port, 2222);
            assert_eq!(workspace, PathBuf::from("./docs"));
            assert!(!headless);
        }
        _ => panic!("Expected CliCommand::Host"),
    }

    // 4. Host with explicit flags
    let cmd = parse_cli_args(&[
        "skat-os".to_string(),
        "host".to_string(),
        "--port".to_string(),
        "4444".to_string(),
        "--workspace".to_string(),
        "/tmp/docs".to_string(),
        "--name".to_string(),
        "Alice".to_string(),
        "--headless".to_string(),
    ])
    .unwrap();
    assert_eq!(
        cmd,
        CliCommand::Host {
            port: 4444,
            workspace: PathBuf::from("/tmp/docs"),
            name: "Alice".to_string(),
            headless: true,
        }
    );

    // 5. Join command
    let cmd = parse_cli_args(&[
        "skat-os".to_string(),
        "join".to_string(),
        "192.168.1.50:2222".to_string(),
        "--name".to_string(),
        "Bob".to_string(),
    ])
    .unwrap();
    assert_eq!(
        cmd,
        CliCommand::Join {
            addr: "192.168.1.50:2222".to_string(),
            name: "Bob".to_string(),
        }
    );
}

#[tokio::test]
async fn test_collab_server_and_client_handshake_and_doc_sync() {
    let initial_doc = "# Living Runbook\n\n```bash\necho ready\n```\n";
    let server_handle = CollabServer::bind(
        "127.0.0.1:0",
        initial_doc.to_string(),
        "runbook.md".to_string(),
    )
    .await
    .expect("Server should bind to ephemeral port");

    let addr = format!("127.0.0.1:{}", server_handle.local_addr.port());

    // Connect Client 1 (Alice)
    let mut alice = CollabClient::connect(&addr, "Alice".to_string())
        .await
        .expect("Alice should connect");

    assert_eq!(alice.doc_name, "runbook.md");
    assert_eq!(alice.initial_content, initial_doc);
    assert_eq!(alice.initial_peers.len(), 1); // Alice is registered
    assert_eq!(alice.initial_peers[0].name, "Alice");

    // Connect Client 2 (Bob)
    let bob = CollabClient::connect(&addr, "Bob".to_string())
        .await
        .expect("Bob should connect");

    assert_eq!(bob.doc_name, "runbook.md");
    assert_eq!(bob.initial_content, initial_doc);
    // Bob should see Alice and himself
    assert_eq!(bob.initial_peers.len(), 2);

    // Alice should receive PeerJoined message for Bob
    let mut received_joined = false;
    for _ in 0..10 {
        while let Some(msg) = alice.try_recv() {
            if let CollabMessage::PeerJoined { peer } = msg {
                assert_eq!(peer.name, "Bob");
                received_joined = true;
                break;
            }
        }
        if received_joined {
            break;
        }
        sleep(Duration::from_millis(20)).await;
    }
    assert!(
        received_joined,
        "Alice should have received PeerJoined for Bob"
    );

    server_handle.stop();
}

#[tokio::test]
async fn test_realtime_typing_and_editing_synchronization() {
    let initial_doc = "Hello";
    let server_handle = CollabServer::bind(
        "127.0.0.1:0",
        initial_doc.to_string(),
        "test.md".to_string(),
    )
    .await
    .unwrap();
    let addr = format!("127.0.0.1:{}", server_handle.local_addr.port());

    let mut alice = CollabClient::connect(&addr, "Alice".to_string())
        .await
        .unwrap();
    let mut bob = CollabClient::connect(&addr, "Bob".to_string())
        .await
        .unwrap();

    let mut alice_buf = TextBuffer::from_str(&alice.initial_content);
    let mut bob_buf = TextBuffer::from_str(&bob.initial_content);

    // Alice moves cursor to end of "Hello"
    alice_buf.move_end();
    assert_eq!(alice_buf.cursor_col, 5);

    // Alice types ' ' then 'W'
    let pos1 = alice_buf.char_index_at_cursor();
    alice_buf.insert_char(' ');
    alice.send_insert_char(pos1, ' ');

    let pos2 = alice_buf.char_index_at_cursor();
    alice_buf.insert_char('W');
    alice.send_insert_char(pos2, 'W');

    // Bob receives both InsertChars
    let mut chars_received = 0;
    for _ in 0..20 {
        while let Some(msg) = bob.try_recv() {
            if let CollabMessage::InsertChar { pos, char, .. } = msg {
                bob_buf.apply_remote_insert(pos, char);
                chars_received += 1;
            }
        }
        if chars_received == 2 {
            break;
        }
        sleep(Duration::from_millis(20)).await;
    }

    assert_eq!(chars_received, 2);
    assert_eq!(alice_buf.get_text(), "Hello W");
    assert_eq!(bob_buf.get_text(), "Hello W");

    // Bob moves cursor to end and deletes 'W'
    bob_buf.move_end();
    let bob_del_pos = bob_buf.char_index_at_cursor() - 1;
    bob_buf.backspace();
    bob.send_delete_char(bob_del_pos);

    // Alice receives DeleteChar
    let mut delete_received = false;
    for _ in 0..20 {
        while let Some(msg) = alice.try_recv() {
            if let CollabMessage::DeleteChar { pos, .. } = msg {
                alice_buf.apply_remote_delete(pos);
                delete_received = true;
                break;
            }
        }
        if delete_received {
            break;
        }
        sleep(Duration::from_millis(20)).await;
    }

    assert!(delete_received);
    assert_eq!(alice_buf.get_text(), "Hello ");
    assert_eq!(bob_buf.get_text(), "Hello ");

    server_handle.stop();
}

#[tokio::test]
async fn test_multi_client_cursor_broadcast() {
    let server_handle = CollabServer::bind(
        "127.0.0.1:0",
        "Line 1\nLine 2\nLine 3\n".to_string(),
        "test.md".to_string(),
    )
    .await
    .unwrap();
    let addr = format!("127.0.0.1:{}", server_handle.local_addr.port());

    let alice = CollabClient::connect(&addr, "Alice".to_string())
        .await
        .unwrap();
    let mut bob = CollabClient::connect(&addr, "Bob".to_string())
        .await
        .unwrap();

    // Alice moves cursor to Line 2, Col 4
    alice.send_cursor_move(2, 4);

    let mut cursor_received = false;
    for _ in 0..20 {
        while let Some(msg) = bob.try_recv() {
            if let CollabMessage::CursorMove { peer_id, line, col } = msg
                && peer_id == alice.my_id
            {
                assert_eq!(line, 2);
                assert_eq!(col, 4);
                cursor_received = true;
                break;
            }
        }
        if cursor_received {
            break;
        }
        sleep(Duration::from_millis(20)).await;
    }

    assert!(
        cursor_received,
        "Bob should receive Alice's cursor movement"
    );
    server_handle.stop();
}

#[tokio::test]
async fn test_shared_code_block_execution_broadcast() {
    let doc = "# Runbook\n\n```bash\necho \"distributed shell\"\n```\n";
    let server_handle =
        CollabServer::bind("127.0.0.1:0", doc.to_string(), "runbook.md".to_string())
            .await
            .unwrap();
    let addr = format!("127.0.0.1:{}", server_handle.local_addr.port());

    let alice = CollabClient::connect(&addr, "Alice".to_string())
        .await
        .unwrap();
    let mut bob = CollabClient::connect(&addr, "Bob".to_string())
        .await
        .unwrap();

    let mut bob_buf = TextBuffer::from_str(&bob.initial_content);

    // Alice executes the code block and broadcasts RunBlock
    alice.send_run_block("block_0".to_string());

    // Alice's runner executes block locally
    let mut alice_buf = TextBuffer::from_str(&alice.initial_content);
    let block = Runner::find_code_block_by_id(&alice_buf, "block_0").expect("Block should exist");
    let temp_dir = tempfile::tempdir().unwrap();
    let result = Runner::execute(&block, temp_dir.path()).await.unwrap();

    assert_eq!(result.exit_code, 0);
    assert!(result.stdout.contains("distributed shell"));

    // Alice applies locally and broadcasts OutputUpdate
    Runner::apply_output(&mut alice_buf, &block, &result);
    alice.send_output_update(
        "block_0".to_string(),
        result.stdout.clone(),
        result.exit_code,
    );

    // Bob receives RunBlock and OutputUpdate
    let mut run_received = false;
    let mut output_received = false;

    for _ in 0..25 {
        while let Some(msg) = bob.try_recv() {
            match msg {
                CollabMessage::RunBlock { block_id, .. } => {
                    assert_eq!(block_id, "block_0");
                    run_received = true;
                }
                CollabMessage::OutputUpdate {
                    block_id,
                    text,
                    exit_code,
                    ..
                } => {
                    assert_eq!(block_id, "block_0");
                    assert_eq!(exit_code, 0);
                    Runner::apply_output_by_id(&mut bob_buf, &block_id, &text, exit_code);
                    output_received = true;
                }
                _ => {}
            }
        }
        if run_received && output_received {
            break;
        }
        sleep(Duration::from_millis(20)).await;
    }

    assert!(run_received, "Bob should receive RunBlock broadcast");
    assert!(output_received, "Bob should receive OutputUpdate broadcast");

    // Both buffers must contain the exact same injected output
    assert!(alice_buf.get_text().contains("```output [exit: 0]"));
    assert!(alice_buf.get_text().contains("distributed shell"));
    assert!(bob_buf.get_text().contains("```output [exit: 0]"));
    assert!(bob_buf.get_text().contains("distributed shell"));
    assert_eq!(alice_buf.get_text(), bob_buf.get_text());

    server_handle.stop();
}

#[tokio::test]
async fn test_client_disconnect_handling() {
    let server_handle = CollabServer::bind(
        "127.0.0.1:0",
        "Test document\n".to_string(),
        "test.md".to_string(),
    )
    .await
    .unwrap();
    let addr = format!("127.0.0.1:{}", server_handle.local_addr.port());

    let mut alice = CollabClient::connect(&addr, "Alice".to_string())
        .await
        .unwrap();
    let bob = CollabClient::connect(&addr, "Bob".to_string())
        .await
        .unwrap();
    let bob_id = bob.my_id.clone();

    // Consume Bob's joined notification on Alice
    sleep(Duration::from_millis(50)).await;
    while alice.try_recv().is_some() {}

    // Bob disconnects (drop bob)
    drop(bob);

    // Alice should receive PeerLeft message
    let mut left_received = false;
    for _ in 0..20 {
        while let Some(msg) = alice.try_recv() {
            if let CollabMessage::PeerLeft { peer_id } = msg
                && peer_id == bob_id
            {
                left_received = true;
                break;
            }
        }
        if left_received {
            break;
        }
        sleep(Duration::from_millis(20)).await;
    }

    assert!(
        left_received,
        "Alice should receive PeerLeft when Bob disconnects"
    );
    server_handle.stop();
}

#[tokio::test]
async fn test_three_way_concurrent_collaboration() {
    let server_handle =
        CollabServer::bind("127.0.0.1:0", "Doc:\n".to_string(), "shared.md".to_string())
            .await
            .unwrap();
    let addr = format!("127.0.0.1:{}", server_handle.local_addr.port());

    let alice = CollabClient::connect(&addr, "Alice".to_string())
        .await
        .unwrap();
    let bob = CollabClient::connect(&addr, "Bob".to_string())
        .await
        .unwrap();
    let mut charlie = CollabClient::connect(&addr, "Charlie".to_string())
        .await
        .unwrap();

    // Alice types 'A', Bob types 'B'
    alice.send_insert_char(5, 'A');
    bob.send_insert_char(6, 'B');

    let mut count = 0;
    for _ in 0..25 {
        while let Some(msg) = charlie.try_recv() {
            if let CollabMessage::InsertChar { .. } = msg {
                count += 1;
            }
        }
        if count >= 2 {
            break;
        }
        sleep(Duration::from_millis(20)).await;
    }

    assert!(
        count >= 2,
        "Charlie should receive both inserts from Alice and Bob"
    );
    server_handle.stop();
}

#[tokio::test]
async fn test_multi_peer_task_checkbox_toggling() {
    let initial_doc = r#"# Living Runbook
- [ ] Deploy web service
- [ ] Run database migrations
* [ ] Verify health metrics
"#;

    let server_handle = CollabServer::bind(
        "127.0.0.1:0",
        initial_doc.to_string(),
        "runbook.md".to_string(),
    )
    .await
    .unwrap();
    let addr = format!("127.0.0.1:{}", server_handle.local_addr.port());

    let mut alice = CollabClient::connect(&addr, "Alice".to_string())
        .await
        .unwrap();
    let mut bob = CollabClient::connect(&addr, "Bob".to_string())
        .await
        .unwrap();

    let mut alice_buf = TextBuffer::from_str(&alice.initial_content);
    let mut bob_buf = TextBuffer::from_str(&bob.initial_content);

    // Initial state: 0/3 tasks completed
    assert_eq!(alice_buf.task_stats(), Some((0, 3)));
    assert_eq!(bob_buf.task_stats(), Some((0, 3)));

    // 1. Alice completes task on line 1 ("- [ ] Deploy web service")
    assert_eq!(alice_buf.toggle_task_at_line(1), Some(true));
    assert_eq!(alice_buf.line_to_string(1), "- [x] Deploy web service");
    assert_eq!(alice_buf.task_stats(), Some((1, 3)));
    alice.send_toggle_task(1, true);

    // Bob receives ToggleTask from Alice
    let mut bob_received_toggle = false;
    for _ in 0..25 {
        while let Some(msg) = bob.try_recv() {
            if let CollabMessage::ToggleTask {
                peer_id: _,
                line,
                checked,
            } = msg
            {
                assert_eq!(line, 1);
                assert!(checked);
                assert!(bob_buf.set_task_checked(line, checked));
                bob_received_toggle = true;
            }
        }
        if bob_received_toggle {
            break;
        }
        sleep(Duration::from_millis(20)).await;
    }
    assert!(bob_received_toggle, "Bob should receive Alice's ToggleTask");
    assert_eq!(bob_buf.line_to_string(1), "- [x] Deploy web service");
    assert_eq!(bob_buf.task_stats(), Some((1, 3)));

    // 2. Bob completes task on line 2 ("- [ ] Run database migrations")
    assert_eq!(bob_buf.toggle_task_at_line(2), Some(true));
    assert_eq!(bob_buf.line_to_string(2), "- [x] Run database migrations");
    assert_eq!(bob_buf.task_stats(), Some((2, 3)));
    bob.send_toggle_task(2, true);

    // Alice receives ToggleTask from Bob
    let mut alice_received_toggle = false;
    for _ in 0..25 {
        while let Some(msg) = alice.try_recv() {
            if let CollabMessage::ToggleTask {
                peer_id: _,
                line,
                checked,
            } = msg
                && line == 2
                && checked
            {
                assert!(alice_buf.set_task_checked(line, checked));
                alice_received_toggle = true;
            }
        }
        if alice_received_toggle {
            break;
        }
        sleep(Duration::from_millis(20)).await;
    }
    assert!(
        alice_received_toggle,
        "Alice should receive Bob's ToggleTask"
    );
    assert_eq!(alice_buf.line_to_string(2), "- [x] Run database migrations");
    assert_eq!(alice_buf.task_stats(), Some((2, 3)));

    // 3. New peer (Charlie) connects: server state should have both tasks checked in DocSync
    let charlie = CollabClient::connect(&addr, "Charlie".to_string())
        .await
        .unwrap();
    let charlie_buf = TextBuffer::from_str(&charlie.initial_content);
    assert_eq!(charlie_buf.line_to_string(1), "- [x] Deploy web service");
    assert_eq!(
        charlie_buf.line_to_string(2),
        "- [x] Run database migrations"
    );
    assert_eq!(charlie_buf.task_stats(), Some((2, 3)));

    server_handle.stop();
}

#[tokio::test]
async fn test_multi_peer_playbook_sequential_execution_broadcast() {
    let initial_doc = r#"# Distributed Playbook
```bash name=fetch_cluster
echo "prod-east-1"
```
```bash name=verify_deploy
echo "Targeting cluster: {{fetch_cluster.stdout}}"
```
"#;

    let server_handle = CollabServer::bind(
        "127.0.0.1:0",
        initial_doc.to_string(),
        "playbook.md".to_string(),
    )
    .await
    .unwrap();
    let addr = format!("127.0.0.1:{}", server_handle.local_addr.port());

    let alice = CollabClient::connect(&addr, "Alice".to_string())
        .await
        .unwrap();
    let mut bob = CollabClient::connect(&addr, "Bob".to_string())
        .await
        .unwrap();

    let mut alice_buf = TextBuffer::from_str(&alice.initial_content);
    let mut bob_buf = TextBuffer::from_str(&bob.initial_content);

    let blocks = Runner::all_code_blocks(&alice_buf);
    assert_eq!(blocks.len(), 2);

    let temp_dir = tempfile::tempdir().unwrap();

    // Alice executes playbook sequentially and broadcasts progress at each step
    let alice_client = &alice;
    let report = Runner::execute_playbook(
        &blocks,
        temp_dir.path(),
        std::collections::HashMap::new(),
        |_idx, block| {
            alice_client.send_run_block(block.id.clone());
        },
        |_idx, block, res| {
            Runner::apply_output(&mut alice_buf, block, res);
            alice_client.send_output_update(block.id.clone(), res.stdout.clone(), res.exit_code);
        },
    )
    .await
    .unwrap();

    assert!(report.success);
    assert_eq!(report.executed_steps, 2);

    // Bob listens for RunBlock and OutputUpdate events
    let mut run_blocks_seen = Vec::new();
    let mut output_updates_seen = Vec::new();

    for _ in 0..40 {
        while let Some(msg) = bob.try_recv() {
            match msg {
                CollabMessage::RunBlock { block_id, .. } => {
                    run_blocks_seen.push(block_id);
                }
                CollabMessage::OutputUpdate {
                    block_id,
                    text,
                    exit_code,
                    ..
                } => {
                    output_updates_seen.push(block_id.clone());
                    Runner::apply_output_by_id(&mut bob_buf, &block_id, &text, exit_code);
                }
                _ => {}
            }
        }
        if output_updates_seen.len() >= 2 {
            break;
        }
        sleep(Duration::from_millis(20)).await;
    }

    assert_eq!(run_blocks_seen, vec!["fetch_cluster", "verify_deploy"]);
    assert_eq!(output_updates_seen, vec!["fetch_cluster", "verify_deploy"]);

    // Both peers' buffers must contain the interpolated chained execution output
    let bob_text = bob_buf.get_text();
    assert!(bob_text.contains("prod-east-1"));
    assert!(bob_text.contains("Targeting cluster: prod-east-1"));
    assert_eq!(alice_buf.get_text(), bob_buf.get_text());

    server_handle.stop();
}

#[tokio::test]
async fn test_multi_peer_streaming_broadcast_synchronization() {
    use skat_os::runner::OutputStatus;

    let doc = "# Living Runbook\n\n```bash name=deploy\necho deploy\n```\n";
    let server_handle =
        CollabServer::bind("127.0.0.1:0", doc.to_string(), "runbook.md".to_string())
            .await
            .expect("Server should bind");
    let addr = format!("127.0.0.1:{}", server_handle.local_addr.port());

    let alice = CollabClient::connect(&addr, "Alice".to_string())
        .await
        .unwrap();
    let mut bob = CollabClient::connect(&addr, "Bob".to_string())
        .await
        .unwrap();

    let mut alice_buf = TextBuffer::from_str(doc);
    let mut bob_buf = TextBuffer::from_str(&bob.initial_content);

    // 1. Alice starts streaming output for `deploy`
    alice.send_stream_start("deploy".to_string());
    Runner::apply_streaming_status(&mut alice_buf, "deploy", OutputStatus::Running, "");

    // 2. Alice streams chunks
    let mut alice_acc = String::new();
    Runner::append_stream_chunk(
        &mut alice_buf,
        "deploy",
        "Pulling container image...\n",
        &mut alice_acc,
    );
    alice.send_stream_chunk(
        "deploy".to_string(),
        "Pulling container image...\n".to_string(),
    );

    Runner::append_stream_chunk(
        &mut alice_buf,
        "deploy",
        "Container started successfully.\n",
        &mut alice_acc,
    );
    alice.send_stream_chunk(
        "deploy".to_string(),
        "Container started successfully.\n".to_string(),
    );

    // 3. Alice ends stream with exit code 0
    let processed = skat_os::ansi::process_terminal_stream(&alice_acc);
    Runner::apply_streaming_status(&mut alice_buf, "deploy", OutputStatus::Exit(0), &processed);
    alice.send_stream_end("deploy".to_string(), 0);

    // 4. Bob receives and applies live streaming events
    let mut bob_received_start = false;
    let mut bob_chunks_count = 0;
    let mut bob_received_end = false;
    let mut bob_acc = String::new();

    for _ in 0..50 {
        while let Some(msg) = bob.try_recv() {
            match msg {
                CollabMessage::StreamStart { block_id, .. } => {
                    assert_eq!(block_id, "deploy");
                    bob_received_start = true;
                    Runner::apply_streaming_status(
                        &mut bob_buf,
                        &block_id,
                        OutputStatus::Running,
                        "",
                    );
                }
                CollabMessage::StreamChunk {
                    block_id, chunk, ..
                } => {
                    assert_eq!(block_id, "deploy");
                    bob_chunks_count += 1;
                    Runner::append_stream_chunk(&mut bob_buf, &block_id, &chunk, &mut bob_acc);
                }
                CollabMessage::StreamEnd {
                    block_id,
                    exit_code,
                    ..
                } => {
                    assert_eq!(block_id, "deploy");
                    assert_eq!(exit_code, 0);
                    bob_received_end = true;
                    let processed = skat_os::ansi::process_terminal_stream(&bob_acc);
                    Runner::apply_streaming_status(
                        &mut bob_buf,
                        &block_id,
                        OutputStatus::Exit(exit_code),
                        &processed,
                    );
                }
                _ => {}
            }
        }
        if bob_received_end {
            break;
        }
        sleep(Duration::from_millis(20)).await;
    }

    assert!(bob_received_start);
    assert_eq!(bob_chunks_count, 2);
    assert!(bob_received_end);

    // Both peers now have identical buffer content with live output
    let bob_text = bob_buf.get_text();
    assert!(bob_text.contains("```output [exit: 0]"));
    assert!(bob_text.contains("Pulling container image..."));
    assert!(bob_text.contains("Container started successfully."));
    assert_eq!(alice_buf.get_text(), bob_buf.get_text());

    server_handle.stop();
}

#[tokio::test]
async fn test_multi_peer_stream_interruption_broadcast() {
    use skat_os::runner::OutputStatus;

    let doc = "# Backup Runbook\n\n```bash name=backup\nsleep 50\n```\n";
    let server_handle = CollabServer::bind("127.0.0.1:0", doc.to_string(), "backup.md".to_string())
        .await
        .expect("Server should bind");
    let addr = format!("127.0.0.1:{}", server_handle.local_addr.port());

    let alice = CollabClient::connect(&addr, "Alice".to_string())
        .await
        .unwrap();
    let mut bob = CollabClient::connect(&addr, "Bob".to_string())
        .await
        .unwrap();

    let mut alice_buf = TextBuffer::from_str(doc);
    let mut bob_buf = TextBuffer::from_str(&bob.initial_content);

    // Alice starts stream and sends partial output
    alice.send_stream_start("backup".to_string());
    Runner::apply_streaming_status(&mut alice_buf, "backup", OutputStatus::Running, "");

    let mut alice_acc = String::new();
    Runner::append_stream_chunk(
        &mut alice_buf,
        "backup",
        "Archiving /var/data...\n",
        &mut alice_acc,
    );
    alice.send_stream_chunk("backup".to_string(), "Archiving /var/data...\n".to_string());

    // Alice is interrupted with Ctrl+C (exit_code 130)
    let processed = skat_os::ansi::process_terminal_stream(&alice_acc);
    Runner::apply_streaming_status(
        &mut alice_buf,
        "backup",
        OutputStatus::Interrupted,
        &processed,
    );
    alice.send_stream_end("backup".to_string(), 130);

    // Bob processes network messages
    let mut bob_acc = String::new();
    let mut bob_interrupted = false;

    for _ in 0..50 {
        while let Some(msg) = bob.try_recv() {
            match msg {
                CollabMessage::StreamStart { block_id, .. } => {
                    Runner::apply_streaming_status(
                        &mut bob_buf,
                        &block_id,
                        OutputStatus::Running,
                        "",
                    );
                }
                CollabMessage::StreamChunk {
                    block_id, chunk, ..
                } => {
                    Runner::append_stream_chunk(&mut bob_buf, &block_id, &chunk, &mut bob_acc);
                }
                CollabMessage::StreamEnd {
                    block_id,
                    exit_code,
                    ..
                } => {
                    assert_eq!(exit_code, 130);
                    bob_interrupted = true;
                    let processed = skat_os::ansi::process_terminal_stream(&bob_acc);
                    Runner::apply_streaming_status(
                        &mut bob_buf,
                        &block_id,
                        OutputStatus::Interrupted,
                        &processed,
                    );
                }
                _ => {}
            }
        }
        if bob_interrupted {
            break;
        }
        sleep(Duration::from_millis(20)).await;
    }

    assert!(bob_interrupted);
    let bob_text = bob_buf.get_text();
    assert!(bob_text.contains("```output [interrupted]\nArchiving /var/data...\n```"));
    assert_eq!(alice_buf.get_text(), bob_buf.get_text());

    server_handle.stop();
}

#[tokio::test]
async fn test_multi_peer_realtime_streaming_command_execution() {
    use skat_os::runner::OutputStatus;

    let doc = "# Build Pipeline\n\n```bash name=pipeline\necho \"Step 1: Lint\"\nsleep 0.05\necho \"Step 2: Test\"\n```\n";
    let server_handle =
        CollabServer::bind("127.0.0.1:0", doc.to_string(), "pipeline.md".to_string())
            .await
            .expect("Server should bind");
    let addr = format!("127.0.0.1:{}", server_handle.local_addr.port());

    let alice = CollabClient::connect(&addr, "Alice".to_string())
        .await
        .unwrap();
    let mut bob = CollabClient::connect(&addr, "Bob".to_string())
        .await
        .unwrap();

    let mut alice_buf = TextBuffer::from_str(doc);
    let mut bob_buf = TextBuffer::from_str(&bob.initial_content);

    let block = Runner::find_code_block(&alice_buf, 2).unwrap();
    let temp_dir = tempfile::tempdir().unwrap();

    // Alice runs block using execute_streaming and relays chunks to Bob
    alice.send_stream_start(block.id.clone());
    Runner::apply_streaming_status(&mut alice_buf, &block.id, OutputStatus::Running, "");

    let alice_client = std::sync::Arc::new(alice);
    let client_for_chunk = std::sync::Arc::clone(&alice_client);
    let block_id_for_chunk = block.id.clone();
    let alice_acc = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
    let acc_for_chunk = std::sync::Arc::clone(&alice_acc);

    let (res_tx, res_rx) = tokio::sync::oneshot::channel();
    let _handle = Runner::execute_streaming(
        &block,
        temp_dir.path(),
        &std::collections::HashMap::new(),
        move |chunk| {
            acc_for_chunk.lock().unwrap().push_str(&chunk);
            client_for_chunk.send_stream_chunk(block_id_for_chunk.clone(), chunk);
        },
        move |result| {
            let _ = res_tx.send(result);
        },
    )
    .await
    .unwrap();

    let result = res_rx.await.unwrap();
    assert_eq!(result.exit_code, 0);
    alice_client.send_stream_end(block.id.clone(), result.exit_code);

    let final_acc = alice_acc.lock().unwrap().clone();
    let processed = skat_os::ansi::process_terminal_stream(&final_acc);
    Runner::apply_streaming_status(
        &mut alice_buf,
        &block.id,
        OutputStatus::Exit(result.exit_code),
        &processed,
    );

    // Bob receives events over TCP in real time
    let mut bob_acc = String::new();
    let mut bob_done = false;

    for _ in 0..60 {
        while let Some(msg) = bob.try_recv() {
            match msg {
                CollabMessage::StreamStart { block_id, .. } => {
                    Runner::apply_streaming_status(
                        &mut bob_buf,
                        &block_id,
                        OutputStatus::Running,
                        "",
                    );
                }
                CollabMessage::StreamChunk {
                    block_id, chunk, ..
                } => {
                    Runner::append_stream_chunk(&mut bob_buf, &block_id, &chunk, &mut bob_acc);
                }
                CollabMessage::StreamEnd {
                    block_id,
                    exit_code,
                    ..
                } => {
                    bob_done = true;
                    let processed = skat_os::ansi::process_terminal_stream(&bob_acc);
                    Runner::apply_streaming_status(
                        &mut bob_buf,
                        &block_id,
                        OutputStatus::Exit(exit_code),
                        &processed,
                    );
                }
                _ => {}
            }
        }
        if bob_done {
            break;
        }
        sleep(Duration::from_millis(20)).await;
    }

    assert!(bob_done);
    let bob_text = bob_buf.get_text();
    assert!(bob_text.contains("Step 1: Lint"));
    assert!(bob_text.contains("Step 2: Test"));
    assert!(bob_text.contains("```output [exit: 0]"));

    server_handle.stop();
}

#[tokio::test]
async fn test_multi_peer_parameter_update_broadcast() {
    use skat_os::form::FormParser;

    let doc = "# Cluster Runbook\n\nTarget: {{param:select:CLUSTER=prod-us-east,prod-eu-west,staging}}\nNamespace: {{param:input:NAMESPACE=default}}\nToggle: {{param:toggle:DRY_RUN=false}}\n";
    let server_handle =
        CollabServer::bind("127.0.0.1:0", doc.to_string(), "cluster.md".to_string())
            .await
            .expect("Server should bind");
    let addr = format!("127.0.0.1:{}", server_handle.local_addr.port());

    let alice = CollabClient::connect(&addr, "Alice".to_string())
        .await
        .unwrap();
    let mut bob = CollabClient::connect(&addr, "Bob".to_string())
        .await
        .unwrap();

    let mut alice_buf = TextBuffer::from_str(doc);
    let mut bob_buf = TextBuffer::from_str(&bob.initial_content);

    // Alice updates CLUSTER to prod-eu-west
    alice.send_param_update("CLUSTER".to_string(), "prod-eu-west".to_string());
    FormParser::update_param_in_buffer(&mut alice_buf, "CLUSTER", "prod-eu-west");

    // Alice also updates DRY_RUN to true
    alice.send_param_update("DRY_RUN".to_string(), "true".to_string());
    FormParser::update_param_in_buffer(&mut alice_buf, "DRY_RUN", "true");

    // Bob receives both parameter updates over TCP
    let mut received_cluster = false;
    let mut received_dry_run = false;

    for _ in 0..50 {
        while let Some(msg) = bob.try_recv() {
            if let CollabMessage::ParamUpdate {
                param_name, value, ..
            } = msg
            {
                FormParser::update_param_in_buffer(&mut bob_buf, &param_name, &value);
                if param_name == "CLUSTER" && value == "prod-eu-west" {
                    received_cluster = true;
                }
                if param_name == "DRY_RUN" && value == "true" {
                    received_dry_run = true;
                }
            }
        }
        if received_cluster && received_dry_run {
            break;
        }
        sleep(Duration::from_millis(20)).await;
    }

    assert!(received_cluster, "Bob should have received CLUSTER update");
    assert!(received_dry_run, "Bob should have received DRY_RUN update");

    // Both buffers should match
    assert_eq!(alice_buf.get_text(), bob_buf.get_text());
    assert!(
        bob_buf
            .get_text()
            .contains("CLUSTER=prod-eu-west,staging,prod-us-east")
    );
    assert!(bob_buf.get_text().contains("DRY_RUN=true"));

    // Connect Charlie late - should receive the updated document from the server state
    let charlie = CollabClient::connect(&addr, "Charlie".to_string())
        .await
        .unwrap();
    assert!(
        charlie
            .initial_content
            .contains("CLUSTER=prod-eu-west,staging,prod-us-east")
    );
    assert!(charlie.initial_content.contains("DRY_RUN=true"));

    server_handle.stop();
}

#[tokio::test]
async fn test_multi_peer_parameterized_runbook_execution() {
    use skat_os::form::FormParser;
    use std::collections::HashMap;

    let doc = "# Deployment Runbook\n\nTarget Region: {{param:input:REGION=us-west-2}}\n\n```bash name=deploy\necho \"Deploying to {{params.REGION}} ($SKAT_PARAM_REGION)\"\n```\n";
    let server_handle = CollabServer::bind("127.0.0.1:0", doc.to_string(), "deploy.md".to_string())
        .await
        .expect("Server should bind");
    let addr = format!("127.0.0.1:{}", server_handle.local_addr.port());

    let mut alice = CollabClient::connect(&addr, "Alice".to_string())
        .await
        .unwrap();
    let bob = CollabClient::connect(&addr, "Bob".to_string())
        .await
        .unwrap();

    let mut alice_buf = TextBuffer::from_str(doc);
    let mut bob_buf = TextBuffer::from_str(&bob.initial_content);

    // Bob changes REGION to eu-central-1 and broadcasts
    bob.send_param_update("REGION".to_string(), "eu-central-1".to_string());
    FormParser::update_param_in_buffer(&mut bob_buf, "REGION", "eu-central-1");

    // Alice receives Bob's update
    let mut alice_received = false;
    for _ in 0..50 {
        while let Some(msg) = alice.try_recv() {
            if let CollabMessage::ParamUpdate {
                param_name, value, ..
            } = msg
                && param_name == "REGION"
                && value == "eu-central-1"
            {
                FormParser::update_param_in_buffer(&mut alice_buf, &param_name, &value);
                alice_received = true;
            }
        }
        if alice_received {
            break;
        }
        sleep(Duration::from_millis(20)).await;
    }
    assert!(alice_received);

    // Now Alice executes the block with her collected parameters
    let params = FormParser::collect_params(&alice_buf);
    assert_eq!(params.get("REGION"), Some(&"eu-central-1".to_string()));

    let blocks = Runner::all_code_blocks(&alice_buf);
    let block = &blocks[0];
    let temp_dir = tempfile::tempdir().unwrap();

    let interpolated_code = Runner::interpolate_variables_with_context(
        &block.code,
        &HashMap::new(),
        &params,
        &HashMap::new(),
    );
    let mut block_to_run = block.clone();
    block_to_run.code = interpolated_code;
    let env_vars =
        Runner::build_environment_variables_with_context(&HashMap::new(), &params, &HashMap::new());

    let result = Runner::execute_with_env(&block_to_run, temp_dir.path(), &env_vars)
        .await
        .unwrap();

    assert_eq!(result.exit_code, 0);
    assert_eq!(
        result.stdout.trim(),
        "Deploying to eu-central-1 (eu-central-1)"
    );

    // Alice updates her buffer with the output and broadcasts to Bob
    Runner::apply_output(&mut alice_buf, block, &result);
    alice.send_output_update(block.id.clone(), result.stdout.clone(), result.exit_code);

    // Bob receives execution output
    let mut bob_received_exec = false;
    let mut bob_client = bob;
    for _ in 0..50 {
        while let Some(msg) = bob_client.try_recv() {
            if let CollabMessage::OutputUpdate {
                block_id,
                text,
                exit_code,
                ..
            } = msg
                && block_id == block.id
                && exit_code == 0
            {
                Runner::apply_output_by_id(&mut bob_buf, &block_id, &text, exit_code);
                bob_received_exec = true;
            }
        }
        if bob_received_exec {
            break;
        }
        sleep(Duration::from_millis(20)).await;
    }

    assert!(bob_received_exec);
    assert_eq!(alice_buf.get_text(), bob_buf.get_text());
    assert!(
        bob_buf
            .get_text()
            .contains("Deploying to eu-central-1 (eu-central-1)")
    );

    server_handle.stop();
}

#[tokio::test]
async fn test_collab_cursor_deduplication_and_disconnection_stress() {
    let doc = "# Collaborative Session\nLine 1\nLine 2\nLine 3\nLine 4\n";
    let server_handle =
        CollabServer::bind("127.0.0.1:0", doc.to_string(), "session.md".to_string())
            .await
            .expect("Server should bind");
    let addr = format!("127.0.0.1:{}", server_handle.local_addr.port());

    let alice = CollabClient::connect(&addr, "Alice".to_string())
        .await
        .unwrap();
    let mut bob = CollabClient::connect(&addr, "Bob".to_string())
        .await
        .unwrap();

    assert!(alice.is_connected());
    assert!(bob.is_connected());

    let alice_id = alice.my_id.clone();

    // Consume joined notifications
    sleep(Duration::from_millis(50)).await;
    while bob.try_recv().is_some() {}

    // Alice sends 10 identical cursor move updates in rapid succession
    for _ in 0..10 {
        alice.send_cursor_move(4, 12);
    }

    // Bob should receive deduplicated cursor move (only 1)
    let mut cursor_moves = 0;
    for _ in 0..25 {
        while let Some(msg) = bob.try_recv() {
            if let CollabMessage::CursorMove { peer_id, line, col } = msg
                && peer_id == alice_id
                && line == 4
                && col == 12
            {
                cursor_moves += 1;
            }
        }
        sleep(Duration::from_millis(15)).await;
    }
    assert_eq!(
        cursor_moves, 1,
        "Redundant cursor moves must be deduplicated across TCP broadcast"
    );

    // Alice disconnects
    drop(alice);

    // Bob must receive PeerLeft
    let mut alice_left = false;
    for _ in 0..25 {
        while let Some(msg) = bob.try_recv() {
            if let CollabMessage::PeerLeft { peer_id } = msg
                && peer_id == alice_id
            {
                alice_left = true;
                break;
            }
        }
        if alice_left {
            break;
        }
        sleep(Duration::from_millis(15)).await;
    }

    assert!(
        alice_left,
        "Bob must receive PeerLeft notification upon Alice disconnect"
    );

    // Server state must have cleaned up Alice
    {
        let state = server_handle.state.lock().await;
        assert_eq!(state.peers.len(), 1);
        assert!(!state.peers.contains_key(&alice_id));
    }

    server_handle.stop();
}
