use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use skat_os::app::{App, AppMode};
use skat_os::editor::buffer::TextBuffer;
use skat_os::palette::CommandPalette;
use skat_os::runner::Runner;
use skat_os::workspace::Workspace;

#[test]
fn test_pty_sanity() {
    let pty_system = portable_pty::native_pty_system();
    let pair = pty_system
        .openpty(portable_pty::PtySize::default())
        .unwrap();
    let mut cmd = portable_pty::CommandBuilder::new("echo");
    cmd.args(["hello"]);
    let mut child = pair.slave.spawn_command(cmd).unwrap();
    drop(pair.slave);
    let mut reader = pair.master.try_clone_reader().unwrap();
    drop(pair.master);
    let mut output = String::new();
    let _ = std::io::Read::read_to_string(&mut reader, &mut output);
    let status = child.wait().unwrap();
    assert!(status.success());
    assert_eq!(status.exit_code(), 0);
    assert!(output.contains("hello"));
}

#[test]
fn test_pty_interactive_stdin() {
    let pty_system = portable_pty::native_pty_system();
    let pair = pty_system
        .openpty(portable_pty::PtySize::default())
        .unwrap();
    let mut cmd = portable_pty::CommandBuilder::new("bash");
    cmd.args(["-c", "read -p 'prompt:' ans; echo \"got: $ans\""]);
    let mut child = pair.slave.spawn_command(cmd).unwrap();
    drop(pair.slave);

    let mut writer = pair.master.take_writer().unwrap();
    let mut reader = pair.master.try_clone_reader().unwrap();
    drop(pair.master);

    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(50));
        let _ = std::io::Write::write_all(&mut writer, b"world\n");
        let _ = std::io::Write::flush(&mut writer);
    });

    let mut output = String::new();
    let _ = std::io::Read::read_to_string(&mut reader, &mut output);
    let status = child.wait().unwrap();
    assert!(status.success());
    assert!(output.contains("got: world"));
}

#[test]
fn test_pty_interruption() {
    let pty_system = portable_pty::native_pty_system();
    let pair = pty_system
        .openpty(portable_pty::PtySize::default())
        .unwrap();
    let mut cmd = portable_pty::CommandBuilder::new("bash");
    cmd.args(["-c", "sleep 10"]);
    let mut child = pair.slave.spawn_command(cmd).unwrap();
    drop(pair.slave);
    drop(pair.master);

    std::thread::sleep(std::time::Duration::from_millis(50));
    child.kill().unwrap();
    let _status = child.wait().unwrap();
}

#[test]
fn test_buffer_basic_typing_and_undo() {
    let mut buf = TextBuffer::new();
    buf.insert_str("Hello");
    assert_eq!(buf.get_text(), "Hello");
    assert_eq!(buf.cursor_col, 5);

    buf.insert_newline();
    buf.insert_str("World");
    assert_eq!(buf.get_text(), "Hello\nWorld");
    assert_eq!(buf.cursor_line, 1);
    assert_eq!(buf.cursor_col, 5);

    // Test backspace
    buf.backspace();
    assert_eq!(buf.get_text(), "Hello\nWorl");

    // Test undo
    assert!(buf.undo());
    assert_eq!(buf.get_text(), "Hello\nWorld");

    // Test redo
    assert!(buf.redo());
    assert_eq!(buf.get_text(), "Hello\nWorl");
}

#[test]
fn test_buffer_word_navigation_and_indentation() {
    let mut buf = TextBuffer::new();
    buf.insert_str("    let x = 100;");
    buf.insert_newline();
    // Verify auto-indent preserved 4 spaces
    assert_eq!(buf.line_to_string(1), "    ");
    assert_eq!(buf.cursor_col, 4);

    buf.move_up();
    buf.move_end();
    buf.move_word_left(); // skips ";" then stops at start of "100"
    assert_eq!(buf.cursor_col, 12);
}

#[test]
fn test_code_block_detection() {
    let doc = r#"# Notes

Here is a script:
```bash
echo "Hello from skat-os"
```

End of document.
"#;
    let buf = TextBuffer::from_str(doc);

    // Cursor on line 4 (inside ```bash ... ```)
    let block = Runner::find_code_block(&buf, 4);
    assert!(block.is_some());
    let b = block.unwrap();
    assert_eq!(b.language, "bash");
    assert_eq!(b.code, "echo \"Hello from skat-os\"");
    assert_eq!(b.start_fence_line, 3);
    assert_eq!(b.end_fence_line, 5);
}

#[tokio::test]
async fn test_runner_execution_and_output_injection() {
    let doc = "# Demo\n\n```bash\necho \"42\"\n```\n";
    let mut buf = TextBuffer::from_str(doc);

    let block = Runner::find_code_block(&buf, 3).expect("Code block should be found");
    let temp_dir = tempfile::tempdir().unwrap();

    let result = Runner::execute(&block, temp_dir.path())
        .await
        .expect("Execution should succeed");
    assert_eq!(result.exit_code, 0);
    assert!(result.stdout.contains("42"));

    Runner::apply_output(&mut buf, &block, &result);
    let updated_text = buf.get_text();
    assert!(updated_text.contains("```output [exit: 0]"));
    assert!(updated_text.contains("42"));
}

#[test]
fn test_toggle_output_collapse() {
    let doc = "# Output Test\n\n```output [exit: 0]\nhello\n```\n";
    let mut buf = TextBuffer::from_str(doc);

    // Cursor on line 2 (header of output block)
    assert!(Runner::toggle_output_collapse(&mut buf, 2));
    assert!(buf.get_text().contains("```output [exit: 0] [collapsed]"));

    // Toggle again to expand
    assert!(Runner::toggle_output_collapse(&mut buf, 2));
    assert!(!buf.get_text().contains("[collapsed]"));
}

#[test]
fn test_workspace_seed_and_backlinks() {
    let dir = tempfile::tempdir().unwrap();
    let ws = Workspace::new(dir.path().to_path_buf()).unwrap();

    // Verify seed documents were created
    assert!(ws.docs.len() >= 3);
    assert!(ws.docs.iter().any(|d| d.relative_path == "welcome.md"));
    assert!(ws.docs.iter().any(|d| d.relative_path == "architecture.md"));
    assert!(ws.docs.iter().any(|d| d.relative_path == "runbook.md"));

    // Verify backlinks
    // welcome.md links to [[architecture]] and [[runbook]]
    let arch_backlinks = ws.get_backlinks_for(&dir.path().join("architecture.md"));
    assert!(arch_backlinks.iter().any(|l| l.contains("welcome.md")));
}

