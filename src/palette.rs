use crossterm::event::{Event, KeyCode, KeyModifiers, MouseButton, MouseEventKind};
use ratatui::{
    Frame,
    layout::{Position, Rect},
    style::Modifier,
    text::Line,
    widgets::{Block, Borders, Clear, Paragraph},
};

use crate::theme::Theme;

#[derive(Clone, Copy)]
pub enum Command {
    Commands,
    Theme,
    Quit,
    Keys,
    Screenshot,
    Refresh,
    Contents,
    PreviousChange,
    NextChange,
    SetTheme(Theme),
    Dismiss,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Commands,
    Themes,
    Keys,
}

pub struct Palette {
    kind: Kind,
    from_commands: bool,
    query: String,
    selected: usize,
    offset: usize,
    items_area: Rect,
}

const COMMANDS: &[(&str, Command)] = &[
    ("Theme — change colors", Command::Theme),
    ("Quit — close viewer", Command::Quit),
    ("Keys — show key bindings", Command::Keys),
    ("Screenshot — save viewport as SVG", Command::Screenshot),
    ("Refresh — reread Markdown", Command::Refresh),
    ("Previous change — jump backward [", Command::PreviousChange),
    ("Next change — jump forward ]", Command::NextChange),
    ("Contents — toggle heading list", Command::Contents),
];
const KEYS: &[&str] = &[
    "Ctrl+P       Command palette",
    "q / Ctrl+C   Quit viewer",
    "t            Toggle contents",
    "r            Refresh file",
    "[ / ]        Previous / next change (wraps)",
    "?            Key bindings",
    "j / Down     Scroll down",
    "k / Up       Scroll up",
    "d / u        Half page down / up",
    "PageDown     Next page (also Space)",
    "PageUp       Previous page",
    "g / Home     Top; reset horizontal pan",
    "G / End      Bottom",
    "h / Left     Pan left",
    "l / Right    Pan right",
    "Mouse wheel  Document / contents",
    "Click        Footer / heading / inspect link",
    "Palette: type to filter; Up/Down to select",
    "Enter        Run selected command",
    "Esc          Back / close; dismiss notice",
];

impl Palette {
    pub fn new(kind: Kind, from_commands: bool) -> Self {
        Self {
            kind,
            from_commands,
            query: String::new(),
            selected: 0,
            offset: 0,
            items_area: Rect::default(),
        }
    }

    fn entries(&self) -> Vec<(&'static str, Command)> {
        if self.kind == Kind::Keys {
            return KEYS.iter().map(|&s| (s, Command::Dismiss)).collect();
        }
        let entries = if self.kind == Kind::Themes {
            Theme::all()
                .map(|theme| (theme.name(), Command::SetTheme(theme)))
                .collect()
        } else {
            COMMANDS.to_vec()
        };
        let query = self.query.to_lowercase();
        entries
            .into_iter()
            .filter(|(label, _)| {
                query
                    .split_whitespace()
                    .all(|word| label.to_lowercase().contains(word))
            })
            .collect()
    }

