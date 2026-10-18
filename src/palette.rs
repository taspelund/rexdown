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
    Theme,
    Quit,
    Keys,
    Screenshot,
    Refresh,
    Contents,
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
    ("Contents — toggle heading list", Command::Contents),
];
const KEYS: &[&str] = &[
    "Ctrl+P       Command palette",
    "q / Ctrl+C   Quit viewer",
    "t            Toggle contents",
    "r            Refresh file",
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
    "Click        Heading / inspect link",
    "Palette: type to filter; Up/Down to select",
    "Enter        Run selected command",
    "Esc          Close palette or key help",
];

impl Palette {
    pub fn new(kind: Kind) -> Self {
        Self {
            kind,
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
            // Keep the end of a long query visible without slicing UTF-8 bytes.
            let offset = unicode_width::UnicodeWidthStr::width(prompt.as_str())
                .saturating_sub(usize::from(inner.width));
            let mut line = prompt.as_str();
            let mut skipped = 0;
            while skipped < offset {
                let Some(c) = line.chars().next() else {
                    break;
                };
                skipped += unicode_width::UnicodeWidthChar::width(c).unwrap_or(0);
                line = &line[c.len_utf8()..];
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
            let help = if self.kind == Kind::Keys {
                "↑/↓ scroll · Esc close"
            } else {
                "↑/↓ select · Enter run · Esc close"
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
                KeyCode::Esc => return Some(Command::Dismiss),
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
                MouseEventKind::Down(MouseButton::Left)
                    if self.kind != Kind::Keys
                        && self
                            .items_area
                            .contains(Position::new(mouse.column, mouse.row)) =>
                {
                    return entries
                        .get(self.offset + usize::from(mouse.row - self.items_area.y))
                        .map(|(_, c)| *c);
                }
                _ => {}
            },
            _ => {}
        }
        None
    }
}
