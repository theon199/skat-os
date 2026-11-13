use skat_os::app::{App, AppMode};
use skat_os::audit::{AuditRecorder, GENESIS_HASH};
use skat_os::collab::client::CollabClient;
use skat_os::collab::protocol::CollabMessage;
use skat_os::collab::server::CollabServer;
use skat_os::editor::buffer::TextBuffer;
use skat_os::form::{FormParamType, FormParser};
use skat_os::guard::{DangerLevel, StaticAnalyzer};
use skat_os::palette::PaletteAction;
use skat_os::runner::{CodeBlock, ExecutionResult, Runner};
use skat_os::vault::SecretVault;
use std::collections::HashMap;
use std::time::Duration;
use tempfile::tempdir;

#[test]
fn test_dangerous_command_heuristic_analysis() {
    // 1. Filesystem wipe
    let r1 = StaticAnalyzer::analyze("rm -rf /");
    assert!(r1.is_destructive);
    assert_eq!(r1.danger_level, DangerLevel::Critical);

    let r2 = StaticAnalyzer::analyze("rm -rf /*");
    assert!(r2.is_destructive);
    assert_eq!(r2.danger_level, DangerLevel::Critical);

    let r3 = StaticAnalyzer::analyze("rm -rf .");
    assert!(r3.is_destructive);

    // 2. Drive formatting and block device overwrite
    let r4 = StaticAnalyzer::analyze("mkfs.ext4 -F /dev/sda1");
    assert!(r4.is_destructive);
    assert_eq!(r4.danger_level, DangerLevel::Critical);

    let r5 = StaticAnalyzer::analyze("dd if=/dev/zero of=/dev/nvme0n1 bs=1M");
    assert!(r5.is_destructive);
    assert_eq!(r5.danger_level, DangerLevel::Critical);

    // 3. Database destructive actions
    let r6 = StaticAnalyzer::analyze("DROP DATABASE production;");
    assert!(r6.is_destructive);
    assert_eq!(r6.danger_level, DangerLevel::Critical);

    let r7 = StaticAnalyzer::analyze("DROP TABLE users;");
    assert!(r7.is_destructive);
    assert_eq!(r7.danger_level, DangerLevel::High);

    let r8 = StaticAnalyzer::analyze("TRUNCATE TABLE transactions;");
    assert!(r8.is_destructive);
    assert_eq!(r8.danger_level, DangerLevel::High);

    // 4. Firewall flush
    let r9 = StaticAnalyzer::analyze("iptables -F");
    assert!(r9.is_destructive);
    assert_eq!(r9.danger_level, DangerLevel::High);

    // 5. Kubernetes deletion
    let r10 = StaticAnalyzer::analyze("kubectl delete namespace production");
    assert!(r10.is_destructive);
    assert_eq!(r10.danger_level, DangerLevel::Critical);

    let r11 = StaticAnalyzer::analyze("kubectl delete ns default");
    assert!(r11.is_destructive);
    assert_eq!(r11.danger_level, DangerLevel::Critical);

    // 6. Safe command
    let safe = StaticAnalyzer::analyze("echo 'Hello World'\nkubectl get pods\ncat /etc/hosts");
    assert!(!safe.is_destructive);
    assert_eq!(safe.danger_level, DangerLevel::Normal);
}

#[test]
fn test_fenced_block_safety_attributes_parsing() {
    let doc = r#"
```bash confirm="Are you sure you want to restart production database cluster?"
systemctl restart postgresql
```

```bash danger=true blast_radius="critical"
rm -rf /tmp/stale_cache/*
```

```bash dry_run=true
terraform apply -auto-approve
```

```bash approvals=2 target="@prod_fleet"
kubectl rollout restart deployment/api
```
"#;

    let buf = TextBuffer::from_str(doc);
    let blocks = Runner::all_code_blocks(&buf);
    assert_eq!(blocks.len(), 4);

    // Block 0: Confirm prompt attribute
    assert_eq!(
        blocks[0].confirm.as_deref(),
        Some("Are you sure you want to restart production database cluster?")
    );
    assert!(blocks[0].requires_confirmation());

    // Block 1: Danger and critical blast radius
    assert!(blocks[1].danger);
    assert_eq!(blocks[1].blast_radius.as_deref(), Some("critical"));
    assert!(blocks[1].is_dangerous());
    assert!(blocks[1].requires_confirmation());

    // Block 2: Dry run
    assert!(blocks[2].dry_run);

    // Block 3: Multiplayer approvals consensus requirement
    assert_eq!(blocks[3].approvals, 2);
    assert_eq!(blocks[3].target.as_deref(), Some("@prod_fleet"));
}

