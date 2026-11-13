use anyhow::Result;
use crossterm::cursor::Show;
use crossterm::event::{self, Event, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use std::env;
use std::io::stdout;
use std::time::Duration;

use skat_os::app::App;
use skat_os::cli::{CliCommand, parse_cli_args};
use skat_os::collab::CollabServer;
use skat_os::ui::UI;
use skat_os::workspace::Workspace;

#[tokio::main]
async fn main() -> Result<()> {
    // 1. Parse command line arguments
    let raw_args: Vec<String> = env::args().collect();
    let command = match parse_cli_args(&raw_args) {
        Ok(cmd) => cmd,
        Err(e) => {
            eprintln!("Error: {}", e);
            eprintln!("Run 'skat-os --help' for usage instructions.");
            std::process::exit(1);
        }
    };

    if let CliCommand::Help = command {
        println!("skat-os - Terminal Document Operating System & Real-Time Multiplayer Runbooks\n");
        println!("USAGE:");
        println!("    skat-os [WORKSPACE]");
        println!(
            "    skat-os host [--port <PORT>] [--workspace <PATH>] [--name <NAME>] [--headless]"
        );
        println!("    skat-os join <HOST:PORT> [--name <NAME>]\n");
        println!("COMMANDS:");
        println!("    host    Start an ephemeral collaboration room server and launch TUI");
        println!("    join    Join an active collaboration room from another terminal");
        println!("    help    Print this help message\n");
        println!("OPTIONS:");
        println!("    -p, --port <PORT>        Port to bind server (default: 2222)");
        println!("    -w, --workspace <PATH>   Path to document workspace (default: ./docs)");
        println!("    -n, --name <NAME>        Your collaborator display name (default: $USER)");
        println!("    --headless               Run headless server without TUI");
        return Ok(());
    }

    // 2. Handle headless host mode
    if let CliCommand::Host {
        port,
        workspace,
        name: _,
        headless: true,
    } = command
    {
        println!("Starting headless skat-os collaboration server...");
        let ws = Workspace::new(workspace)?;
        let first_doc = ws.docs.first().map(|d| d.path.clone());
        let (initial_content, doc_name) = if let Some(path) = first_doc {
            let content = std::fs::read_to_string(&path).unwrap_or_default();
            let name = path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            (content, name)
        } else {
            (
                "# Welcome\n\nShared Document Room\n".to_string(),
                "welcome.md".to_string(),
            )
        };

        let bind_addr = format!("0.0.0.0:{}", port);
        let server_handle = CollabServer::bind(&bind_addr, initial_content, doc_name).await?;
        println!(
            "Collaboration server listening on {}",
            server_handle.local_addr
        );
        println!("Press Ctrl+C to stop.");
        tokio::signal::ctrl_c().await?;
        println!("\nServer stopped.");
        return Ok(());
    }

    // 3. Setup panic hook to cleanly restore terminal
    let default_panic = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = disable_raw_mode();
        let _ = execute!(stdout(), LeaveAlternateScreen, Show);
        default_panic(info);
    }));

    // 4. Initialize terminal in raw mode & alternate screen
    enable_raw_mode()?;
    let mut stdout = stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // 5. Initialize App based on command
    let mut app = match command {
        CliCommand::Host {
            port,
            workspace,
            name,
            headless: false,
        } => match App::new_host(workspace, port, name).await {
            Ok(a) => a,
            Err(e) => {
                cleanup_terminal(&mut terminal)?;
                eprintln!("Error hosting skat-os room: {}", e);
                return Err(e);
            }
        },
        CliCommand::Join { addr, name } => match App::new_client(&addr, name).await {
            Ok(a) => a,
            Err(e) => {
                cleanup_terminal(&mut terminal)?;
                eprintln!("Error joining skat-os room at {}: {}", addr, e);
                return Err(e);
            }
        },
        CliCommand::Local { workspace } => match App::new(workspace) {
            Ok(a) => a,
            Err(e) => {
                cleanup_terminal(&mut terminal)?;
                eprintln!("Error initializing skat-os: {}", e);
                return Err(e);
            }
        },
        _ => unreachable!(),
    };

    // 6. Main Event Loop
    let tick_rate = Duration::from_millis(25);
    loop {
        app.handle_app_events();

        // Adjust editor scroll to keep cursor visible
        let size = terminal.size()?;
        let editor_width = if app.sidebar_visible {
            let sb_width = (size.width * 25 / 100).clamp(24, 38);
            (size.width.saturating_sub(sb_width)).saturating_sub(6) as usize
        } else {
            size.width.saturating_sub(6) as usize
        };
        let editor_height = size.height.saturating_sub(3) as usize;
        app.editor.buffer.adjust_scroll(editor_width, editor_height);

        // Draw TUI
        terminal.draw(|frame| {
            UI::render(&app, frame);
        })?;

        // Poll for inputs, window resize, or timer tick
        if event::poll(tick_rate)? {
            match event::read()? {
                Event::Key(key) => {
                    // Ignore key release events to prevent double inputs
                    if key.kind == KeyEventKind::Press {
                        app.handle_key_event(key);
                    }
                }
                Event::Resize(_, _) => {
                    // Redraw triggers next loop iteration with updated terminal size
                }
                _ => {}
            }
        }

        if app.should_quit {
            break;
        }
    }

    // 7. Cleanup terminal
    cleanup_terminal(&mut terminal)?;

    println!("skat-os exited gracefully.");
    Ok(())
}

fn cleanup_terminal(terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>) -> Result<()> {
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen, Show)?;
    let _ = terminal.show_cursor();
    Ok(())
}