#[test]
fn test_workspace_search_content() {
    let dir = tempfile::tempdir().unwrap();
    let ws = Workspace::new(dir.path().to_path_buf()).unwrap();

    let matches = ws.search_content("Document Operating System");
    assert!(!matches.is_empty());
    assert!(matches.iter().any(|m| m.relative_path == "welcome.md"));
}

#[test]
fn test_command_palette_fuzzy_matching() {
    let dir = tempfile::tempdir().unwrap();
    let ws = Workspace::new(dir.path().to_path_buf()).unwrap();

    let mut palette = CommandPalette::new();
    palette.reset(&ws, None);
    assert!(!palette.items.is_empty());

    // Search for "runbook"
    palette.insert_char('r', &ws, None);
    palette.insert_char('u', &ws, None);
    palette.insert_char('n', &ws, None);

    let top_item = palette.selected_item().unwrap();
    assert!(
        top_item.label.to_lowercase().contains("runbook") || top_item.subtext.contains("runbook")
    );
}

#[test]
fn test_task_checklist_toggle_and_stats() {
    let doc = r#"# Tasks
- [ ] Task 1
* [x] Task 2
- [ ] Task 3
Normal paragraph text
"#;
    let mut buf = TextBuffer::from_str(doc);

    // Initial stats: 1 completed out of 3 total
    assert_eq!(buf.task_stats(), Some((1, 3)));

    // Line 1 is "- [ ] Task 1", toggle to checked
    assert_eq!(buf.toggle_task_at_line(1), Some(true));
    assert_eq!(buf.line_to_string(1), "- [x] Task 1");
    assert_eq!(buf.task_stats(), Some((2, 3)));

    // Toggle back to unchecked
    assert_eq!(buf.toggle_task_at_line(1), Some(false));
    assert_eq!(buf.line_to_string(1), "- [ ] Task 1");
    assert_eq!(buf.task_stats(), Some((1, 3)));

    // Undo should restore checked state
    assert!(buf.undo());
    assert_eq!(buf.line_to_string(1), "- [x] Task 1");
    assert_eq!(buf.task_stats(), Some((2, 3)));

    // Test cursor checklist check
    buf.cursor_line = 2; // "* [x] Task 2"
    buf.cursor_col = 0;
    assert!(buf.is_cursor_on_task_item());
    buf.cursor_col = 3;
    assert!(buf.is_cursor_on_task_item());

    // Line 4 is normal paragraph: not a task item
    buf.cursor_line = 4;
    assert!(!buf.is_cursor_on_task_item());
    assert_eq!(buf.toggle_task_at_line(4), None);
}

#[test]
fn test_named_code_block_parsing() {
    let (lang1, name1) = Runner::parse_fence_header("bash name=get_pod");
    assert_eq!(lang1, "bash");
    assert_eq!(name1, Some("get_pod".to_string()));

    let (lang2, name2) = Runner::parse_fence_header("python name=\"my_script\"");
    assert_eq!(lang2, "python");
    assert_eq!(name2, Some("my_script".to_string()));

    let (lang3, name3) = Runner::parse_fence_header("name=step1 zsh");
    assert_eq!(lang3, "zsh");
    assert_eq!(name3, Some("step1".to_string()));

    let doc = r#"# Playbook
```bash name=get_pod
kubectl get pods
```
```python
print("unnamed")
```
"#;
    let buf = TextBuffer::from_str(doc);
    let blocks = Runner::all_code_blocks(&buf);
    assert_eq!(blocks.len(), 2);

    assert_eq!(blocks[0].id, "get_pod");
    assert_eq!(blocks[0].name, Some("get_pod".to_string()));
    assert_eq!(blocks[0].language, "bash");

    assert_eq!(blocks[1].id, "block_1");
    assert_eq!(blocks[1].name, None);
    assert_eq!(blocks[1].language, "python");

    // Lookup by custom name
    let found = Runner::find_code_block_by_id(&buf, "get_pod");
    assert!(found.is_some());
    assert_eq!(found.unwrap().name, Some("get_pod".to_string()));

    // Lookup by block index
    let found_by_idx = Runner::find_code_block_by_id(&buf, "block_0");
    assert!(found_by_idx.is_some());
    assert_eq!(found_by_idx.unwrap().id, "get_pod");
}

#[test]
fn test_variable_interpolation_and_env_vars() {
    use std::collections::HashMap;

    let mut outputs = HashMap::new();
    outputs.insert(
        "get_pod".to_string(),
        skat_os::runner::ExecutionResult {
            stdout: "web-pod-xyz-123\n\n".to_string(),
            stderr: String::new(),
            exit_code: 0,
            timed_out: false,
        },
    );
    outputs.insert(
        "block_0".to_string(),
        skat_os::runner::ExecutionResult {
            stdout: "web-pod-xyz-123\n\n".to_string(),
            stderr: String::new(),
            exit_code: 0,
            timed_out: false,
        },
    );

    // 1. Template interpolation with trailing newline stripping
    let code1 = "kubectl logs {{get_pod.stdout}} --tail=50";
    let interpolated1 = Runner::interpolate_variables(code1, &outputs);
    assert_eq!(interpolated1, "kubectl logs web-pod-xyz-123 --tail=50");

    // 2. Block index syntax
    let code2 = "curl http://{{block_0.stdout}}:8080";
    let interpolated2 = Runner::interpolate_variables(code2, &outputs);
    assert_eq!(interpolated2, "curl http://web-pod-xyz-123:8080");

    // 3. Exit code interpolation
    let code3 = "echo 'Exit code was {{get_pod.exit_code}}'";
    let interpolated3 = Runner::interpolate_variables(code3, &outputs);
    assert_eq!(interpolated3, "echo 'Exit code was 0'");

    // 4. Auto-injected environment variables
    let env_vars = Runner::build_environment_variables(&outputs);
    assert_eq!(
        env_vars.get("SKAT_GET_POD_STDOUT").map(|s| s.as_str()),
        Some("web-pod-xyz-123")
    );
    assert_eq!(
        env_vars.get("SKAT_BLOCK_0_STDOUT").map(|s| s.as_str()),
        Some("web-pod-xyz-123")
    );
    assert_eq!(
        env_vars.get("SKAT_GET_POD_EXIT_CODE").map(|s| s.as_str()),
        Some("0")
    );
}