#[test]
fn test_secret_vault_interpolation_and_redaction() {
    let mut vault = SecretVault::new();
    vault.insert("AWS_SECRET_KEY", "AKIAIOSFODNN7EXAMPLE");
    vault.insert("DB_PASSWORD", "SuperSecretP@ssword!");

    let script = r#"
export AWS_KEY="{{secret.AWS_SECRET_KEY}}"
export DB_PASS="{{secrets.DB_PASSWORD}}"
curl -u admin:$DB_PASS https://api.prod.internal/v1
"#;

    let interpolated = vault.interpolate_secrets(script);
    assert!(interpolated.contains("export AWS_KEY=\"AKIAIOSFODNN7EXAMPLE\""));
    assert!(interpolated.contains("export DB_PASS=\"SuperSecretP@ssword!\""));

    // Verify masking on output
    let output = "Connected with password SuperSecretP@ssword!, generated key AKIAIOSFODNN7EXAMPLE.";
    let masked = vault.mask(output);
    assert_eq!(masked, "Connected with password [REDACTED], generated key [REDACTED].");
    assert!(!masked.contains("SuperSecretP@ssword!"));
    assert!(!masked.contains("AKIAIOSFODNN7EXAMPLE"));
}

#[tokio::test]
async fn test_secret_stream_masking_zero_leak() {
    let temp_dir = tempdir().unwrap();

    let mut vault = SecretVault::with_workspace(temp_dir.path());
    vault.insert("TOKEN", "TopSecretToken987654321");

    let block = CodeBlock {
        id: "leak_test".to_string(),
        name: None,
        language: "bash".to_string(),
        code: "echo \"The secret is {{secret.TOKEN}}\"".to_string(),
        start_fence_line: 0,
        end_fence_line: 2,
        existing_output: None,
        format: skat_os::runner::BlockFormat::Text,
        target: None,
        proxy: None,
        parallel: false,
        confirm: None,
        danger: false,
        blast_radius: None,
        dry_run: false,
        approvals: 1,
    };

    let (chunk_tx, mut chunk_rx) = tokio::sync::mpsc::unbounded_channel::<String>();
    let (end_tx, end_rx) = tokio::sync::oneshot::channel::<ExecutionResult>();

    let mut envs = HashMap::new();
    envs.insert("TOKEN".to_string(), "TopSecretToken987654321".to_string());

    let _handle = Runner::execute_streaming_with_pool(
        &block,
        temp_dir.path(),
        &envs,
        &skat_os::mesh::SshSessionPool::new(),
        None,
        move |chunk| {
            let _ = chunk_tx.send(chunk);
        },
        move |res| {
            let _ = end_tx.send(res);
        },
    )
    .await
    .unwrap();

    let result = end_rx.await.unwrap();

    // The raw secret value MUST be masked as [REDACTED] in stdout
    assert!(result.stdout.contains("[REDACTED]"));
    assert!(!result.stdout.contains("TopSecretToken987654321"));

    // Verify all stream chunks never leaked the raw secret
    let mut all_chunks = Vec::new();
    while let Ok(chunk) = chunk_rx.try_recv() {
        assert!(!chunk.contains("TopSecretToken987654321"));
        all_chunks.push(chunk);
    }
}

#[test]
fn test_secret_form_parameter_masking() {
    let line = "API Token: {{param:secret:API_KEY=my_secret_token_123}}";
    let params = FormParser::parse_line(0, line);
    assert_eq!(params.len(), 1);

    let p = &params[0];
    assert_eq!(p.param_type, FormParamType::Secret);
    assert_eq!(p.name, "API_KEY");
    assert_eq!(p.value, "my_secret_token_123");
    // Display value must be masked with bullet points
    assert_eq!(p.display_value(), "•••••••••••••••••••");

    // Updating secret parameter in buffer
    let mut buffer = TextBuffer::from_str(line);
    FormParser::update_param_in_buffer(&mut buffer, "API_KEY", "new_secret_key");
    assert!(buffer.get_text().contains("{{param:secret:API_KEY=new_secret_key}}"));
}

