use super::*;
use crossterm::event::{KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::{Terminal, backend::TestBackend};

fn draw(app: &mut App, width: u16, height: u16) -> String {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal.draw(|frame| app.draw(frame)).unwrap();
    terminal
        .backend()
        .buffer()
        .content
        .iter()
        .map(|cell| cell.symbol())
        .collect()
}

fn key(app: &mut App, code: KeyCode) -> bool {
    app.event(Event::Key(KeyEvent::new(code, KeyModifiers::NONE)))
}

fn line(screen: &str, width: usize, row: usize) -> String {
    screen.chars().skip(width * row).take(width).collect()
}

fn click(app: &mut App, column: u16, row: u16) {
    app.event(Event::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column,
        row,
        modifiers: KeyModifiers::NONE,
    }));
}

#[test]
fn refresh_tracks_the_passage_and_navigates_latest_changes() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("board.md");
    let source = (0..60)
        .map(|n| format!("Passage {n}\n\n"))
        .collect::<String>();
    std::fs::write(&path, &source).unwrap();
    let mut app = App::new(path.clone());
    app.reload();
    let first = draw(&mut app, 110, 14);
    assert!(first.contains("Loaded "));
    app.reload();
    assert_eq!(
        draw(&mut app, 110, 14),
        first,
        "unchanged polls are not updates"
    );
    key(&mut app, KeyCode::PageDown);
    assert!(line(&draw(&mut app, 110, 14), 110, 1).contains("Passage 6"));

    let added = format!(
        "# New introduction\n\n{}",
        source.replace("Passage 30\n", "Changed thirty\n")
    );
    std::fs::write(&path, &added).unwrap();
    app.reload();
    app.reload(); // Multiple reads before a draw must not lose the old anchor.
    let updated = draw(&mut app, 110, 14);
    assert!(line(&updated, 110, 1).contains("Passage 6"));
    assert!(updated.contains("Updated ") && updated.contains("2 changes"));
    key(&mut app, KeyCode::Char(']'));
    let next = draw(&mut app, 110, 14);
    assert!(line(&next, 110, 1).contains("Changed thirty"));
    assert!(
        line(&next, 110, 1).starts_with('▎'),
        "changed passage has a gutter marker"
    );
    key(&mut app, KeyCode::Char('['));
    assert!(line(&draw(&mut app, 110, 14), 110, 1).contains("New introduction"));
    key(&mut app, KeyCode::Char(']'));
    draw(&mut app, 110, 14);

    // Deleting the passage being read follows its successor, not an old screen row.
    std::fs::write(&path, added.replace("Changed thirty\n\n", "")).unwrap();
    app.reload();
    let deleted = draw(&mut app, 110, 14);
    assert!(line(&deleted, 110, 1).contains("Passage 31"));
    assert!(deleted.contains("1 change"));
    app.reload();
    assert_eq!(
        draw(&mut app, 110, 14),
        deleted,
        "polls retain latest-change markers"
    );
    draw(&mut app, 45, 12);
    key(&mut app, KeyCode::Char('t'));
    assert!(draw(&mut app, 45, 12).contains("Passage 31"));
}

