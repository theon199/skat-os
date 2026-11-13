use anyhow::Result;
use skat_os::app::App;
use skat_os::collab::client::CollabClient;
use skat_os::collab::protocol::CollabMessage;
use skat_os::collab::server::CollabServer;
use skat_os::editor::buffer::TextBuffer;
use skat_os::mesh::{global_ssh_pool, FleetOutput, HostOutput, HostRegistry, SshEndpoint, SshSessionPool};
use skat_os::runner::{CodeBlock, ExecutionResult, Runner};
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

#[test]
fn test_targeted_code_block_fence_parsing() {
    let doc = r#"
```bash target="ssh://ubuntu@10.0.0.12:22"
uname -a
```

```bash name="remote_app" target="ssh://app@10.0.1.5" proxy="ssh://bastion.corp.com"
systemctl status app
```

```bash target="ssh://{{params.USER}}@{{params.HOST}}"
hostname
```

```bash target="@web_servers" parallel=true
uptime
```

```bash target="@db_servers" parallel=false
pg_isready
```
"#;

    let buf = TextBuffer::from_str(doc);
    let blocks = Runner::all_code_blocks(&buf);
    assert_eq!(blocks.len(), 5);

    // Block 0: Direct SSH target
    assert_eq!(blocks[0].target.as_deref(), Some("ssh://ubuntu@10.0.0.12:22"));
    assert_eq!(blocks[0].proxy, None);
    assert!(!blocks[0].parallel);

    // Block 1: Target via Bastion / Jump Host
    assert_eq!(blocks[1].name.as_deref(), Some("remote_app"));
    assert_eq!(blocks[1].target.as_deref(), Some("ssh://app@10.0.1.5"));
    assert_eq!(blocks[1].proxy.as_deref(), Some("ssh://bastion.corp.com"));
    assert!(!blocks[1].parallel);

    // Block 2: Parameterized target
    assert_eq!(blocks[2].target.as_deref(), Some("ssh://{{params.USER}}@{{params.HOST}}"));

    // Block 3: Host group with parallel=true
    assert_eq!(blocks[3].target.as_deref(), Some("@web_servers"));
    assert!(blocks[3].parallel);

    // Block 4: Host group with parallel=false
    assert_eq!(blocks[4].target.as_deref(), Some("@db_servers"));
    assert!(!blocks[4].parallel);
}

#[test]
fn test_target_parameter_interpolation() {
    let mut params = HashMap::new();
    params.insert("USER".to_string(), "admin".to_string());
    params.insert("HOST".to_string(), "cluster.internal".to_string());
    params.insert("PORT".to_string(), "2222".to_string());
    params.insert("BASTION".to_string(), "bastion.corp.com".to_string());
    params.insert("ENV".to_string(), "prod".to_string());

    let target_template = "ssh://{{params.USER}}@{{params.HOST}}:{{params.PORT}}";
    let proxy_template = "ssh://{{params.BASTION}}";
    let group_template = "@{{params.ENV}}_servers";

    let outputs = HashMap::new();
    let tables = HashMap::new();

    let interpolated_target = Runner::interpolate_variables_with_context(target_template, &outputs, &params, &tables);
    assert_eq!(interpolated_target, "ssh://admin@cluster.internal:2222");

    let ep = SshEndpoint::parse(&interpolated_target).unwrap();
    assert_eq!(ep.user, Some("admin".to_string()));
    assert_eq!(ep.host, "cluster.internal");
    assert_eq!(ep.port, 2222);

    let interpolated_proxy = Runner::interpolate_variables_with_context(proxy_template, &outputs, &params, &tables);
    assert_eq!(interpolated_proxy, "ssh://bastion.corp.com");

    let interpolated_group = Runner::interpolate_variables_with_context(group_template, &outputs, &params, &tables);
    assert_eq!(interpolated_group, "@prod_servers");
}