#[test]
fn test_frontmatter_encrypted_secrets_chacha20poly1305() {
    let mut secrets = HashMap::new();
    secrets.insert("PROD_DATABASE_KEY".to_string(), "CorrectHorseBatteryStaple".to_string());
    secrets.insert("STRIPE_SECRET".to_string(), "sk_live_998877665544".to_string());

    let passphrase = "my-secure-cluster-passphrase";
    let (nonce_hex, cipher_hex) = SecretVault::encrypt_secrets_payload(&secrets, passphrase).unwrap();

    let decrypted = SecretVault::decrypt_secrets_payload(&nonce_hex, &cipher_hex, passphrase).unwrap();
    assert_eq!(decrypted.get("PROD_DATABASE_KEY"), Some(&"CorrectHorseBatteryStaple".to_string()));
    assert_eq!(decrypted.get("STRIPE_SECRET"), Some(&"sk_live_998877665544".to_string()));

    // Invalid passphrase must fail authentication
    assert!(SecretVault::decrypt_secrets_payload(&nonce_hex, &cipher_hex, "invalid-passphrase").is_err());
}

#[test]
fn test_immutable_audit_log_recording_and_hash_chaining() {
    let temp_dir = tempdir().unwrap();
    let mut recorder = AuditRecorder::new(temp_dir.path()).unwrap();

    let mut params = HashMap::new();
    params.insert("CLUSTER".to_string(), "prod-us-east".to_string());

    let entry1 = recorder
        .record(
            "runbook.md",
            "step_1",
            "Alice",
            vec!["10.0.0.1".to_string()],
            "systemctl restart db",
            params.clone(),
            0,
            250,
            "Service restarted successfully",
        )
        .unwrap();

    assert_eq!(entry1.prev_hash, GENESIS_HASH);
    assert!(!entry1.hash.is_empty());

    let entry2 = recorder
        .record(
            "runbook.md",
            "step_2",
            "Bob",
            vec!["@web_servers".to_string()],
            "kubectl rollout status deploy/web",
            params,
            0,
            120,
            "deployment successfully rolled out",
        )
        .unwrap();

    // Hash chaining verification
    assert_eq!(entry2.prev_hash, entry1.hash);

    // Verify whole file integrity
    assert!(recorder.verify_integrity().unwrap());

    // Load and inspect entries
    let entries = recorder.load_entries().unwrap();
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].block_id, "step_1");
    assert_eq!(entries[1].block_id, "step_2");
    assert_eq!(entries[0].collaborator, "Alice");
    assert_eq!(entries[1].collaborator, "Bob");

    // Tampering test: modifying a byte in audit.log must fail integrity check
    let content = std::fs::read_to_string(&recorder.log_path).unwrap();
    let tampered = content.replace("systemctl restart db", "systemctl stop db");
    std::fs::write(&recorder.log_path, tampered).unwrap();

    assert!(!recorder.verify_integrity().unwrap());
}

