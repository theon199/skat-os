use skat_os::editor::buffer::TextBuffer;
use skat_os::palette::CommandPalette;
use skat_os::runner::Runner;
use skat_os::workspace::Workspace;

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

    let result = Runner::execute(&block, temp_dir.path()).await.expect("Execution should succeed");
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
    assert!(top_item.label.to_lowercase().contains("runbook") || top_item.subtext.contains("runbook"));
}