#[test]
fn test_host_group_resolution_frontmatter_and_file() {
    let doc = r#"---
web_servers: [10.0.0.1, 10.0.0.2]
edge_nodes:
  - 10.0.2.1
  - 10.0.2.2
---
# Frontmatter Runbook
"#;
    let buf = TextBuffer::from_str(doc);
    let temp_dir = tempfile::tempdir().unwrap();

    // hosts.yaml in directory
    let hosts_file = temp_dir.path().join("hosts.yaml");
    std::fs::write(&hosts_file, "db_servers: [10.0.3.1, 10.0.3.2]\n").unwrap();

    // 1. Resolve from frontmatter
    let web = HostRegistry::resolve_target("@web_servers", Some(&buf), temp_dir.path()).unwrap();
    assert_eq!(web, vec!["10.0.0.1", "10.0.0.2"]);

    let edge = HostRegistry::resolve_target("@edge_nodes", Some(&buf), temp_dir.path()).unwrap();
    assert_eq!(edge, vec!["10.0.2.1", "10.0.2.2"]);

    // 2. Resolve from hosts.yaml
    let db = HostRegistry::resolve_target("@db_servers", Some(&buf), temp_dir.path()).unwrap();
    assert_eq!(db, vec!["10.0.3.1", "10.0.3.2"]);

    // 3. Resolve direct target
    let direct = HostRegistry::resolve_target("ssh://ubuntu@10.0.0.99", Some(&buf), temp_dir.path()).unwrap();
    assert_eq!(direct, vec!["ssh://ubuntu@10.0.0.99"]);

    // 4. Missing group
    let err = HostRegistry::resolve_target("@missing_servers", Some(&buf), temp_dir.path());
    assert!(err.is_err());
}

#[test]
fn test_downstream_variable_interpolation_multi_host() {
    let fleet_output = FleetOutput::new(
        "web_servers".to_string(),
        vec![
            HostOutput {
                host: "10.0.0.1".to_string(),
                stdout: "web-1 ready\n".to_string(),
                stderr: String::new(),
                exit_code: 0,
            },
            HostOutput {
                host: "10.0.0.2".to_string(),
                stdout: "web-2 degraded\n".to_string(),
                stderr: "Port 80 busy\n".to_string(),
                exit_code: 1,
            },
        ],
        true,
    );

    let mut outputs = HashMap::new();
    let res = ExecutionResult {
        stdout: fleet_output.to_output_text(),
        stderr: String::new(),
        exit_code: 1,
        timed_out: false,
    };
    outputs.insert("web_servers".to_string(), res.clone());
    outputs.insert("check_cluster".to_string(), res);

    let params = HashMap::new();
    let tables = HashMap::new();

    // Test indexed host stdout
    let code1 = "Node 0: {{web_servers[0].stdout}} | Node 1: {{web_servers[1].stdout}}";
    let interpolated1 = Runner::interpolate_variables_with_context(code1, &outputs, &params, &tables);
    assert_eq!(interpolated1, "Node 0: web-1 ready | Node 1: web-2 degraded");

    // Test indexed host stderr and exit_code
    let code2 = "Node 1 err: {{web_servers[1].stderr}}, code: {{web_servers[1].exit_code}}, host: {{web_servers[1].host}}";
    let interpolated2 = Runner::interpolate_variables_with_context(code2, &outputs, &params, &tables);
    assert_eq!(interpolated2, "Node 1 err: Port 80 busy, code: 1, host: 10.0.0.2");

    // Test aggregate fields: failed, success, count, hosts
    let code3 = "Failed: [{{web_servers.failed}}], Success: {{web_servers.success}}, Count: {{web_servers.count}}";
    let interpolated3 = Runner::interpolate_variables_with_context(code3, &outputs, &params, &tables);
    assert_eq!(interpolated3, "Failed: [10.0.0.2], Success: false, Count: 2");

    // Test reference via custom block name
    let code4 = "First host: {{check_cluster[0].host}}";
    let interpolated4 = Runner::interpolate_variables_with_context(code4, &outputs, &params, &tables);
    assert_eq!(interpolated4, "First host: 10.0.0.1");

    // Test auto-injected environment variables
    let env_vars = Runner::build_environment_variables_with_context(&outputs, &params, &tables);
    assert_eq!(env_vars.get("SKAT_WEB_SERVERS_0_STDOUT"), Some(&"web-1 ready".to_string()));
    assert_eq!(env_vars.get("SKAT_WEB_SERVERS_1_STDOUT"), Some(&"web-2 degraded".to_string()));
    assert_eq!(env_vars.get("SKAT_WEB_SERVERS_1_STDERR"), Some(&"Port 80 busy".to_string()));
    assert_eq!(env_vars.get("SKAT_WEB_SERVERS_1_EXIT_CODE"), Some(&"1".to_string()));
    assert_eq!(env_vars.get("SKAT_WEB_SERVERS_FAILED"), Some(&"10.0.0.2".to_string()));
    assert_eq!(env_vars.get("SKAT_WEB_SERVERS_SUCCESS"), Some(&"false".to_string()));
    assert_eq!(env_vars.get("SKAT_WEB_SERVERS_COUNT"), Some(&"2".to_string()));
}