    pub fn draw(&mut self, frame: &mut Frame, theme: Theme) {
        let screen = frame.area();
        let width = screen.width.saturating_sub(2).min(60);
        let height = screen
            .height
            .saturating_sub(2)
            .min(if self.kind == Kind::Keys { 23 } else { 12 });
        let area = Rect::new(
            screen.x + (screen.width - width) / 2,
            screen.y + (screen.height - height) / 2,
            width,
            height,
        );
        let title = match self.kind {
            Kind::Commands => " Command palette ",
            Kind::Themes => " Choose theme ",
            Kind::Keys => " Key bindings ",
        };
        let block = Block::default()
            .borders(Borders::ALL)
            .title(title)
            .style(theme.style())
            .border_style(theme.style().fg(theme.accent()));
        let inner = block.inner(area);
        frame.render_widget(Clear, area);
        frame.render_widget(block, area);
        let query_rows = u16::from(self.kind != Kind::Keys).min(inner.height);
        if query_rows > 0 {
            let prompt = format!("> {}", self.query);
            // Keep the end of a long query, plus one cell for the cursor,
            // visible without slicing UTF-8 bytes.
            let full = unicode_width::UnicodeWidthStr::width(prompt.as_str());
            let offset = (full + 1).saturating_sub(usize::from(inner.width));
            let mut line = prompt.as_str();
            let mut skipped = 0;
            while skipped < offset {
                let Some(c) = line.chars().next() else {
                    break;
                };
                skipped += unicode_width::UnicodeWidthChar::width(c).unwrap_or(0);
                line = &line[c.len_utf8()..];
            }
            // Skipping a wide character can overshoot, so measure what remains.
            let shown = unicode_width::UnicodeWidthStr::width(line);
            if shown < usize::from(inner.width) {
                frame.set_cursor_position(Position::new(inner.x + shown as u16, inner.y));
            }
            frame.render_widget(
                Paragraph::new(line).style(theme.style()),
                Rect::new(inner.x, inner.y, inner.width, 1),
            );
        }
        self.items_area = Rect::new(
            inner.x,
            inner.y + query_rows,
            inner.width,
            inner.height.saturating_sub(query_rows + 1),
        );
        let entries = self.entries();
        self.selected = self.selected.min(entries.len().saturating_sub(1));
        let visible = usize::from(self.items_area.height).max(1);
        if self.kind == Kind::Keys {
            self.selected = self.selected.min(entries.len().saturating_sub(visible));
            self.offset = self.selected;
        } else {
            self.offset = self.offset.min(self.selected);
            if self.selected >= self.offset + visible {
                self.offset = self.selected + 1 - visible;
            }
        }
        if entries.is_empty() {
            frame.render_widget(
                Paragraph::new("No matches").style(theme.style()),
                self.items_area,
            );
        }
        for (row, (label, _)) in entries.iter().enumerate().skip(self.offset).take(visible) {
            if row - self.offset >= usize::from(self.items_area.height) {
                break;
            }
            let style = if row == self.selected && self.kind != Kind::Keys {
                theme.bar_style().add_modifier(Modifier::BOLD)
            } else {
                theme.style()
            };
            frame.render_widget(
                Paragraph::new(Line::from(*label)).style(style),
                Rect::new(
                    self.items_area.x,
                    self.items_area.y + (row - self.offset) as u16,
                    self.items_area.width,
                    1,
                ),
            );
        }
        if inner.height > 0 {
            let help = match self.kind {
                Kind::Commands => "↑/↓ select · Enter run · Esc close",
                Kind::Themes if self.from_commands => "↑/↓ select · Enter apply · Esc back",
                Kind::Themes => "↑/↓ select · Enter apply · Esc close",
                Kind::Keys if self.from_commands => "↑/↓ scroll · Esc back",
                Kind::Keys => "↑/↓ scroll · Esc close",
            };
            frame.render_widget(
                Paragraph::new(help).style(theme.style().fg(theme.accent())),
                Rect::new(inner.x, inner.bottom() - 1, inner.width, 1),
            );
        }
    }