#[test]
fn test_block_dependency_resolution_and_topological_sort() {
    let doc = r#"# Pipeline
```bash name=step_a
echo "Base setup"
```
```bash name=step_b
echo "Configuring {{step_a.stdout}}"
```
```bash name=step_c
echo "Deploying {{step_b.stdout}}"
```
"#;
    let buf = TextBuffer::from_str(doc);
    let blocks = Runner::all_code_blocks(&buf);

    assert_eq!(
        Runner::extract_dependencies(&blocks[1].code),
        vec!["step_a"]
    );
    assert_eq!(
        Runner::extract_dependencies(&blocks[2].code),
        vec!["step_b"]
    );

    let deps = Runner::resolve_block_dependencies(&blocks).expect("Dependencies should resolve");
    assert_eq!(deps, vec![(0, vec![]), (1, vec![0]), (2, vec![1])]);

    let order = Runner::resolve_execution_order(&blocks).expect("Topological sort should succeed");
    assert_eq!(order, vec![0, 1, 2]);

    // Test circular dependency detection
    let cycle_doc = r#"
```bash name=cycle_x
echo "{{cycle_y.stdout}}"
```
```bash name=cycle_y
echo "{{cycle_x.stdout}}"
```
"#;
    let cycle_buf = TextBuffer::from_str(cycle_doc);
    let cycle_blocks = Runner::all_code_blocks(&cycle_buf);
    let cycle_result = Runner::resolve_execution_order(&cycle_blocks);
    assert!(cycle_result.is_err());
    assert!(cycle_result.unwrap_err().contains("Circular dependency"));

    // Test missing dependency error
    let missing_doc = r#"
```bash name=broken
echo "{{nonexistent_step.stdout}}"
```
"#;
    let missing_buf = TextBuffer::from_str(missing_doc);
    let missing_blocks = Runner::all_code_blocks(&missing_buf);
    let missing_result = Runner::resolve_block_dependencies(&missing_blocks);
    assert!(missing_result.is_err());
    assert!(missing_result.unwrap_err().contains("unknown block"));
}

#[tokio::test]
async fn test_playbook_sequential_execution_and_chaining() {
    let doc = r#"# Chained Playbook
```bash name=gen_token
echo "secret_token_123"
```
```bash name=use_token
echo "TOKEN={{gen_token.stdout}} ENV=$SKAT_GEN_TOKEN_STDOUT"
```
"#;
    let buf = TextBuffer::from_str(doc);
    let blocks = Runner::all_code_blocks(&buf);
    let temp_dir = tempfile::tempdir().unwrap();

    let mut started_steps = Vec::new();
    let mut finished_steps = Vec::new();

    let report = Runner::execute_playbook(
        &blocks,
        temp_dir.path(),
        std::collections::HashMap::new(),
        |idx, block| {
            started_steps.push((idx, block.id.clone()));
        },
        |idx, block, res| {
            finished_steps.push((idx, block.id.clone(), res.stdout.trim().to_string()));
        },
    )
    .await
    .expect("Playbook execution should succeed");

    assert!(report.success);
    assert_eq!(report.executed_steps, 2);
    assert_eq!(report.total_steps, 2);
    assert_eq!(report.failed_step, None);

    assert_eq!(started_steps.len(), 2);
    assert_eq!(finished_steps.len(), 2);

    // Verify step 2 successfully interpolated step 1 output and env var
    assert_eq!(finished_steps[0].2, "secret_token_123");
    assert_eq!(
        finished_steps[1].2,
        "TOKEN=secret_token_123 ENV=secret_token_123"
    );
}

#[tokio::test]
async fn test_playbook_fail_fast_safeguard() {
    let doc = r#"# Fail-Fast Playbook
```bash name=step_1_ok
echo "Success 1"
```
```bash name=step_2_failing
echo "Failing here" >&2
exit 7
```
```bash name=step_3_unreached
echo "Should not run"
```
"#;
    let buf = TextBuffer::from_str(doc);
    let blocks = Runner::all_code_blocks(&buf);
    let temp_dir = tempfile::tempdir().unwrap();

    let mut executed_ids = Vec::new();

    let report = Runner::execute_playbook(
        &blocks,
        temp_dir.path(),
        std::collections::HashMap::new(),
        |_idx, block| {
            executed_ids.push(block.id.clone());
        },
        |_idx, _block, _res| {},
    )
    .await
    .expect("Execution should return report without panic");

    assert!(!report.success);
    assert_eq!(report.executed_steps, 2);
    assert_eq!(report.total_steps, 3);
    assert_eq!(report.failed_step, Some(1));
    assert_eq!(report.failed_block_id, Some("step_2_failing".to_string()));
    assert!(
        report
            .error_message
            .unwrap()
            .contains("failed with exit code 7")
    );

    // Step 3 was never executed
    assert_eq!(executed_ids, vec!["step_1_ok", "step_2_failing"]);
}

#[test]
fn test_app_interactive_space_toggle_and_shortcuts() {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    let temp_dir = tempfile::tempdir().unwrap();
    let mut app = skat_os::app::App::new(temp_dir.path().to_path_buf()).unwrap();

    let initial = "# Checklist\n- [ ] First task\nSome other text\n";
    app.editor.buffer = TextBuffer::from_str(initial);

    // Position cursor at line 1, col 0 (on "- [ ] First task")
    app.editor.buffer.cursor_line = 1;
    app.editor.buffer.cursor_col = 0;

    // Press Space
    app.handle_key_event(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE));
    assert_eq!(app.editor.buffer.line_to_string(1), "- [x] First task");
    assert_eq!(app.status_message, "Task marked completed");

    // Press Space again to toggle back
    app.handle_key_event(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE));
    assert_eq!(app.editor.buffer.line_to_string(1), "- [ ] First task");
    assert_eq!(app.status_message, "Task marked incomplete");

    // Position cursor on non-task line (line 2: "Some other text")
    app.editor.buffer.cursor_line = 2;
    app.editor.buffer.cursor_col = 4; // after "Some"
    app.handle_key_event(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE));
    // Should insert a space character as normal typing
    assert_eq!(app.editor.buffer.line_to_string(2), "Some  other text");
}

#[test]
fn test_ansi_escape_parsing_to_ratatui_spans() {
    use ratatui::style::{Color, Modifier, Style};
    use skat_os::ansi::{parse_ansi_spans, strip_ansi_codes};

    // 1. Standard colors & modifiers
    let sample = "\x1b[1;32mPASSED\x1b[0m \x1b[31mFAIL\x1b[0m \x1b[4;34mLINK\x1b[0m";
    let spans = parse_ansi_spans(sample);
    assert_eq!(spans.len(), 5);
    assert_eq!(spans[0].content, "PASSED");
    assert_eq!(
        spans[0].style,
        Style::default()
            .fg(Color::Green)
            .add_modifier(Modifier::BOLD)
    );
    assert_eq!(spans[1].content, " ");
    assert_eq!(spans[2].content, "FAIL");
    assert_eq!(spans[2].style, Style::default().fg(Color::Red));
    assert_eq!(spans[3].content, " ");
    assert_eq!(spans[4].content, "LINK");
    assert_eq!(
        spans[4].style,
        Style::default()
            .fg(Color::Blue)
            .add_modifier(Modifier::UNDERLINED)
    );

    // 2. 256 colors & 24-bit Truecolor
    let color_sample = "\x1b[38;5;82mIndexed82\x1b[0m \x1b[38;2;12;34;56mRGBText\x1b[0m";
    let color_spans = parse_ansi_spans(color_sample);
    assert_eq!(color_spans[0].content, "Indexed82");
    assert_eq!(
        color_spans[0].style,
        Style::default().fg(Color::Indexed(82))
    );
    assert_eq!(color_spans[2].content, "RGBText");
    assert_eq!(
        color_spans[2].style,
        Style::default().fg(Color::Rgb(12, 34, 56))
    );

    // 3. Strip ANSI codes
    assert_eq!(
        strip_ansi_codes("\x1b[1;32mTest\x1b[0m \x1b[33mWarning\x1b[0m"),
        "Test Warning"
    );
}