#[test]
fn wrapped_passage_survives_reflow_and_changes_cycle_at_eof() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("board.md");
    let source = format!(
        "[Read](#reading)\n\n{}\n\n# Reading\n\n{}\n\n{}\n\nLast paragraph",
        "Before ".repeat(80),
        "Needle ".repeat(200),
        "After ".repeat(90)
    );
    std::fs::write(&path, &source).unwrap();
    let mut app = App::new(path.clone());
    app.reload();
    draw(&mut app, 90, 14);
    click(&mut app, 2, 1);
    draw(&mut app, 90, 14);
    for _ in 0..3 {
        key(&mut app, KeyCode::Down);
    }
    assert!(line(&draw(&mut app, 90, 14), 90, 1).contains("Needle"));
    for width in [45, 110, 62] {
        assert!(line(&draw(&mut app, width, 14), width as usize, 1).contains("Needle"));
    }
    let edited = source
        .replace("Before ", "A longer introduction ")
        .replace("Last paragraph", "Changed ending");
    std::fs::write(&path, edited).unwrap();
    app.reload();
    assert!(line(&draw(&mut app, 90, 14), 90, 1).contains("Needle"));
    key(&mut app, KeyCode::Char(']'));
    let end = draw(&mut app, 90, 14);
    assert!(end.contains("Changed ending"));
    assert!(line(&end, 90, 12).starts_with('▎'));
    // The end change cannot reach the top of the viewport; still advance past it.
    key(&mut app, KeyCode::Char(']'));
    assert!(line(&draw(&mut app, 90, 14), 90, 1).contains("A longer introduction"));
    key(&mut app, KeyCode::Char('['));
    assert!(draw(&mut app, 90, 14).contains("Changed ending"));
    std::fs::write(&path, "").unwrap();
    app.reload();
    assert!(!draw(&mut app, 90, 14).contains("Needle"));
    key(&mut app, KeyCode::Char(']'));
    std::fs::write(&path, "Recovered").unwrap();
    app.reload();
    assert!(draw(&mut app, 90, 14).contains("Recovered"));
}

#[test]
fn directly_opened_help_closes_without_opening_commands() {
    let mut app = App::new(PathBuf::from("board.md"));
    let document = draw(&mut app, 90, 24);
    let help_column = line(&document, 90, 23).find("? keys").unwrap() as u16;
    for from_footer in [true, false] {
        if from_footer {
            click(&mut app, help_column, 23);
        } else {
            key(&mut app, KeyCode::Char('?'));
        }
        let help = draw(&mut app, 90, 24);
        assert!(help.contains("Key bindings"));
        key(&mut app, KeyCode::Esc);
        assert_eq!(draw(&mut app, 90, 24), document);
    }
}

#[test]
fn footer_actions_are_clickable_hoverable_and_survive_notices() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("board.md");
    std::fs::write(
        &path,
        format!(
            "# Board\n\n[Inspect](https://example.com)\n\n{}",
            "Body has a long passage that must not leak through a notice.\n\n".repeat(20)
        ),
    )
    .unwrap();
    let mut app = App::new(path);
    app.reload();
    let mut terminal = Terminal::new(TestBackend::new(110, 20)).unwrap();
    terminal.draw(|f| app.draw(f)).unwrap();
    let normal = terminal.backend().buffer()[(2, 19)].modifier;
    app.event(Event::Mouse(MouseEvent {
        kind: MouseEventKind::Moved,
        column: 2,
        row: 19,
        modifiers: KeyModifiers::NONE,
    }));
    terminal.draw(|f| app.draw(f)).unwrap();
    assert_ne!(terminal.backend().buffer()[(2, 19)].modifier, normal);
    click(&mut app, 2, 19);
    assert!(draw(&mut app, 110, 20).contains("Command palette"));
    key(&mut app, KeyCode::Esc);
    click(&mut app, 2, 3);
    let screen = draw(&mut app, 110, 20);
    assert!(screen.contains("Link (read-only): https://example.com"));
    let footer = screen.chars().skip(110 * 19).collect::<String>();
    let contents = footer.find("contents").unwrap();
    click(&mut app, contents as u16, 19);
    assert!(draw(&mut app, 110, 20).contains(" Contents"));
    key(&mut app, KeyCode::Esc);
    assert!(!draw(&mut app, 110, 20).contains("Link (read-only)"));
    key(&mut app, KeyCode::Down);
    key(&mut app, KeyCode::Char('r'));
    assert_eq!(
        line(&draw(&mut app, 110, 20), 110, 18).trim(),
        "Already up to date"
    );
}