#[tokio::test]
async fn test_remote_execution_and_session_pooling() {
    let pool = SshSessionPool::new();
    let call_count = Arc::new(AtomicUsize::new(0));

    let c_clone = call_count.clone();
    pool.register_mock_host("node1.internal", move |cmd, _env| {
        c_clone.fetch_add(1, Ordering::SeqCst);
        ExecutionResult {
            stdout: format!("mock-stdout: {}", cmd),
            stderr: String::new(),
            exit_code: 0,
            timed_out: false,
        }
    });

    let temp_dir = tempfile::tempdir().unwrap();

    let block1 = CodeBlock {
        id: "step1".to_string(),
        name: None,
        language: "bash".to_string(),
        code: "echo hello 1".to_string(),
        start_fence_line: 0,
        end_fence_line: 2,
        existing_output: None,
        format: skat_os::runner::BlockFormat::Text,
        target: Some("ssh://ubuntu@node1.internal:22".to_string()),
        proxy: None,
        parallel: false,
        confirm: None,
        danger: false,
        blast_radius: None,
        dry_run: false,
        approvals: 1,
    };

    let (tx1, rx1) = tokio::sync::oneshot::channel();
    let _handle1 = Runner::execute_streaming_with_pool(
        &block1,
        temp_dir.path(),
        &HashMap::new(),
        &pool,
        None,
        |_chunk| {},
        move |res| {
            let _ = tx1.send(res);
        },
    )
    .await
    .unwrap();

    let res1 = rx1.await.unwrap();
    assert_eq!(res1.exit_code, 0);
    assert!(res1.stdout.contains("echo hello 1"));
    assert_eq!(call_count.load(Ordering::SeqCst), 1);

    // Run second block on same target
    let block2 = CodeBlock {
        id: "step2".to_string(),
        name: None,
        language: "bash".to_string(),
        code: "echo hello 2".to_string(),
        start_fence_line: 4,
        end_fence_line: 6,
        existing_output: None,
        format: skat_os::runner::BlockFormat::Text,
        target: Some("ssh://ubuntu@node1.internal:22".to_string()),
        proxy: None,
        parallel: false,
        confirm: None,
        danger: false,
        blast_radius: None,
        dry_run: false,
        approvals: 1,
    };

    let (tx2, rx2) = tokio::sync::oneshot::channel();
    let _handle2 = Runner::execute_streaming_with_pool(
        &block2,
        temp_dir.path(),
        &HashMap::new(),
        &pool,
        None,
        |_chunk| {},
        move |res| {
            let _ = tx2.send(res);
        },
    )
    .await
    .unwrap();

    let res2 = rx2.await.unwrap();
    assert_eq!(res2.exit_code, 0);
    assert!(res2.stdout.contains("echo hello 2"));
    assert_eq!(call_count.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn test_parallel_fleet_fan_out_execution() {
    let pool = SshSessionPool::new();

    pool.register_mock_host("10.0.0.1", |cmd, _env| ExecutionResult {
        stdout: format!("host-1: {}\n", cmd),
        stderr: String::new(),
        exit_code: 0,
        timed_out: false,
    });

    pool.register_mock_host("10.0.0.2", |cmd, _env| ExecutionResult {
        stdout: format!("host-2: {}\n", cmd),
        stderr: String::new(),
        exit_code: 0,
        timed_out: false,
    });

    pool.register_mock_host("10.0.0.3", |cmd, _env| ExecutionResult {
        stdout: format!("host-3: {}\n", cmd),
        stderr: String::new(),
        exit_code: 0,
        timed_out: false,
    });

    let doc = r#"---
cluster: [10.0.0.1, 10.0.0.2, 10.0.0.3]
---
```bash target="@cluster" parallel=true
uptime
```
"#;
    let mut buffer = TextBuffer::from_str(doc);
    let temp_dir = tempfile::tempdir().unwrap();

    let block = Runner::find_code_block(&buffer, 4).unwrap();
    assert_eq!(block.target.as_deref(), Some("@cluster"));
    assert!(block.parallel);

    let received_chunks = Arc::new(std::sync::Mutex::new(Vec::new()));
    let rc_clone = received_chunks.clone();

    let (res_tx, res_rx) = tokio::sync::oneshot::channel();

    let _handle = Runner::execute_streaming_with_pool(
        &block,
        temp_dir.path(),
        &HashMap::new(),
        &pool,
        Some(&buffer),
        move |chunk| {
            rc_clone.lock().unwrap().push(chunk);
        },
        move |result| {
            let _ = res_tx.send(result);
        },
    )
    .await
    .unwrap();

    let result = res_rx.await.unwrap();
    assert_eq!(result.exit_code, 0);

    // Verify streaming chunks received with host metadata badges
    let chunks = received_chunks.lock().unwrap();
    assert!(!chunks.is_empty());
    let combined_chunks = chunks.join("");
    assert!(combined_chunks.contains("[10.0.0.1]"));
    assert!(combined_chunks.contains("[10.0.0.2]"));
    assert!(combined_chunks.contains("[10.0.0.3]"));

    // Apply output to buffer
    Runner::apply_output(&mut buffer, &block, &result);
    let buffer_text = buffer.get_text();

    assert!(buffer_text.contains("[10.0.0.1: ✓ exit 0] [10.0.0.2: ✓ exit 0] [10.0.0.3: ✓ exit 0]"));
    assert!(buffer_text.contains("=== [10.0.0.1] ==="));
    assert!(buffer_text.contains("=== [10.0.0.2] ==="));
    assert!(buffer_text.contains("=== [10.0.0.3] ==="));

    // Verify FleetOutput parsed cleanly from buffer output
    let parsed_fleet = FleetOutput::parse(&result.stdout).unwrap();
    assert_eq!(parsed_fleet.hosts.len(), 3);
    assert!(parsed_fleet.is_success());
    assert!(parsed_fleet.failed_hosts().is_empty());
}

#[tokio::test]
async fn test_playbook_fleet_fail_fast_safeguard() {
    // Node 1 succeeds, Node 2 fails
    global_ssh_pool().register_mock_host("10.99.0.1", |_cmd, _env| ExecutionResult {
        stdout: "node 1 ok\n".to_string(),
        stderr: String::new(),
        exit_code: 0,
        timed_out: false,
    });
    global_ssh_pool().register_mock_host("10.99.0.2", |_cmd, _env| ExecutionResult {
        stdout: String::new(),
        stderr: "deployment failed on node 2\n".to_string(),
        exit_code: 2,
        timed_out: false,
    });

    let doc = r#"---
web_nodes: [10.99.0.1, 10.99.0.2]
---
```bash name=deploy_fleet target="@web_nodes" parallel=true
deploy_service
```

```bash name=verify_deploy
echo "Deployment successful"
```
"#;

    let buffer = TextBuffer::from_str(doc);
    let temp_dir = tempfile::tempdir().unwrap();
    std::fs::write(temp_dir.path().join("hosts.yaml"), "web_nodes: [10.99.0.1, 10.99.0.2]\n").unwrap();
    let blocks = Runner::all_code_blocks(&buffer);
    assert_eq!(blocks.len(), 2);

    let executed_steps = Arc::new(AtomicUsize::new(0));
    let es_clone = executed_steps.clone();

    let report = Runner::execute_playbook(
        &blocks,
        temp_dir.path(),
        HashMap::new(),
        move |step_idx, _b| {
            es_clone.fetch_add(1, Ordering::SeqCst);
            assert_eq!(step_idx, 0, "Second step must NOT execute due to fail-fast safeguard");
        },
        |_step_idx, _b, _res| {},
    )
    .await
    .unwrap();

    assert!(!report.success);
    assert_eq!(report.executed_steps, 1);
    assert_eq!(report.failed_step, Some(0));
    assert_eq!(executed_steps.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn test_remote_streaming_and_interactive_stdin() {
    let pool = SshSessionPool::new();

    pool.register_mock_host("interactive.node", |cmd, _env| {
        ExecutionResult {
            stdout: format!("Interactive answer received for {}\n", cmd),
            stderr: String::new(),
            exit_code: 0,
            timed_out: false,
        }
    });

    let temp_dir = tempfile::tempdir().unwrap();

    let block = CodeBlock {
        id: "interactive_block".to_string(),
        name: None,
        language: "bash".to_string(),
        code: "read -p 'Confirm [y/N]: ' ans; echo $ans".to_string(),
        start_fence_line: 0,
        end_fence_line: 2,
        existing_output: None,
        format: skat_os::runner::BlockFormat::Text,
        target: Some("ssh://admin@interactive.node:22".to_string()),
        proxy: None,
        parallel: false,
        confirm: None,
        danger: false,
        blast_radius: None,
        dry_run: false,
        approvals: 1,
    };

    let (res_tx, res_rx) = tokio::sync::oneshot::channel();
    let handle = Runner::execute_streaming_with_pool(
        &block,
        temp_dir.path(),
        &HashMap::new(),
        &pool,
        None,
        |_chunk| {},
        move |res| {
            let _ = res_tx.send(res);
        },
    )
    .await
    .unwrap();

    // Send stdin keystrokes
    let _ = handle.stdin_tx.send(b"y\n".to_vec());

    let res = res_rx.await.unwrap();
    assert_eq!(res.exit_code, 0);
    assert!(res.stdout.contains("Interactive answer"));
}

#[tokio::test]
async fn test_multiplayer_remote_stream_synchronization() -> Result<()> {
    let temp_dir = tempfile::tempdir().unwrap();
    let doc_path = temp_dir.path().join("cluster.md");

    let doc_content = r#"---
fleet: [10.0.0.1, 10.0.0.2]
---
# Live Fleet Runbook

```bash name=run_fleet target="@fleet" parallel=true
echo "Checking cluster state..."
```
"#;
    std::fs::write(&doc_path, doc_content)?;

    // Register mock hosts on global pool
    global_ssh_pool().register_mock_host("10.0.0.1", |_cmd, _env| ExecutionResult {
        stdout: "node1: status ok\n".to_string(),
        stderr: String::new(),
        exit_code: 0,
        timed_out: false,
    });
    global_ssh_pool().register_mock_host("10.0.0.2", |_cmd, _env| ExecutionResult {
        stdout: "node2: status ok\n".to_string(),
        stderr: String::new(),
        exit_code: 0,
        timed_out: false,
    });

    // 1. Start zero-cloud TCP server
    let server_handle = CollabServer::bind("127.0.0.1:0", doc_content.to_string(), "cluster.md".to_string()).await?;
    let port = server_handle.local_addr.port();
    let server_addr = format!("127.0.0.1:{}", port);

    // 2. Alice connects as host
    let alice = CollabClient::connect(&server_addr, "Alice".to_string()).await?;
    tokio::time::sleep(Duration::from_millis(50)).await;

    // 3. Bob connects as client
    let mut bob_client = CollabClient::connect(&server_addr, "Bob".to_string()).await?;
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Bob receives doc sync
    let bob_buffer = TextBuffer::from_str(&bob_client.initial_content);
    assert!(bob_buffer.get_text().contains("run_fleet"));

    // 4. Alice executes targeted remote code block
    let alice_buffer = TextBuffer::from_str(doc_content);
    let block = Runner::find_code_block(&alice_buffer, 6).unwrap();

    let alice_client = Arc::new(alice);
    let client_for_chunk = Arc::clone(&alice_client);
    let b_id = block.id.clone();

    let (res_tx, res_rx) = tokio::sync::oneshot::channel();

    let _handle = Runner::execute_streaming_with_pool(
        &block,
        temp_dir.path(),
        &HashMap::new(),
        global_ssh_pool(),
        Some(&alice_buffer),
        move |chunk| {
            // Broadcast chunk with remote host metadata badge over TCP
            client_for_chunk.send_stream_chunk(b_id.clone(), chunk);
        },
        move |res| {
            let _ = res_tx.send(res);
        },
    )
    .await?;

    let res = res_rx.await.unwrap();
    assert_eq!(res.exit_code, 0);

    // Alice sends stream end
    alice_client.send_stream_end(block.id.clone(), res.exit_code);

    // 5. Bob receives streamed chunks and stream end
    tokio::time::sleep(Duration::from_millis(100)).await;
    let mut bob_chunks = Vec::new();
    let mut stream_ended = false;

    while let Some(msg) = bob_client.try_recv() {
        match msg {
            CollabMessage::StreamChunk { chunk, .. } => {
                bob_chunks.push(chunk);
            }
            CollabMessage::StreamEnd { exit_code, .. } => {
                assert_eq!(exit_code, 0);
                stream_ended = true;
            }
            _ => {}
        }
    }

    assert!(stream_ended, "Bob must receive StreamEnd message");
    assert!(!bob_chunks.is_empty(), "Bob must receive streamed remote chunks");

    let combined_bob_stream = bob_chunks.join("");
    assert!(combined_bob_stream.contains("[10.0.0.1]"));
    assert!(combined_bob_stream.contains("[10.0.0.2]"));

    server_handle.stop();
    Ok(())
}

#[test]
fn test_interactive_canvas_tab_navigation() {
    let temp_dir = tempfile::tempdir().unwrap();
    let mut app = App::new(temp_dir.path().to_path_buf()).unwrap();

    let doc = r#"---
fleet: [10.0.0.1, 10.0.0.2, 10.0.0.3]
---
```bash name=check_fleet target="@fleet"
uptime
```
```output [exit: 0]
[10.0.0.1: ✓ exit 0] [10.0.0.2: ✓ exit 0] [10.0.0.3: ✖ exit 1]

=== [10.0.0.1] ===
host 1 ok

=== [10.0.0.2] ===
host 2 ok

=== [10.0.0.3] ===
host 3 failed
```
"#;

    app.editor.buffer = TextBuffer::from_str(doc);
    app.refresh_params_and_tables();

    // Verify fleet state parsed
    let fleet = app.fleet_states.get("check_fleet").unwrap();
    assert_eq!(fleet.hosts.len(), 3);
    assert_eq!(fleet.selected_tab, 0);

    // Place cursor inside output block (line 9)
    app.editor.buffer.cursor_line = 9;

    // Simulate pressing ']' to switch to tab 1
    app.handle_key_event(crossterm::event::KeyEvent::from(crossterm::event::KeyCode::Char(']')));
    let fleet = app.fleet_states.get("check_fleet").unwrap();
    assert_eq!(fleet.selected_tab, 1);
    assert!(app.status_message.contains("Showing host 2/3 (10.0.0.2)") || app.status_message.contains("Host 2/3"));

    // Simulate pressing ']' to switch to tab 2
    app.handle_key_event(crossterm::event::KeyEvent::from(crossterm::event::KeyCode::Char(']')));
    let fleet = app.fleet_states.get("check_fleet").unwrap();
    assert_eq!(fleet.selected_tab, 2);

    // Simulate pressing '[' to switch back to tab 1
    app.handle_key_event(crossterm::event::KeyEvent::from(crossterm::event::KeyCode::Char('[')));
    let fleet = app.fleet_states.get("check_fleet").unwrap();
    assert_eq!(fleet.selected_tab, 1);
}

#[test]
fn test_mesh_ipv6_endpoint_parsing_regression() {
    use skat_os::mesh::SshEndpoint;

    // Bracketed IPv6 with port
    let ep1 = SshEndpoint::parse("[::1]:2222").unwrap();
    assert_eq!(ep1.host, "::1");
    assert_eq!(ep1.port, 2222);
    assert_eq!(ep1.user, None);

    // Bracketed IPv6 with user and default port
    let ep2 = SshEndpoint::parse("admin@[2001:db8::1]:22").unwrap();
    assert_eq!(ep2.host, "2001:db8::1");
    assert_eq!(ep2.port, 22);
    assert_eq!(ep2.user, Some("admin".to_string()));

    // Standard hostname without port
    let ep3 = SshEndpoint::parse("ssh://deployer@cluster.internal").unwrap();
    assert_eq!(ep3.host, "cluster.internal");
    assert_eq!(ep3.port, 22);
    assert_eq!(ep3.user, Some("deployer".to_string()));
}

#[test]
fn test_mesh_session_pool_evict_regression() {
    use skat_os::mesh::{SshEndpoint, SshSessionPool};

    let pool = SshSessionPool::new();
    let ep = SshEndpoint::parse("user@192.168.1.50:22").unwrap();

    // Evict on non-existent or stale connection should cleanly succeed without panics
    pool.evict(&ep, None);
}