#[test]
fn test_incremental_stream_chunk_appending_and_buffer_management() {
    use skat_os::runner::{OutputStatus, Runner};

    let doc = "# Build Script\n\n```bash name=builder\ncargo build\n```\n\nFooter note\n";
    let mut buf = TextBuffer::from_str(doc);

    // 1. Stream starts: output block created as [running]
    assert!(Runner::apply_streaming_status(
        &mut buf,
        "builder",
        OutputStatus::Running,
        ""
    ));
    assert!(buf.get_text().contains("```output [running]\n```"));

    // 2. Incremental chunk 1: "Compiling core v0.1.0\n"
    let mut acc = String::new();
    assert!(Runner::append_stream_chunk(
        &mut buf,
        "builder",
        "Compiling core v0.1.0\n",
        &mut acc
    ));
    assert!(
        buf.get_text()
            .contains("```output [running]\nCompiling core v0.1.0\n```")
    );

    // 3. Incremental chunk 2: Progress bar with \r carriage return overwrite
    assert!(Runner::append_stream_chunk(
        &mut buf,
        "builder",
        "Progress: [==>  ] 30%",
        &mut acc
    ));
    assert!(buf.get_text().contains("Progress: [==>  ] 30%"));

    assert!(Runner::append_stream_chunk(
        &mut buf,
        "builder",
        "\rProgress: [====>] 75%",
        &mut acc
    ));
    // Verify \r overwrote the 30% line cleanly!
    assert!(buf.get_text().contains("Progress: [====>] 75%"));
    assert!(!buf.get_text().contains("30%"));

    // 4. Stream ends with exit code 0
    let processed = skat_os::ansi::process_terminal_stream(&acc);
    assert!(Runner::apply_streaming_status(
        &mut buf,
        "builder",
        OutputStatus::Exit(0),
        &processed
    ));
    let final_text = buf.get_text();
    assert!(final_text.contains("```output [exit: 0]"));
    assert!(final_text.contains("Compiling core v0.1.0"));
    assert!(final_text.contains("Progress: [====>] 75%"));
    assert!(final_text.contains("Footer note"));
}

#[test]
fn test_stream_interruption_buffer_management() {
    use skat_os::runner::{OutputStatus, Runner};

    let doc = "```bash name=long_task\nsleep 100\n```\n";
    let mut buf = TextBuffer::from_str(doc);

    Runner::apply_streaming_status(&mut buf, "long_task", OutputStatus::Running, "Working...");
    assert!(
        buf.get_text()
            .contains("```output [running]\nWorking...\n```")
    );

    Runner::apply_streaming_status(
        &mut buf,
        "long_task",
        OutputStatus::Interrupted,
        "Working...\n[interrupted]",
    );
    let updated = buf.get_text();
    assert!(updated.contains("```output [interrupted]\nWorking...\n[interrupted]\n```"));

    // Verify find_following_output_block recognizes [interrupted] as exit code 130
    let block = Runner::find_code_block(&buf, 1).unwrap();
    let out = block.existing_output.unwrap();
    assert_eq!(out.exit_code, 130);
}

#[tokio::test]
async fn test_streaming_command_execution() {
    let doc =
        "```bash name=stream_echo\necho \"First Line\"\nsleep 0.05\necho \"Second Line\"\n```\n";
    let buf = TextBuffer::from_str(doc);
    let block = Runner::find_code_block(&buf, 1).unwrap();
    let temp_dir = tempfile::tempdir().unwrap();

    let chunks = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let chunks_clone = std::sync::Arc::clone(&chunks);

    let (res_tx, res_rx) = tokio::sync::oneshot::channel();
    let _handle = Runner::execute_streaming(
        &block,
        temp_dir.path(),
        &std::collections::HashMap::new(),
        move |chunk| {
            chunks_clone.lock().unwrap().push(chunk);
        },
        move |result| {
            let _ = res_tx.send(result);
        },
    )
    .await
    .unwrap();

    let result = res_rx.await.unwrap();
    assert_eq!(result.exit_code, 0);
    assert!(result.stdout.contains("First Line"));
    assert!(result.stdout.contains("Second Line"));

    let recorded_chunks = chunks.lock().unwrap().clone();
    assert!(!recorded_chunks.is_empty());
}

#[tokio::test]
async fn test_streaming_command_interruption() {
    let doc = "```bash name=sleeper\necho \"Started\"\nsleep 10\necho \"Finished\"\n```\n";
    let buf = TextBuffer::from_str(doc);
    let block = Runner::find_code_block(&buf, 1).unwrap();
    let temp_dir = tempfile::tempdir().unwrap();

    let (res_tx, res_rx) = tokio::sync::oneshot::channel();
    let mut handle = Runner::execute_streaming(
        &block,
        temp_dir.path(),
        &std::collections::HashMap::new(),
        |_chunk| {},
        move |result| {
            let _ = res_tx.send(result);
        },
    )
    .await
    .unwrap();

    // Allow process to start and print "Started"
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    // Trigger graceful cancellation (Ctrl+C equivalent)
    if let Some(cancel) = handle.cancel_tx.take() {
        let _ = cancel.send(());
    }

    let result = res_rx.await.unwrap();
    assert_eq!(result.exit_code, 130);
    assert!(result.stdout.contains("Started"));
    assert!(!result.stdout.contains("Finished"));
}

#[tokio::test]
async fn test_streaming_interactive_stdin_prompt() {
    let doc = "```bash name=prompter\nread -p \"Name: \" user\necho \"Hello $user!\"\n```\n";
    let buf = TextBuffer::from_str(doc);
    let block = Runner::find_code_block(&buf, 1).unwrap();
    let temp_dir = tempfile::tempdir().unwrap();

    let (res_tx, res_rx) = tokio::sync::oneshot::channel();
    let handle = Runner::execute_streaming(
        &block,
        temp_dir.path(),
        &std::collections::HashMap::new(),
        |_chunk| {},
        move |result| {
            let _ = res_tx.send(result);
        },
    )
    .await
    .unwrap();

    // Send input to prompt
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    let _ = handle.stdin_tx.send(b"SkatUser\n".to_vec());

    let result = res_rx.await.unwrap();
    assert_eq!(result.exit_code, 0);
    assert!(result.stdout.contains("Hello SkatUser!"));
}