    pub fn event(&mut self, event: Event) -> Option<Command> {
        let entries = self.entries();
        let last = entries.len().saturating_sub(1);
        match event {
            Event::Key(key) => match key.code {
                KeyCode::Esc if !self.from_commands => return Some(Command::Dismiss),
                KeyCode::Esc => {
                    // Back to the command list, on the command that opened us.
                    let origin = if self.kind == Kind::Themes {
                        "Theme"
                    } else {
                        "Keys"
                    };
                    *self = Self::new(Kind::Commands, false);
                    self.selected = COMMANDS
                        .iter()
                        .position(|(label, _)| label.starts_with(origin))
                        .unwrap_or(0);
                }
                KeyCode::Enter if self.kind != Kind::Keys => {
                    return entries.get(self.selected).map(|(_, c)| *c);
                }
                KeyCode::Up => self.selected = self.selected.saturating_sub(1),
                KeyCode::Down => self.selected = (self.selected + 1).min(last),
                KeyCode::PageUp => {
                    self.selected = self
                        .selected
                        .saturating_sub(usize::from(self.items_area.height).max(1))
                }
                KeyCode::PageDown => {
                    self.selected = self
                        .selected
                        .saturating_add(usize::from(self.items_area.height).max(1))
                        .min(last)
                }
                KeyCode::Home => self.selected = 0,
                KeyCode::End => self.selected = last,
                KeyCode::Char('q') if self.kind == Kind::Keys => return Some(Command::Dismiss),
                KeyCode::Backspace if self.kind != Kind::Keys => {
                    self.query.pop();
                    self.selected = 0;
                    self.offset = 0;
                }
                KeyCode::Char(c)
                    if self.kind != Kind::Keys
                        && !key
                            .modifiers
                            .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
                {
                    self.query.push(c);
                    self.selected = 0;
                    self.offset = 0;
                }
                _ => {}
            },
            Event::Mouse(mouse) => match mouse.kind {
                MouseEventKind::ScrollDown => self.selected = (self.selected + 1).min(last),
                MouseEventKind::ScrollUp => self.selected = self.selected.saturating_sub(1),
                MouseEventKind::Moved | MouseEventKind::Down(MouseButton::Left)
                    if self.kind != Kind::Keys =>
                {
                    // Only a drawn row holding a real entry can be hovered or
                    // clicked; blank rows and space outside change nothing.
                    let point = Position::new(mouse.column, mouse.row);
                    let row = self
                        .items_area
                        .contains(point)
                        .then(|| self.offset + usize::from(mouse.row - self.items_area.y));
                    if let Some((row, (_, command))) =
                        row.and_then(|row| entries.get(row).map(|entry| (row, entry)))
                    {
                        self.selected = row;
                        if mouse.kind != MouseEventKind::Moved {
                            return Some(*command);
                        }
                    }
                }
                _ => {}
            },
            _ => {}
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyEvent, MouseEvent};
    use ratatui::{Terminal, backend::TestBackend};

    fn render(terminal: &mut Terminal<TestBackend>, palette: &mut Palette) -> Vec<String> {
        terminal
            .draw(|frame| palette.draw(frame, Theme::Terminal))
            .unwrap();
        let buffer = terminal.backend().buffer();
        (0..buffer.area.height)
            .map(|y| {
                (0..buffer.area.width)
                    .map(|x| buffer[(x, y)].symbol())
                    .collect()
            })
            .collect()
    }

    /// Text of the highlighted (theme-reversed) list row, if any.
    fn selected(terminal: &Terminal<TestBackend>, rows: &[String]) -> Option<String> {
        let buffer = terminal.backend().buffer();
        (3..buffer.area.height - 3)
            .find(|&y| buffer[(2, y)].modifier.contains(Modifier::REVERSED))
            .map(|y| {
                rows[usize::from(y)]
                    .trim_matches(|c: char| c == '│' || c.is_whitespace())
                    .to_string()
            })
    }

    fn mouse(kind: MouseEventKind, column: u16, row: u16) -> Event {
        Event::Mouse(MouseEvent {
            kind,
            column,
            row,
            modifiers: KeyModifiers::NONE,
        })
    }

    fn key(code: KeyCode) -> Event {
        Event::Key(KeyEvent::new(code, KeyModifiers::NONE))
    }

    #[test]
    fn hover_selects_without_activation_cursor_tracks_query_and_escape_backs_out() {
        // 60x14: query row y=2 (text from x=2), list y=3..=10, help y=11.
        let mut terminal = Terminal::new(TestBackend::new(60, 14)).unwrap();
        let mut palette = Palette::new(Kind::Commands, false);
        let rows = render(&mut terminal, &mut palette);
        assert!(selected(&terminal, &rows).unwrap().starts_with("Theme"));
        assert!(terminal.backend().cursor_visible());
        terminal.backend_mut().assert_cursor_position((4, 2));

        let moved = MouseEventKind::Moved;
        let click = MouseEventKind::Down(MouseButton::Left);
        assert!(palette.event(mouse(moved, 10, 5)).is_none());
        let rows = render(&mut terminal, &mut palette);
        assert!(selected(&terminal, &rows).unwrap().starts_with("Keys"));
        for c in "Keys".chars() {
            palette.event(key(KeyCode::Char(c)));
        }
        render(&mut terminal, &mut palette);
        // Blank list rows and space outside the modal neither select nor run.
        for (kind, column, row) in [(moved, 10, 9), (moved, 0, 0), (click, 10, 9), (click, 0, 0)] {
            assert!(palette.event(mouse(kind, column, row)).is_none());
        }
        let rows = render(&mut terminal, &mut palette);
        assert!(selected(&terminal, &rows).unwrap().starts_with("Keys"));
        assert!(matches!(
            palette.event(key(KeyCode::Enter)),
            Some(Command::Keys)
        ));

        // Theme rows get the same hover feedback.
        let mut themes = Palette::new(Kind::Themes, true);
        render(&mut terminal, &mut themes);
        assert!(themes.event(mouse(moved, 10, 4)).is_none());
        let rows = render(&mut terminal, &mut themes);
        let second = Theme::all().nth(1).unwrap();
        assert_eq!(selected(&terminal, &rows).unwrap(), second.name());
        assert!(matches!(
            themes.event(key(KeyCode::Enter)),
            Some(Command::SetTheme(theme)) if theme == second
        ));

        // Escape from a nested list returns to commands; Escape again closes.
        for (kind, origin) in [(Kind::Themes, "Theme"), (Kind::Keys, "Keys")] {
            let mut palette = Palette::new(kind, true);
            render(&mut terminal, &mut palette);
            assert!(palette.event(key(KeyCode::Esc)).is_none());
            let rows = render(&mut terminal, &mut palette);
            assert!(rows.concat().contains("Command palette"));
            assert!(selected(&terminal, &rows).unwrap().starts_with(origin));
            assert!(terminal.backend().cursor_visible());
            assert!(matches!(
                palette.event(key(KeyCode::Esc)),
                Some(Command::Dismiss)
            ));
        }

        // A long query scrolls so the cursor stays inside the modal.
        let mut palette = Palette::new(Kind::Commands, false);
        for _ in 0..80 {
            palette.event(key(KeyCode::Char('x')));
        }
        let rows = render(&mut terminal, &mut palette);
        assert!(rows.concat().contains("No matches"));
        terminal.backend_mut().assert_cursor_position((57, 2));
        // Too small for a query row: no stray cursor.
        let mut tiny = Terminal::new(TestBackend::new(3, 3)).unwrap();
        render(&mut tiny, &mut palette);
        assert!(!tiny.backend().cursor_visible());
    }
}
