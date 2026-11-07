use anyhow::Result;
use crossterm::cursor::Show;
use crossterm::event::{self, Event, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use std::env;
use std::io::stdout;
use std::path::PathBuf;
use std::time::Duration;

use skat_os::app::App;
use skat_os::ui::UI;

#[tokio::main]
async fn main() -> Result<()> {
    // 1. Determine workspace path
    let args: Vec<String> = env::args().collect();
    let root_dir = if args.len() > 1 {
        PathBuf::from(&args[1])
    } else {
        PathBuf::from("./docs")
    };

    // 2. Setup panic hook to cleanly restore terminal
    let default_panic = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = disable_raw_mode();
        let _ = execute!(stdout(), LeaveAlternateScreen, Show);
        default_panic(info);
    }));

    // 3. Initialize terminal in raw mode & alternate screen
    enable_raw_mode()?;
    let mut stdout = stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // 4. Initialize App
    let mut app = match App::new(root_dir) {
        Ok(a) => a,
        Err(e) => {
            disable_raw_mode()?;
            execute!(std::io::stdout(), LeaveAlternateScreen, Show)?;
            eprintln!("Error initializing skat-os: {}", e);
            return Err(e);
        }
    };

    // 5. Main Event Loop
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

        // Poll for inputs or timer tick
        if event::poll(tick_rate)? {
            if let Event::Key(key) = event::read()? {
                // Ignore key release events to prevent double inputs
                if key.kind == KeyEventKind::Press {
                    app.handle_key_event(key);
                }
            }
        }

        if app.should_quit {
            break;
        }
    }

    // 6. Cleanup terminal
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen, Show)?;
    terminal.show_cursor()?;

    println!("skat-os exited gracefully.");
    Ok(())
}