#[tokio::test]
async fn test_app_ctrl_c_interruption_behavior() {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    let temp_dir = tempfile::tempdir().unwrap();
    let mut app = skat_os::app::App::new(temp_dir.path().to_path_buf()).unwrap();

    let doc = "# Runbook\n\n```bash name=infinite\nsleep 30\n```\n";
    app.editor.buffer = TextBuffer::from_str(doc);
    app.editor.buffer.cursor_line = 3;

    // Trigger execution of the sleep block
    app.trigger_run_block();
    assert!(app.is_running_block);
    assert!(app.running_command.is_some());

    // Allow task to spawn
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    // Press Ctrl+C: should cancel running command, mark [interrupted], and NOT quit app
    app.handle_key_event(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL));

    assert!(!app.is_running_block);
    assert!(app.running_command.is_none());
    assert!(!app.should_quit); // App must NOT quit on interrupt!
    assert!(app.status_message.contains("interrupted"));

    let buffer_text = app.editor.buffer.get_text();
    assert!(buffer_text.contains("```output [interrupted]"));

    // Press Ctrl+C when no command is running: now app should quit
    app.handle_key_event(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL));
    assert!(app.should_quit);
}

#[test]
fn test_form_parameter_parsing_and_find_navigation() {
    use skat_os::form::{FormParamType, FormParser};

    let doc = "# Cluster Runbook\n\nTarget: {{param:select:CLUSTER=prod-us-east,prod-eu-west,staging}}\nNamespace: {{param:input:NAMESPACE=kube-system}}\nDry run: {{param:toggle:DRY_RUN=true}}\n";
    let buf = TextBuffer::from_str(doc);

    let all = FormParser::find_all(&buf);
    assert_eq!(all.len(), 3);

    // Verify CLUSTER select
    assert_eq!(all[0].name, "CLUSTER");
    assert_eq!(all[0].value, "prod-us-east");
    match &all[0].param_type {
        FormParamType::Select { options } => {
            assert_eq!(options, &vec!["prod-us-east", "prod-eu-west", "staging"]);
        }
        _ => panic!("Expected Select parameter"),
    }

    // Verify NAMESPACE input
    assert_eq!(all[1].name, "NAMESPACE");
    assert_eq!(all[1].value, "kube-system");
    assert_eq!(all[1].param_type, FormParamType::Input);

    // Verify DRY_RUN toggle
    assert_eq!(all[2].name, "DRY_RUN");
    assert_eq!(all[2].value, "true");
    assert_eq!(all[2].param_type, FormParamType::Toggle);

    // Test find_at_cursor
    let at_cursor = FormParser::find_at_cursor(&buf, 2, 12);
    assert!(at_cursor.is_some());
    assert_eq!(at_cursor.unwrap().name, "CLUSTER");

    // Test find_next (tab navigation)
    let next1 = FormParser::find_next(&buf, 2, 10).unwrap();
    assert_eq!(next1.name, "NAMESPACE");

    let next2 = FormParser::find_next(&buf, 3, 15).unwrap();
    assert_eq!(next2.name, "DRY_RUN");

    // Wraparound to first parameter
    let wrap = FormParser::find_next(&buf, 4, 25).unwrap();
    assert_eq!(wrap.name, "CLUSTER");

    // Test find_prev (shift+tab navigation)
    let prev1 = FormParser::find_prev(&buf, 4, 20).unwrap();
    assert_eq!(prev1.name, "NAMESPACE");

    let prev_wrap = FormParser::find_prev(&buf, 1, 0).unwrap();
    assert_eq!(prev_wrap.name, "DRY_RUN");
}

#[test]
fn test_form_parameter_inline_document_toggling_and_cycling() {
    use skat_os::form::FormParser;

    let doc = "Select: {{param:select:CLUSTER=prod-us-east,prod-eu-west,staging}}\nToggle: {{param:toggle:DRY_RUN=true}}\nInput: {{param:input:TAG=v1.0.0}}\n";
    let mut buf = TextBuffer::from_str(doc);

    // 1. Cycle select
    buf.cursor_line = 0;
    buf.cursor_col = 15;
    let cycled = FormParser::toggle_or_cycle_at_cursor(&mut buf);
    assert_eq!(
        cycled,
        Some(("CLUSTER".to_string(), "prod-eu-west".to_string()))
    );
    assert!(
        buf.line_to_string(0)
            .contains("CLUSTER=prod-eu-west,staging,prod-us-east")
    );

    // Cycle select again
    let cycled2 = FormParser::toggle_or_cycle_at_cursor(&mut buf);
    assert_eq!(
        cycled2,
        Some(("CLUSTER".to_string(), "staging".to_string()))
    );
    assert!(
        buf.line_to_string(0)
            .contains("CLUSTER=staging,prod-us-east,prod-eu-west")
    );

    // 2. Toggle boolean
    buf.cursor_line = 1;
    buf.cursor_col = 15;
    let toggled = FormParser::toggle_or_cycle_at_cursor(&mut buf);
    assert_eq!(toggled, Some(("DRY_RUN".to_string(), "false".to_string())));
    assert!(buf.line_to_string(1).contains("DRY_RUN=false"));

    let toggled2 = FormParser::toggle_or_cycle_at_cursor(&mut buf);
    assert_eq!(toggled2, Some(("DRY_RUN".to_string(), "true".to_string())));
    assert!(buf.line_to_string(1).contains("DRY_RUN=true"));

    // 3. Update input
    buf.cursor_line = 2;
    buf.cursor_col = 15;
    let updated = FormParser::update_input_at_cursor(&mut buf, "v2.0.0");
    assert_eq!(updated, Some(("TAG".to_string(), "v2.0.0".to_string())));
    assert!(buf.line_to_string(2).contains("TAG=v2.0.0"));

    // 4. Verify collect_params returns updated active values
    let params = FormParser::collect_params(&buf);
    assert_eq!(params.get("CLUSTER"), Some(&"staging".to_string()));
    assert_eq!(params.get("DRY_RUN"), Some(&"true".to_string()));
    assert_eq!(params.get("TAG"), Some(&"v2.0.0".to_string()));
}