#[test]
fn contents_hover_covers_the_whole_entry_without_navigating() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("board.md");
    std::fs::write(
        &path,
        format!(
            "# Alpha\n\n{}## Beta\n\n{}",
            "Before\n\n".repeat(20),
            "After\n\n".repeat(20)
        ),
    )
    .unwrap();
    let mut app = App::new(path);
    app.reload();
    key(&mut app, KeyCode::Char('t'));
    let mut terminal = Terminal::new(TestBackend::new(90, 24)).unwrap();
    terminal.draw(|f| app.draw(f)).unwrap();
    let normal = terminal.backend().buffer().clone();

    // Hover the padding, not the label. Moving away, onto the title, or
    // onto empty contents space must remove the entire highlight.
    for (column, row) in [(29, 3), (30, 3), (29, 1), (29, 5)] {
        app.event(Event::Mouse(MouseEvent {
            kind: MouseEventKind::Moved,
            column,
            row,
            modifiers: KeyModifiers::NONE,
        }));
        terminal.draw(|f| app.draw(f)).unwrap();
        let buffer = terminal.backend().buffer();
        for y in 1..23 {
            for x in 0..90 {
                let cell = &buffer[(x, y)];
                if (column, row) == (29, 3) && y == 3 && x < 30 {
                    assert!(cell.modifier.contains(Modifier::REVERSED), "column {x}");
                    assert_eq!(cell.symbol(), normal[(x, y)].symbol());
                } else {
                    assert_eq!(cell, &normal[(x, y)]);
                }
            }
        }
    }
    click(&mut app, 29, 3);
    assert!(line(&draw(&mut app, 90, 24), 90, 1).contains("Beta"));
}

#[test]
fn refresh_preserves_offset_then_recovers_and_clamps() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("document.md");
    let source = (0..60)
        .map(|n| format!("Original {n}\n\n"))
        .collect::<String>();
    std::fs::write(&path, &source).unwrap();
    let mut app = App::new(path.clone());
    app.reload();
    draw(&mut app, 90, 24);
    key(&mut app, KeyCode::PageDown);
    let before = draw(&mut app, 90, 24);
    std::fs::write(&path, source.replace("Original", "Replaced")).unwrap();
    app.reload();
    let after = draw(&mut app, 90, 24);
    for row in 1..23 {
        assert_eq!(
            line(&after, 90, row).trim_start_matches('▎').trim(),
            line(&before, 90, row)
                .replace("Original", "Replaced")
                .trim()
        );
    }
    std::fs::remove_file(&path).unwrap();
    app.reload();
    let failed = draw(&mut app, 90, 24);
    assert!(failed.contains("Cannot read file"));
    assert!(failed.contains("Replaced 11"));
    std::fs::write(&path, [0xff]).unwrap();
    app.reload();
    assert!(draw(&mut app, 90, 24).contains("Replaced 11"));
    std::fs::write(&path, "# Recovered\n\nShort replacement").unwrap();
    app.reload();
    let recovered = draw(&mut app, 90, 24);
    assert!(recovered.contains("Recovered"));
    assert!(recovered.contains("Short replacement"));
    assert!(!recovered.contains("Cannot read file"));
}

#[test]
fn half_pages_follow_resize_and_contents_is_optional() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("document.md");
    std::fs::write(
        &path,
        format!(
            "# Board\n\n{}",
            (0..100).map(|n| format!("row {n}\n\n")).collect::<String>()
        ),
    )
    .unwrap();
    let mut app = App::new(path);
    app.reload();
    assert!(!draw(&mut app, 110, 32).contains("Contents"));
    key(&mut app, KeyCode::Char('d'));
    assert!(draw(&mut app, 110, 32).contains("row 7"));
    key(&mut app, KeyCode::Char('u'));
    key(&mut app, KeyCode::Char('u'));
    assert!(draw(&mut app, 110, 32).contains("Board"));
    key(&mut app, KeyCode::Char('t'));
    assert!(draw(&mut app, 110, 32).contains("Contents"));
    key(&mut app, KeyCode::Char('t'));
    draw(&mut app, 45, 20);
    key(&mut app, KeyCode::Char('d'));
    let screen = draw(&mut app, 45, 20);
    assert!(screen.contains("row 4"));
    assert!(!screen.contains("row 2 "));
    key(&mut app, KeyCode::Char('G'));
    let bottom = draw(&mut app, 45, 20);
    key(&mut app, KeyCode::Char('d'));
    assert_eq!(draw(&mut app, 45, 20), bottom);
    assert!(!key(&mut app, KeyCode::Char('q')));
}