#[tokio::test]
async fn test_multi_peer_approval_workflow_over_tcp() {
    let server_handle = CollabServer::bind(
        "127.0.0.1:0",
        "# Runbook\n\n```bash approvals=2\nkubectl delete namespace staging\n```\n".to_string(),
        "runbook.md".to_string(),
    )
    .await
    .unwrap();

    let addr = format!("127.0.0.1:{}", server_handle.local_addr.port());

    // Alice connects
    let alice = CollabClient::connect(&addr, "Alice".to_string()).await.unwrap();
    // Bob connects
    let mut bob = CollabClient::connect(&addr, "Bob".to_string()).await.unwrap();

    tokio::time::sleep(Duration::from_millis(50)).await;

    // Alice requests approval for block_0 requiring 2 approvals
    alice.send_approval_request("block_0".to_string(), "kubectl delete namespace staging".to_string(), 2);

    // Bob receives the ApprovalRequest and ApprovalStatus (1/2 from Alice)
    let mut bob_saw_request = false;
    let mut bob_saw_status_1 = false;

    let deadline = tokio::time::Instant::now() + Duration::from_millis(500);
    while tokio::time::Instant::now() < deadline {
        if let Some(msg) = bob.try_recv() {
            match msg {
                CollabMessage::ApprovalRequest { block_id, required_approvals, .. } => {
                    assert_eq!(block_id, "block_0");
                    assert_eq!(required_approvals, 2);
                    bob_saw_request = true;
                }
                CollabMessage::ApprovalStatus { block_id, approvals_count, required_approvals, approved } => {
                    assert_eq!(block_id, "block_0");
                    assert_eq!(required_approvals, 2);
                    if approvals_count == 1 {
                        assert!(!approved);
                        bob_saw_status_1 = true;
                    }
                }
                _ => {}
            }
        }
        if bob_saw_request && bob_saw_status_1 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    assert!(bob_saw_request, "Bob should receive ApprovalRequest from Alice");
    assert!(bob_saw_status_1, "Bob should receive initial ApprovalStatus (1/2)");

    // Bob casts approval vote
    bob.send_approval_vote("block_0".to_string());

    // Both peers receive ApprovalStatus with approved = true
    let mut approved_consensus = false;
    let deadline = tokio::time::Instant::now() + Duration::from_millis(500);
    while tokio::time::Instant::now() < deadline {
        while let Some(msg) = bob.try_recv() {
            if let CollabMessage::ApprovalStatus { block_id, approvals_count, approved, .. } = msg
                && block_id == "block_0" && approvals_count == 2 && approved
            {
                approved_consensus = true;
                break;
            }
        }
        if approved_consensus {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    assert!(approved_consensus, "Multiplayer consensus should be reached (2/2 approvals)");
    server_handle.stop();
}

#[tokio::test]
async fn test_dry_run_environment_injection() {
    let temp_dir = tempdir().unwrap();

    let block = CodeBlock {
        id: "dry_run_test".to_string(),
        name: None,
        language: "bash".to_string(),
        code: "echo \"DRY_RUN=$SKAT_DRY_RUN\"".to_string(),
        start_fence_line: 0,
        end_fence_line: 2,
        existing_output: None,
        format: skat_os::runner::BlockFormat::Text,
        target: None,
        proxy: None,
        parallel: false,
        confirm: None,
        danger: false,
        blast_radius: None,
        dry_run: true,
        approvals: 1,
    };

    let result = Runner::execute_streaming_with_pool(
        &block,
        temp_dir.path(),
        &HashMap::new(),
        &skat_os::mesh::SshSessionPool::new(),
        None,
        |_chunk| {},
        |_res| {},
    )
    .await;

    assert!(result.is_ok());

    let (res_tx, res_rx) = tokio::sync::oneshot::channel();
    let _handle = Runner::execute_streaming_with_pool(
        &block,
        temp_dir.path(),
        &HashMap::new(),
        &skat_os::mesh::SshSessionPool::new(),
        None,
        |_chunk| {},
        move |res| {
            let _ = res_tx.send(res);
        },
    )
    .await
    .unwrap();

    let res = res_rx.await.unwrap();
    assert_eq!(res.exit_code, 0);
    assert!(res.stdout.contains("DRY_RUN=1"));
}

#[tokio::test]
async fn test_command_palette_audit_log_action() {
    let temp_dir = tempdir().unwrap();
    let mut app = App::new(temp_dir.path().to_path_buf()).unwrap();

    // Trigger Command Palette
    app.mode = AppMode::CommandPalette;
    app.palette.reset(&app.workspace, app.editor.file_path.as_ref());

    // Search for "audit"
    for c in "audit".chars() {
        app.palette.insert_char(c, &app.workspace, app.editor.file_path.as_ref());
    }

    let selected = app.palette.selected_item().unwrap();
    assert!(selected.label.contains("Audit Log"));
    assert!(matches!(selected.action, PaletteAction::ViewAuditLog));

    // Execute palette action
    app.execute_palette_action(selected.action.clone());

    // App mode must switch to AuditLogModal
    assert!(matches!(app.mode, AppMode::AuditLogModal { .. }));
}

#[test]
fn test_guard_enhanced_destruction_detection_regression() {
    use skat_os::guard::{ApprovalConsensus, DangerLevel, StaticAnalyzer};

    // 1. Reversed flags: rm -fr /
    let res1 = StaticAnalyzer::analyze("rm -fr /");
    assert!(res1.is_destructive);
    assert_eq!(res1.danger_level, DangerLevel::Critical);

    // 2. rm with --no-preserve-root
    let res2 = StaticAnalyzer::analyze("rm -rf --no-preserve-root /var/log");
    assert!(res2.is_destructive);
    assert_eq!(res2.danger_level, DangerLevel::Critical);

    // 3. Chmod uppercase and lowercase recursive
    let res3 = StaticAnalyzer::analyze("chmod -R 777 /");
    assert!(res3.is_destructive);
    let res4 = StaticAnalyzer::analyze("chmod -r 777 /");
    assert!(res4.is_destructive);

    // 4. SQL drop schema
    let res5 = StaticAnalyzer::analyze("DROP SCHEMA IF EXISTS production CASCADE;");
    assert!(res5.is_destructive);

    // 5. Consensus helper methods
    let mut consensus = ApprovalConsensus::new("b1".to_string(), "reboot".to_string(), 2);
    assert!(!consensus.has_approved("Alice"));
    // First approval (1 of 2): consensus not met yet
    assert!(!consensus.add_approval("Alice".to_string()));
    assert!(consensus.has_approved("Alice"));
    assert_eq!(consensus.current_count(), 1);
    assert!(!consensus.is_approved());

    // Second approval (2 of 2): consensus met!
    assert!(consensus.add_approval("Bob".to_string()));
    assert!(consensus.has_approved("Bob"));
    assert_eq!(consensus.current_count(), 2);
    assert!(consensus.is_approved());

    // remove approval: consensus no longer held
    assert!(!consensus.remove_approval("Alice"));
    assert!(!consensus.has_approved("Alice"));
    assert_eq!(consensus.current_count(), 1);
    assert!(!consensus.is_approved());
}

#[test]
fn test_vault_zero_leak_substring_masking_regression() {
    use skat_os::vault::SecretVault;

    let mut vault = SecretVault::new();
    vault.register_sensitive_value("secret");
    vault.register_sensitive_value("secret_super_token_999");

    let log = "Connecting with token secret_super_token_999 and basic secret here.";
    let masked = vault.mask(log);

    // Longest substring must be masked first:
    // Should be: "Connecting with token [REDACTED] and basic [REDACTED] here."
    // NOT: "Connecting with token [REDACTED]_super_token_999..."
    assert_eq!(
        masked,
        "Connecting with token [REDACTED] and basic [REDACTED] here."
    );
    assert!(!masked.contains("_super_token_999"));
}

#[test]
fn test_audit_large_output_hash_chain_consistency_regression() {
    use skat_os::audit::AuditRecorder;
    use std::collections::HashMap;

    let temp_dir = tempdir().unwrap();
    let mut audit = AuditRecorder::new(temp_dir.path()).unwrap();

    // Generate output far exceeding 500 characters
    let large_output = "SUCCESS-DATA-CHUNK-".repeat(100); // 1900 chars
    assert!(large_output.len() > 500);

    audit
        .record(
            "runbook.md",
            "block_0",
            "Alice",
            vec!["host1.local".to_string()],
            "cat /var/log/huge.log",
            HashMap::new(),
            0,
            120,
            &large_output,
        )
        .unwrap();

    audit
        .record(
            "runbook.md",
            "block_1",
            "Bob",
            vec!["host2.local".to_string()],
            "cat /var/log/another.log",
            HashMap::new(),
            0,
            95,
            &large_output,
        )
        .unwrap();

    let is_valid = audit.verify_integrity().unwrap();
    assert!(is_valid, "Audit hash chain must remain valid for >500 character outputs");

    let entries = audit.load_entries().unwrap();
    assert_eq!(entries.len(), 2);
}