#[tokio::test]
async fn test_form_parameter_variable_interpolation_and_env_vars() {
    use std::collections::HashMap;

    let mut params = HashMap::new();
    params.insert("CLUSTER".to_string(), "prod-us-east".to_string());
    params.insert("NAMESPACE".to_string(), "billing".to_string());
    params.insert("DRY_RUN".to_string(), "true".to_string());

    let template = "kubectl --cluster={{params.CLUSTER}} -n {{params.NAMESPACE}} apply --dry-run={{params.DRY_RUN}}";
    let interpolated =
        Runner::interpolate_variables_with_params(template, &HashMap::new(), &params);
    assert_eq!(
        interpolated,
        "kubectl --cluster=prod-us-east -n billing apply --dry-run=true"
    );

    // Test environment variables injection
    let env_vars = Runner::build_environment_variables_with_params(&HashMap::new(), &params);
    assert_eq!(
        env_vars.get("SKAT_PARAM_CLUSTER"),
        Some(&"prod-us-east".to_string())
    );
    assert_eq!(
        env_vars.get("SKAT_PARAM_NAMESPACE"),
        Some(&"billing".to_string())
    );
    assert_eq!(
        env_vars.get("SKAT_PARAM_DRY_RUN"),
        Some(&"true".to_string())
    );

    // Test execution with environment variables
    let temp_dir = tempfile::tempdir().unwrap();
    let block = skat_os::runner::CodeBlock {
        id: "test_param_exec".to_string(),
        name: None,
        language: "bash".to_string(),
        code: "echo \"Cluster is $SKAT_PARAM_CLUSTER in $SKAT_PARAM_NAMESPACE\"".to_string(),
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

    let result = Runner::execute_with_env(&block, temp_dir.path(), &env_vars)
        .await
        .unwrap();
    assert_eq!(result.exit_code, 0);
    assert!(result.stdout.contains("Cluster is prod-us-east in billing"));
}

#[test]
fn test_structured_table_parsing_json_and_csv() {
    use skat_os::table::TableData;

    // Test JSON Array of Objects
    let json_data = r#"[
        {"id": "pod-1", "name": "nginx-ingress", "status": "Running", "restarts": "0"},
        {"id": "pod-2", "name": "auth-service", "status": "CrashLoopBackOff", "restarts": "5"},
        {"id": "pod-3", "name": "redis-cache", "status": "Running", "restarts": "1"}
    ]"#;

    let table = TableData::parse(json_data).expect("Failed to parse JSON table");
    assert_eq!(table.headers, vec!["id", "name", "status", "restarts"]);
    assert_eq!(table.rows.len(), 3);
    assert_eq!(table.selected_row, 0);
    assert_eq!(table.get_selected_cell("name"), Some("nginx-ingress"));
    assert_eq!(table.get_cell(1, "status"), Some("CrashLoopBackOff"));

    // Test CSV with headers
    let csv_data = "POD_ID,IMAGE,CPU\npod-101,nginx:1.21,50m\npod-102,redis:7.0,120m\n";
    let csv_table = TableData::parse(csv_data).expect("Failed to parse CSV table");
    assert_eq!(csv_table.headers, vec!["POD_ID", "IMAGE", "CPU"]);
    assert_eq!(csv_table.rows.len(), 2);
    assert_eq!(csv_table.rows[0], vec!["pod-101", "nginx:1.21", "50m"]);
    assert_eq!(csv_table.rows[1], vec!["pod-102", "redis:7.0", "120m"]);

    // Test row selection and row JSON export
    let mut nav_table = table;
    nav_table.select_next_row();
    assert_eq!(nav_table.selected_row, 1);
    assert_eq!(nav_table.get_selected_cell("name"), Some("auth-service"));

    let row_json = nav_table.get_selected_row_json().unwrap();
    assert!(row_json.contains("\"name\":\"auth-service\""));
    assert!(row_json.contains("\"status\":\"CrashLoopBackOff\""));
}

#[tokio::test]
async fn test_structured_table_variable_interpolation_and_env_vars() {
    use skat_os::table::TableData;
    use std::collections::HashMap;

    let json_data = r#"[
        {"id": "svc-web", "port": "8080", "protocol": "HTTP"},
        {"id": "svc-db", "port": "5432", "protocol": "TCP"}
    ]"#;

    let mut table = TableData::parse(json_data).unwrap();
    let mut tables = HashMap::new();
    tables.insert("get_services".to_string(), table.clone());

    // 1. Interpolation with row 0 selected (svc-web)
    let template = "curl http://localhost:{{get_services.selected.port}} # connecting to {{get_services.selected.id}}";
    let interpolated = Runner::interpolate_variables_with_context(
        template,
        &HashMap::new(),
        &HashMap::new(),
        &tables,
    );
    assert_eq!(
        interpolated,
        "curl http://localhost:8080 # connecting to svc-web"
    );

    // Verify environment variables
    let env_vars =
        Runner::build_environment_variables_with_context(&HashMap::new(), &HashMap::new(), &tables);
    assert_eq!(
        env_vars.get("SKAT_GET_SERVICES_SELECTED_PORT"),
        Some(&"8080".to_string())
    );
    assert_eq!(
        env_vars.get("SKAT_GET_SERVICES_SELECTED_ID"),
        Some(&"svc-web".to_string())
    );

    // 2. Select row 1 (svc-db) and verify dynamic reactive update
    table.select_next_row();
    tables.insert("get_services".to_string(), table);

    let interpolated2 = Runner::interpolate_variables_with_context(
        template,
        &HashMap::new(),
        &HashMap::new(),
        &tables,
    );
    assert_eq!(
        interpolated2,
        "curl http://localhost:5432 # connecting to svc-db"
    );

    let env_vars2 =
        Runner::build_environment_variables_with_context(&HashMap::new(), &HashMap::new(), &tables);
    assert_eq!(
        env_vars2.get("SKAT_GET_SERVICES_SELECTED_PORT"),
        Some(&"5432".to_string())
    );
    assert_eq!(
        env_vars2.get("SKAT_GET_SERVICES_SELECTED_ID"),
        Some(&"svc-db".to_string())
    );
}

#[test]
fn test_live_metric_chart_and_sparkline_parsing() {
    use skat_os::chart::ChartData;

    // Test streaming numeric series for Sparkline
    let stream_output = "15\n30\n45\n92\n64\n12\n";
    let chart = ChartData::parse(stream_output);
    assert_eq!(chart.values, vec![15, 30, 45, 92, 64, 12]);
    assert_eq!(chart.max_value, 92);
    assert!(!chart.is_empty());

    // Test labeled metric bars for BarChart
    let metrics = "cpu_usage: 42%\nmem_usage: 79%\ndisk_io: 15%\n";
    let bar_chart = ChartData::parse(metrics);
    assert_eq!(
        bar_chart.bars,
        vec![
            ("cpu_usage".to_string(), 42),
            ("mem_usage".to_string(), 79),
            ("disk_io".to_string(), 15),
        ]
    );
    assert_eq!(bar_chart.max_value, 79);
}