#[test]
fn real_link_hit_testing_never_replaces_watched_document() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("document.md");
    std::fs::write(&path, format!("[External](https://example.com)\n\n[File](missing.md)\n\n[Jump](#candidate)\n\n{}## Candidate\n\nNot accepted", "Filler\n\n".repeat(50))).unwrap();
    let mut app = App::new(path);
    app.reload();
    draw(&mut app, 90, 24);
    click(&mut app, 2, 1);
    assert!(draw(&mut app, 90, 24).contains("https://example.com"));
    click(&mut app, 2, 3);
    assert!(draw(&mut app, 90, 24).contains("missing.md"));
    click(&mut app, 2, 5);
    let screen = draw(&mut app, 90, 24);
    assert!(screen.contains("Not accepted"));
    assert!(!screen.contains("External"));
    key(&mut app, KeyCode::Char('g'));
    key(&mut app, KeyCode::Char('t'));
    draw(&mut app, 90, 24);
    click(&mut app, 2, 2);
    assert!(draw(&mut app, 90, 24).contains("Not accepted"));
}

#[test]
fn status_and_help_backgrounds_fill_the_entire_pane() {
    let mut app = App::new(PathBuf::from("short.md"));
    palette(&mut app, "theme");
    key(&mut app, KeyCode::Enter);
    for c in "dark".chars() {
        key(&mut app, KeyCode::Char(c));
    }
    key(&mut app, KeyCode::Enter);
    for (width, height) in [(110, 32), (45, 20), (1, 2)] {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal.draw(|frame| app.draw(frame)).unwrap();
        let buffer = terminal.backend().buffer();
        for y in [0, height - 1] {
            let background = buffer[(0, y)].bg;
            assert_ne!(background, Color::Rgb(24, 27, 33));
            for x in 0..width {
                assert_eq!(
                    buffer[(x, y)].bg,
                    background,
                    "column {x} of {width}, row {y}"
                );
            }
        }
    }
}

fn palette(app: &mut App, query: &str) {
    assert!(app.event(Event::Key(KeyEvent::new(
        KeyCode::Char('p'),
        KeyModifiers::CONTROL
    ))));
    for c in query.chars() {
        assert!(key(app, KeyCode::Char(c)));
    }
}

