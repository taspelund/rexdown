mod app;
mod markdown;
mod palette;
mod revision;
mod screenshot;
mod theme;

use std::{
    env, io,
    path::PathBuf,
    process::ExitCode,
    time::{Duration, Instant},
};

use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture},
    execute,
};

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("rexdown: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> io::Result<()> {
    let mut args = env::args_os().skip(1).peekable();
    match args.peek().and_then(|s| s.to_str()) {
        Some("--help" | "-h") => {
            println!(
                "Rexdown — live, read-only Markdown\n\nUsage: rexdown [--screenshot-dir DIR] [--] FILE\n       rexdown --check\n       rexdown --theme-licenses\n\nCtrl+P command palette: themes, keys, screenshots, refresh, contents, changes, quit\nq quit · t contents · r refresh · ? keys · [/] previous/next change\nd/u half page · j/k scroll · g/G top/bottom\narrows / PageUp / PageDown · ←/→ pan · clickable footer\n\nRefresh follows the current source block. Marks show the latest content update.\nScreenshots save SVG files in Pictures/Rexdown by default.\n--screenshot-dir DIR chooses another directory (created on export).\nFilenames include the document name and UTC date/time.\nPolls UTF-8 file contents every 750 ms. No URLs or files are opened by links."
            );
            return Ok(());
        }
        Some("--theme-licenses") => {
            println!("{}", include_str!("../THEME_LICENSES.md"));
            println!("{}", two_face::acknowledgement::listing().to_md());
            return Ok(());
        }
        Some("--version" | "-V") => {
            println!("rexdown {}", env!("CARGO_PKG_VERSION"));
            return Ok(());
        }
        Some("--check") => {
            println!("{}", env::current_exe()?.display());
            return Ok(());
        }
        _ => {}
    }
    let usage = || {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "usage: rexdown [--screenshot-dir DIR] [--] FILE (or --help)",
        )
    };
    let mut path = None;
    let mut screenshot_directory = None;
    let mut positional_only = false;
    while let Some(arg) = args.next() {
        if !positional_only && arg == "--" {
            positional_only = true;
        } else if !positional_only && arg == "--screenshot-dir" {
            let directory = args.next().filter(|s| !s.is_empty()).ok_or_else(usage)?;
            screenshot_directory = Some(PathBuf::from(directory));
        } else if (!positional_only && arg.to_string_lossy().starts_with('-')) || path.is_some() {
            return Err(usage());
        } else {
            path = Some(PathBuf::from(arg));
        }
    }
    let mut app = app::App::new(path.ok_or_else(usage)?);
    app.screenshot_directory = screenshot_directory;
    app.reload();
    // The guard covers setup failures and normal errors as well as unwinding.
    let _restore = RestoreTerminal;
    let mut terminal = ratatui::try_init()?;
    execute!(io::stdout(), EnableMouseCapture)?;
    let interval = Duration::from_millis(750);
    let mut next_read = Instant::now() + interval;
    loop {
        terminal.draw(|frame| app.draw(frame))?;
        if event::poll(next_read.saturating_duration_since(Instant::now()))?
            && !app.event(event::read()?)
        {
            break;
        }
        if Instant::now() >= next_read {
            app.reload();
            next_read = Instant::now() + interval;
        }
    }
    Ok(())
}

struct RestoreTerminal;

impl Drop for RestoreTerminal {
    fn drop(&mut self) {
        let _ = execute!(io::stdout(), DisableMouseCapture);
        ratatui::restore();
    }
}