#[test]
fn test_app_interactive_form_tab_navigation_and_shortcuts() {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    let temp_dir = tempfile::tempdir().unwrap();
    let mut app = skat_os::app::App::new(temp_dir.path().to_path_buf()).unwrap();

    let doc = "# Deployment Runbook\n\nCluster: {{param:select:CLUSTER=prod-us-east,prod-eu-west}}\nDryRun: {{param:toggle:DRY_RUN=true}}\nName: {{param:input:APP=web}}\n";
    app.editor.buffer = TextBuffer::from_str(doc);
    app.refresh_params_and_tables();
    app.editor.buffer.cursor_line = 0;
    app.editor.buffer.cursor_col = 0;

    // Press Tab: should focus first parameter (CLUSTER)
    app.handle_key_event(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
    assert_eq!(app.editor.buffer.cursor_line, 2);
    assert!(app.status_message.contains("CLUSTER"));

    // Press Space: should cycle CLUSTER dropdown to prod-eu-west
    app.handle_key_event(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE));
    assert_eq!(app.params.get("CLUSTER"), Some(&"prod-eu-west".to_string()));
    assert!(
        app.editor
            .buffer
            .line_to_string(2)
            .contains("CLUSTER=prod-eu-west,prod-us-east")
    );

    // Press Tab again: should focus next parameter (DRY_RUN)
    app.handle_key_event(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
    assert_eq!(app.editor.buffer.cursor_line, 3);
    assert!(app.status_message.contains("DRY_RUN"));

    // Press Space: should toggle DRY_RUN from true to false
    app.handle_key_event(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE));
    assert_eq!(app.params.get("DRY_RUN"), Some(&"false".to_string()));
    assert!(
        app.editor
            .buffer
            .line_to_string(3)
            .contains("DRY_RUN=false")
    );

    // Press Tab again: should focus APP input
    app.handle_key_event(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
    assert_eq!(app.editor.buffer.cursor_line, 4);

    // Press Enter on input parameter: should enter ParamPrompt mode
    app.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert!(matches!(
        app.mode,
        skat_os::app::AppMode::ParamPrompt { .. }
    ));

    // Type new parameter value: "api" + Enter
    app.handle_key_event(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE));
    app.handle_key_event(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE));
    app.handle_key_event(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE));
    app.handle_key_event(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE));
    app.handle_key_event(KeyEvent::new(KeyCode::Char('p'), KeyModifiers::NONE));
    app.handle_key_event(KeyEvent::new(KeyCode::Char('i'), KeyModifiers::NONE));
    app.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

    assert_eq!(app.mode, skat_os::app::AppMode::Normal);
    assert_eq!(app.params.get("APP"), Some(&"api".to_string()));
    assert!(app.editor.buffer.line_to_string(4).contains("APP=api"));
}

#[tokio::test]
async fn test_playbook_with_parameters_and_table_chaining() {
    let temp_dir = tempfile::tempdir().unwrap();

    let doc = r#"# Playbook

Cluster: {{param:select:ENV=staging,production}}

```bash name=list_services format=table
echo '[{"service": "auth", "port": "3000"}, {"service": "gateway", "port": "8080"}]'
```

```bash name=test_service
echo "Testing service on port $SKAT_LIST_SERVICES_SELECTED_PORT in $SKAT_PARAM_ENV environment"
```
"#;

    let buf = TextBuffer::from_str(doc);
    let blocks = Runner::all_code_blocks(&buf);
    assert_eq!(blocks.len(), 2);
    assert_eq!(blocks[0].format, skat_os::runner::BlockFormat::Table);

    let mut params = std::collections::HashMap::new();
    params.insert("ENV".to_string(), "staging".to_string());

    let (step_start_tx, mut step_start_rx) = tokio::sync::mpsc::unbounded_channel();
    let (step_end_tx, mut step_end_rx) = tokio::sync::mpsc::unbounded_channel();

    let report = Runner::execute_playbook_with_context(
        &blocks,
        temp_dir.path(),
        std::collections::HashMap::new(),
        params,
        std::collections::HashMap::new(),
        move |idx, block| {
            let _ = step_start_tx.send((idx, block.id.clone(), block.code.clone()));
        },
        move |idx, block, res| {
            let _ = step_end_tx.send((idx, block.id.clone(), res.clone()));
        },
    )
    .await
    .unwrap();

    assert!(report.success);
    assert_eq!(report.executed_steps, 2);

    let _start0 = step_start_rx.recv().await.unwrap();
    let end0 = step_end_rx.recv().await.unwrap();
    assert_eq!(end0.1, "list_services");

    let start1 = step_start_rx.recv().await.unwrap();
    assert_eq!(start1.1, "test_service");

    let end1 = step_end_rx.recv().await.unwrap();
    assert_eq!(end1.1, "test_service");
    // Verify downstream block executed with chained table selected port and parameter ENV!
    assert!(
        end1.2
            .stdout
            .contains("Testing service on port 3000 in staging environment")
    );
}

#[test]
fn test_unsaved_changes_modal_on_ctrl_q() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = App::new(dir.path().to_path_buf()).unwrap();

    // Verify initially clean
    assert!(!app.editor.is_dirty());

    // Type a character to make buffer dirty
    app.handle_key_event(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE));
    assert!(app.editor.is_dirty());

    // Press Ctrl+Q: should trigger UnsavedChangesModal and NOT quit immediately
    app.handle_key_event(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::CONTROL));
    assert!(!app.should_quit);
    assert!(matches!(app.mode, AppMode::UnsavedChangesModal { .. }));

    // Press 'c' to Cancel
    app.handle_key_event(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::NONE));
    assert!(!app.should_quit);
    assert_eq!(app.mode, AppMode::Normal);
    assert!(app.editor.is_dirty());

    // Press Ctrl+Q again -> Cancel dismissed -> Press 'd' to Discard & Quit
    app.handle_key_event(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::CONTROL));
    assert!(matches!(app.mode, AppMode::UnsavedChangesModal { .. }));
    app.handle_key_event(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::NONE));
    assert!(app.should_quit);
}

#[test]
fn test_unsaved_changes_modal_on_document_switch() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = App::new(dir.path().to_path_buf()).unwrap();

    let doc2_path = app.workspace.create_document("note2.md").unwrap();

    // Modify current document
    app.handle_key_event(KeyEvent::new(KeyCode::Char('z'), KeyModifiers::NONE));
    assert!(app.editor.is_dirty());

    // Try to open doc2: should trigger UnsavedChangesModal
    app.open_document(&doc2_path);
    assert!(matches!(app.mode, AppMode::UnsavedChangesModal { .. }));

    // Press 's' to Save & Proceed
    app.handle_key_event(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::NONE));
    assert_eq!(app.mode, AppMode::Normal);
    assert_eq!(app.editor.file_path.as_ref(), Some(&doc2_path));
}