#[test]
fn palette_filters_executes_and_keeps_typing_out_of_document_shortcuts() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("board.md");
    std::fs::write(&path, "# Board\n\nBody").unwrap();
    let mut app = App::new(path);
    app.reload();
    let original = draw(&mut app, 90, 24);
    palette(&mut app, "q"); // Must search, not quit.
    let screen = draw(&mut app, 90, 24);
    assert!(screen.contains("Command palette"));
    assert!(screen.contains("Quit"));
    assert!(!screen.contains("Screenshot"));
    key(&mut app, KeyCode::Esc);
    assert_eq!(draw(&mut app, 90, 24), original);
    palette(&mut app, "contents");
    assert!(key(&mut app, KeyCode::Enter));
    assert!(draw(&mut app, 90, 24).contains("Contents"));
    palette(&mut app, "keys");
    key(&mut app, KeyCode::Enter);
    assert!(draw(&mut app, 90, 24).contains("Key bindings"));
    draw(&mut app, 45, 8);
    key(&mut app, KeyCode::End);
    assert!(draw(&mut app, 45, 8).contains("Back / close; dismiss notice"));
    key(&mut app, KeyCode::Esc);
    assert!(draw(&mut app, 45, 8).contains("Command palette"));
    key(&mut app, KeyCode::Esc);
    palette(&mut app, "zzzzz");
    assert!(draw(&mut app, 45, 12).contains("No matches"));
    key(&mut app, KeyCode::Enter);
    assert!(draw(&mut app, 45, 12).contains("No matches"));
    key(&mut app, KeyCode::Esc);
    palette(&mut app, "");
    draw(&mut app, 45, 8);
    key(&mut app, KeyCode::End);
    assert!(draw(&mut app, 45, 8).contains("Contents — toggle"));
    click(&mut app, 3, 4);
    assert!(!draw(&mut app, 45, 8).contains("Contents"));
    palette(&mut app, "");
    for (width, height) in [(1, 1), (3, 3), (8, 4)] {
        draw(&mut app, width, height);
        key(&mut app, KeyCode::Down);
    }
    key(&mut app, KeyCode::Esc);
    palette(&mut app, "quit");
    assert!(!key(&mut app, KeyCode::Enter));
}

#[test]
fn theme_picker_changes_prose_and_code_and_restores_terminal() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("board.md");
    std::fs::write(&path, "Prose\n\n```rust\nlet x = 42;\n```").unwrap();
    let mut app = App::new(path);
    app.reload();
    let mut terminal = Terminal::new(TestBackend::new(80, 20)).unwrap();
    terminal.draw(|f| app.draw(f)).unwrap();
    let inherited = terminal.backend().buffer().clone();
    palette(&mut app, "theme");
    key(&mut app, KeyCode::Enter);
    assert!(draw(&mut app, 45, 15).contains("Choose theme"));
    for c in "light".chars() {
        key(&mut app, KeyCode::Char(c));
    }
    key(&mut app, KeyCode::Enter);
    terminal.draw(|f| app.draw(f)).unwrap();
    let light = terminal.backend().buffer();
    assert_ne!(inherited[(1, 1)].bg, light[(1, 1)].bg);
    assert_ne!(inherited[(1, 1)].fg, light[(1, 1)].fg);
    assert_ne!(inherited[(3, 3)].fg, light[(3, 3)].fg);
    palette(&mut app, "theme");
    key(&mut app, KeyCode::Enter);
    for c in "terminal".chars() {
        key(&mut app, KeyCode::Char(c));
    }
    key(&mut app, KeyCode::Enter);
    terminal.draw(|f| app.draw(f)).unwrap();
    assert_eq!(terminal.backend().buffer(), &inherited);

    // Independent palette values distinguish real theme selection from a
    // renamed Dark/Light toggle. Code must also change, not just the UI.
    for (name, background) in [
        ("Nord", Color::Rgb(46, 52, 64)),
        ("Catppuccin Latte", Color::Rgb(239, 241, 245)),
    ] {
        palette(&mut app, "theme");
        key(&mut app, KeyCode::Enter);
        for c in name.chars() {
            key(&mut app, KeyCode::Char(c));
        }
        let picker = draw(&mut app, 45, 12);
        assert!(picker.contains(name) && !picker.contains("No matches"));
        key(&mut app, KeyCode::Enter);
        terminal.draw(|f| app.draw(f)).unwrap();
        let themed = terminal.backend().buffer();
        assert_eq!(themed[(1, 1)].bg, background);
        assert_ne!(themed[(3, 3)].fg, inherited[(3, 3)].fg);
    }
}