#[test]
fn test_expanded_seed_documents_and_backlinks() {
    let dir = tempfile::tempdir().unwrap();
    let ws = Workspace::new(dir.path().to_path_buf()).unwrap();

    assert!(ws.docs.len() >= 5);
    assert!(ws.docs.iter().any(|d| d.relative_path == "welcome.md"));
    assert!(ws.docs.iter().any(|d| d.relative_path == "daily.md"));
    assert!(ws.docs.iter().any(|d| d.relative_path == "runbook.md"));
    assert!(ws.docs.iter().any(|d| d.relative_path == "fleet.md"));
    assert!(ws.docs.iter().any(|d| d.relative_path == "architecture.md"));

    // Check backlinks from welcome to daily and fleet
    let daily_backlinks = ws.get_backlinks_for(&dir.path().join("daily.md"));
    assert!(
        daily_backlinks
            .iter()
            .any(|l| l.contains("welcome.md") || l.contains("architecture.md"))
    );
}

#[test]
fn test_workspace_path_expansion() {
    use skat_os::cli::expand_path;

    if let Ok(home) = std::env::var("HOME") {
        let expanded = expand_path("~/notes");
        assert_eq!(expanded, std::path::PathBuf::from(home).join("notes"));
    }
}

#[test]
fn test_extended_task_syntax_and_stats_regression() {
    let doc = "\
- [ ] dash task
* [ ] star task
+ [x] plus task
1. [ ] ordered dot task
2) [x] ordered paren task
\t- [ ] tab indented task
- [ ] 
";
    let mut buf = TextBuffer::from_str(doc);
    assert_eq!(buf.task_stats(), Some((2, 7)));

    // Toggle line 0 (- [ ]) -> (- [x])
    assert_eq!(buf.toggle_task_at_line(0), Some(true));
    assert_eq!(buf.line_to_string(0), "- [x] dash task");

    // Toggle line 2 (+ [x]) -> (+ [ ])
    assert_eq!(buf.toggle_task_at_line(2), Some(false));
    assert_eq!(buf.line_to_string(2), "+ [ ] plus task");

    // Toggle line 3 (1. [ ]) -> (1. [x])
    assert_eq!(buf.toggle_task_at_line(3), Some(true));
    assert_eq!(buf.line_to_string(3), "1. [x] ordered dot task");

    // Toggle line 4 (2) [x]) -> (2) [ ])
    assert_eq!(buf.toggle_task_at_line(4), Some(false));
    assert_eq!(buf.line_to_string(4), "2) [ ] ordered paren task");

    assert_eq!(buf.task_stats(), Some((2, 7)));
}

#[test]
fn test_chart_unit_support_and_clamped_metrics_regression() {
    use skat_os::chart::ChartData;

    // Test units: ms, MB, KB, %
    let stream_with_units = "\
latency: 125ms
memory: 512MB
throughput: 34KB
error_rate: 0%
loss: -10%
";
    let chart = ChartData::parse(stream_with_units);
    assert_eq!(chart.bars.len(), 5);
    assert_eq!(chart.bars[0], ("latency".to_string(), 125));
    assert_eq!(chart.bars[1], ("memory".to_string(), 512));
    assert_eq!(chart.bars[2], ("throughput".to_string(), 34));
    assert_eq!(chart.bars[3], ("error_rate".to_string(), 0));
    assert_eq!(chart.bars[4], ("loss".to_string(), 0)); // clamped negative value

    // Test structured JSON metric array
    let json_stream = r#"[{"label": "cpu", "value": 45}, {"name": "mem", "val": 88}]"#;
    let json_chart = ChartData::parse(json_stream);
    assert_eq!(json_chart.values, vec![45, 88]);
    assert_eq!(json_chart.max_value, 88);
}

#[test]
fn test_form_empty_parameter_values_regression() {
    use skat_os::form::FormParser;

    let line = "Configure tag: {{param:input:TAG=}} and env: {{param:select:ENV=prod|staging|dev}}";
    let params = FormParser::parse_line(0, line);
    assert_eq!(params.len(), 2);
    assert_eq!(params[0].name, "TAG");
    assert_eq!(params[0].value, "");

    let doc = "Tag is: {{param:input:TAG=}}\n";
    let mut buf = TextBuffer::from_str(doc);
    FormParser::update_param_in_buffer(&mut buf, "TAG", "v1.2.3");
    assert_eq!(buf.get_text(), "Tag is: {{param:input:TAG=v1.2.3}}\n");
}

#[test]
fn test_runner_variable_interpolation_colons_and_dots_regression() {
    use std::collections::HashMap;

    let mut params = HashMap::new();
    params.insert("REGION".to_string(), "us-east-1".to_string());
    params.insert("HOST".to_string(), "prod.server".to_string());

    let template =
        "curl http://{{param.HOST}}:8080/deploy?region={{param:REGION}}&r2={{params.REGION}}";
    let interpolated = Runner::interpolate_variables_with_context(
        template,
        &HashMap::new(),
        &params,
        &HashMap::new(),
    );
    assert_eq!(
        interpolated,
        "curl http://prod.server:8080/deploy?region=us-east-1&r2=us-east-1"
    );
}

#[test]
fn test_table_array_of_arrays_bounds_safety_regression() {
    use skat_os::table::TableData;

    // Ragged rows
    let json_ragged = r#"[
        ["col1", "col2", "col3"],
        ["val1"],
        ["valA", "valB", "valC", "valD"]
    ]"#;
    let table = TableData::parse(json_ragged).expect("Should parse ragged array of arrays");
    assert_eq!(table.headers, vec!["col1", "col2", "col3"]);
    assert_eq!(table.rows.len(), 2);
    // Row 0 aligned to 3 headers
    assert_eq!(table.rows[0], vec!["val1", "", ""]);
    // Row 1 aligned to 3 headers (truncated extra)
    assert_eq!(table.rows[1], vec!["valA", "valB", "valC"]);
}

#[test]
fn test_editor_save_auto_creates_directories_regression() {
    use skat_os::editor::Editor;

    let dir = tempfile::tempdir().unwrap();
    let target_path = dir.path().join("deeply").join("nested").join("runbook.md");
    assert!(!target_path.exists());

    let buf = TextBuffer::from_str("# Deeply Nested Document\n");
    let mut editor = Editor::new();
    editor.buffer = buf;
    editor
        .save_as(&target_path)
        .expect("save_as should auto-create parent directories");

    assert!(target_path.exists());
    let saved_content = std::fs::read_to_string(&target_path).unwrap();
    assert_eq!(saved_content, "# Deeply Nested Document\n");
}