#[test]
fn default_inherits_terminal_colors() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("board.md");
    std::fs::write(
        &path,
        "Prose\n\n```rust\nlet x = \"hello\"; // comment\n```",
    )
    .unwrap();
    let mut app = App::new(path);
    app.reload();
    let mut terminal = Terminal::new(TestBackend::new(80, 20)).unwrap();
    terminal.draw(|f| app.draw(f)).unwrap();
    let buffer = terminal.backend().buffer();
    assert_eq!(buffer[(1, 1)].fg, Color::Reset);
    assert_eq!(buffer[(1, 1)].bg, Color::Reset);
    assert!(buffer.content.iter().all(|c| !matches!(c.fg, Color::Rgb(..) | Color::Indexed(16..)) && c.bg == Color::Reset));
    // The ANSI syntax theme maps comments to the terminal's green, not an
    // almost-black RGB color made from its encoded palette index.
    assert_eq!(buffer[(23, 3)].symbol(), "c");
    assert_eq!(buffer[(23, 3)].fg, Color::Indexed(2));
    palette(&mut app, "");
    terminal.draw(|f| app.draw(f)).unwrap();
    assert!(
        terminal.backend().buffer()[(12, 6)]
            .modifier
            .contains(ratatui::style::Modifier::REVERSED)
    );
}

#[test]
fn screenshot_command_saves_visible_document_not_palette_and_reports_failure() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("board.md");
    let output = dir.path().join("exports/nested");
    std::fs::write(
        &path,
        format!(
            "# Visible & safe\n\n```\n<script>alert(1)</script>\n```\n\nUnicode: 界 é \u{ffff}\n\n{}\n# Offscreen sentinel",
            "Filler\n\n".repeat(30)
        ),
    )
    .unwrap();
    let mut app = App::new(path.clone());
    app.screenshot_directory = Some(output.clone());
    app.reload();
    palette(&mut app, "theme");
    key(&mut app, KeyCode::Enter);
    for c in "dark".chars() {
        key(&mut app, KeyCode::Char(c));
    }
    key(&mut app, KeyCode::Enter);
    draw(&mut app, 80, 20);
    for _ in 0..2 {
        palette(&mut app, "screenshot");
        key(&mut app, KeyCode::Enter);
        draw(&mut app, 80, 20);
        assert!(draw(&mut app, 80, 20).contains("Saved screenshot"));
    }
    assert!(output.is_dir(), "export must create the chosen directory");
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 2);
    let files: Vec<_> = std::fs::read_dir(&output)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|s| s == "svg"))
        .collect();
    assert_eq!(files.len(), 2);
    for file in &files {
        let name = file.file_name().unwrap().to_str().unwrap();
        assert!(name.starts_with("board-20"), "{name}");
        // A readable UTC date and time, with a suffix only for collisions.
        assert_eq!(&name[10..11], "-");
        assert_eq!(&name[13..14], "-");
        assert_eq!(&name[16..17], "_");
        assert_eq!(&name[25..26], "Z");
    }
    let svg = std::fs::read_to_string(&files[0]).unwrap();
    assert!(svg.contains("&amp;"));
    assert!(svg.contains("&lt;"));
    assert!(!svg.contains("<script>"));
    let xml = roxmltree::Document::parse(&svg).unwrap();
    let text: String = xml
        .descendants()
        .filter(|n| n.has_tag_name("text"))
        .filter_map(|n| n.text())
        .collect();
    let text: String = text.chars().filter(|c| !c.is_whitespace()).collect();
    assert!(text.contains("Visible&safe"));
    assert!(text.contains("<script>alert(1)</script>"));
    assert!(!text.contains("Offscreensentinel"));
    assert!(!text.contains("Commandpalette"));
    // A file where the output directory should be must fail without changing it.
    app.screenshot_directory = Some(path.clone());
    let source = std::fs::read(&path).unwrap();
    palette(&mut app, "screenshot");
    key(&mut app, KeyCode::Enter);
    draw(&mut app, 80, 20);
    assert!(draw(&mut app, 80, 20).contains("Cannot save screenshot"));
    assert_eq!(std::fs::read(&path).unwrap(), source);
}
