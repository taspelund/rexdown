use std::{fs, path::PathBuf};

use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind};
use ratatui::{
    Frame,
    layout::{Position, Rect},
    style::{Color, Style},
    widgets::Block,
};
use unicode_width::UnicodeWidthStr;

use crate::{
    markdown::{self, Document, Renderer, Row},
    palette::{Command, Kind, Palette},
    screenshot,
    theme::Theme,
};

pub struct App {
    path: PathBuf,
    pub screenshot_directory: Option<PathBuf>,
    source: Option<String>,
    error: Option<String>,
    notice: Option<String>,
    renderer: Renderer,
    document: Document,
    dirty: bool,
    body: Rect,
    toc: Rect,
    show_toc: bool,
    toc_offset: usize,
    scroll: usize,
    horizontal: usize,
    theme: Theme,
    palette: Option<Palette>,
    screenshot_requested: bool,
}

impl App {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            screenshot_directory: None,
            source: None,
            error: None,
            notice: None,
            renderer: Renderer::new(),
            document: Document::default(),
            dirty: true,
            body: Rect::default(),
            toc: Rect::default(),
            show_toc: false,
            toc_offset: 0,
            scroll: 0,
            horizontal: 0,
            theme: Theme::default(),
            palette: None,
            screenshot_requested: false,
        }
    }

    pub fn reload(&mut self) {
        match fs::read_to_string(&self.path) {
            Ok(source) => {
                self.error = None;
                if self.source.as_ref() != Some(&source) {
                    self.source = Some(source);
                    self.dirty = true;
                }
            }
            Err(error) => self.error = Some(error.to_string()),
        }
    }

    pub fn draw(&mut self, frame: &mut Frame) {
        let area = frame.area();
        frame.render_widget(Block::default().style(self.theme.style()), area);
        let toc_width = if self.show_toc {
            (area.width / 3).min(30)
        } else {
            0
        };
        let body = Rect::new(
            area.x + toc_width + area.width.min(1),
            area.y + area.height.min(1),
            area.width.saturating_sub(toc_width + 2),
            area.height.saturating_sub(2),
        );
        self.toc = Rect::new(area.x, body.y, toc_width, body.height);
        if body.width != self.body.width {
            self.dirty = true;
        }
        self.body = body;
        if self.dirty {
            self.document = self.renderer.render(
                self.source.as_deref().unwrap_or(""),
                usize::from(body.width),
            );
            for glyph in self.document.rows.iter_mut().flatten() {
                glyph.style = self.theme.resolve(glyph.style);
            }
            self.dirty = false;
        }
        self.clamp();
        let name = self.path.file_name().unwrap_or_default().to_string_lossy();
        let status = if let Some(error) = &self.error {
            format!("Cannot read file · retrying · {name}: {error}")
        } else {
            format!("{name} · LIVE")
        };
        let bar = self.theme.bar_style().fg(if self.error.is_some() {
            self.theme.error()
        } else {
            self.theme.foreground()
        });
        let header = Rect::new(area.x, area.y, area.width, area.height.min(1));
        frame.render_widget(Block::default().style(bar), header);
        paint(frame, &markdown::plain(&status, bar, None), header, 0);
        for (i, row) in self
            .document
            .rows
            .iter()
            .skip(self.scroll)
            .take(usize::from(body.height))
            .enumerate()
        {
            paint(
                frame,
                row,
                Rect::new(body.x, body.y + i as u16, body.width, 1),
                self.horizontal,
            );
        }
        if toc_width > 0 {
            paint(
                frame,
                &markdown::plain(" Contents", Style::default().fg(self.theme.accent()), None),
                Rect::new(self.toc.x, self.toc.y, toc_width, self.toc.height.min(1)),
                0,
            );
            for (i, h) in self
                .document
                .headings
                .iter()
                .skip(self.toc_offset)
                .take(usize::from(self.toc.height.saturating_sub(1)))
                .enumerate()
            {
                let label = format!(" {}{}", " ".repeat(h.level.saturating_sub(1)), h.title);
                paint(
                    frame,
                    &markdown::plain(
                        &label,
                        self.theme.resolve(Style::default().fg(Color::LightBlue)),
                        None,
                    ),
                    Rect::new(
                        self.toc.x,
                        self.toc.y + 1 + i as u16,
                        toc_width.saturating_sub(1),
                        1,
                    ),
                    0,
                );
            }
        }
        if area.height > 1 {
            let footer = self.notice.as_deref().unwrap_or(
                "Ctrl+P commands · q quit · t contents · r refresh · d/u half page · ←/→ pan",
            );
            let footer_area = Rect::new(area.x, area.bottom() - 1, area.width, 1);
            frame.render_widget(Block::default().style(bar), footer_area);
            paint(frame, &markdown::plain(footer, bar, None), footer_area, 0);
        }
        if let Some(palette) = &mut self.palette {
            palette.draw(frame, self.theme);
        }
        if std::mem::take(&mut self.screenshot_requested) {
            self.notice = Some(
                match screenshot::save(
                    frame.buffer_mut(),
                    &self.path,
                    self.screenshot_directory.as_deref(),
                ) {
                    Ok(path) => format!("Saved screenshot: {}", path.display()),
                    Err(error) => format!("Cannot save screenshot: {error}"),
                },
            );
        }
    }

    fn clamp(&mut self) {
        self.scroll = self.scroll.min(
            self.document
                .rows
                .len()
                .saturating_sub(usize::from(self.body.height)),
        );
        let width = self
            .document
            .rows
            .iter()
            .map(|r| markdown::width(r))
            .max()
            .unwrap_or(0);
        self.horizontal = self
            .horizontal
            .min(width.saturating_sub(usize::from(self.body.width)));
        self.toc_offset = self.toc_offset.min(
            self.document
                .headings
                .len()
                .saturating_sub(usize::from(self.toc.height.saturating_sub(1))),
        );
    }

    fn scroll_by(&mut self, amount: isize) {
        self.scroll = self.scroll.saturating_add_signed(amount);
        self.clamp();
    }

    pub fn event(&mut self, event: Event) -> bool {
        if let Event::Key(key) = &event {
            if key.kind == KeyEventKind::Release {
                return true;
            }
            self.notice = None;
            if key.modifiers.contains(KeyModifiers::CONTROL) {
                match key.code {
                    KeyCode::Char('c') => return false,
                    KeyCode::Char('p') => {
                        self.palette = if self.palette.is_some() {
                            None
                        } else {
                            Some(Palette::new(Kind::Commands))
                        };
                        return true;
                    }
                    _ => {}
                }
            }
        }
        if let Some(palette) = &mut self.palette {
            let command = palette.event(event);
            return command.is_none_or(|command| self.command(command));
        }
        match event {
            Event::Key(key) if key.kind != KeyEventKind::Release => {
                let half = (self.body.height / 2).max(1) as isize;
                match key.code {
                    KeyCode::Char('q') => return false,
                    KeyCode::Char('t') => self.show_toc = !self.show_toc,
                    KeyCode::Char('r') => self.reload(),
                    KeyCode::Char('j') | KeyCode::Down => self.scroll_by(1),
                    KeyCode::Char('k') | KeyCode::Up => self.scroll_by(-1),
                    KeyCode::Char('d') => self.scroll_by(half),
                    KeyCode::Char('u') => self.scroll_by(-half),
                    KeyCode::PageDown | KeyCode::Char(' ') => {
                        self.scroll_by(self.body.height as isize)
                    }
                    KeyCode::PageUp => self.scroll_by(-(self.body.height as isize)),
                    KeyCode::Char('g') | KeyCode::Home => {
                        self.scroll = 0;
                        self.horizontal = 0;
                    }
                    KeyCode::Char('G') | KeyCode::End => {
                        self.scroll = self.document.rows.len();
                        self.clamp();
                    }
                    KeyCode::Left | KeyCode::Char('h') => {
                        self.horizontal = self.horizontal.saturating_sub(4)
                    }
                    KeyCode::Right | KeyCode::Char('l') => {
                        self.horizontal = self.horizontal.saturating_add(4);
                        self.clamp();
                    }
                    _ => {}
                }
            }
            Event::Mouse(mouse) => {
                let point = Position::new(mouse.column, mouse.row);
                let in_toc = self.toc.contains(point);
                match mouse.kind {
                    MouseEventKind::ScrollDown | MouseEventKind::ScrollUp => {
                        let amount = if mouse.kind == MouseEventKind::ScrollDown {
                            3
                        } else {
                            -3
                        };
                        if in_toc {
                            self.toc_offset = self.toc_offset.saturating_add_signed(amount);
                            self.clamp();
                        } else {
                            self.scroll_by(amount);
                        }
                    }
                    MouseEventKind::ScrollRight => {
                        self.horizontal = self.horizontal.saturating_add(4);
                        self.clamp();
                    }
                    MouseEventKind::ScrollLeft => {
                        self.horizontal = self.horizontal.saturating_sub(4)
                    }
                    MouseEventKind::Down(MouseButton::Left) if in_toc && mouse.row > self.toc.y => {
                        if let Some(h) = self
                            .document
                            .headings
                            .get(self.toc_offset + usize::from(mouse.row - self.toc.y - 1))
                        {
                            self.scroll = h.row;
                            self.horizontal = 0;
                            self.clamp();
                        }
                    }
                    MouseEventKind::Down(MouseButton::Left) if self.body.contains(point) => {
                        let y = self.scroll + usize::from(mouse.row - self.body.y);
                        let x = self.horizontal + usize::from(mouse.column - self.body.x);
                        let mut column = 0;
                        let link = self
                            .document
                            .rows
                            .get(y)
                            .and_then(|row| {
                                row.iter().find(|g| {
                                    column += g.text.width();
                                    column > x
                                })
                            })
                            .and_then(|g| g.link);
                        if let Some(link) = link {
                            self.follow_link(link);
                        }
                    }
                    _ => {}
                }
            }
            _ => {}
        }
        true
    }

    fn command(&mut self, command: Command) -> bool {
        self.palette = None;
        match command {
            Command::Theme => self.palette = Some(Palette::new(Kind::Themes)),
            Command::Keys => self.palette = Some(Palette::new(Kind::Keys)),
            Command::Quit => return false,
            Command::Screenshot => self.screenshot_requested = true,
            Command::Refresh => self.reload(),
            Command::Contents => self.show_toc = !self.show_toc,
            Command::SetTheme(theme) => {
                self.theme = theme;
                self.renderer.theme = theme;
                self.dirty = true;
            }
            Command::Dismiss => {}
        }
        true
    }

    fn follow_link(&mut self, link: usize) {
        let target = &self.document.links[link];
        if let Some(anchor) = target.strip_prefix('#') {
            if let Some(h) = self.document.headings.iter().find(|h| h.anchor == anchor) {
                self.scroll = h.row;
                self.horizontal = 0;
                self.clamp();
            } else {
                self.notice = Some(format!("Heading not found: {target}"));
            }
        } else {
            self.notice = Some(format!("Link (read-only): {target}"));
        }
    }
}

fn paint(frame: &mut Frame, row: &Row, area: Rect, offset: usize) {
    if area.is_empty() {
        return;
    }
    let mut column = 0;
    for g in row {
        let w = g.text.width();
        if column >= offset && column + w <= offset + usize::from(area.width) {
            frame.buffer_mut().set_stringn(
                area.x + (column - offset) as u16,
                area.y,
                &g.text,
                w,
                g.style,
            );
        }
        column += w;
        if column >= offset + usize::from(area.width) {
            break;
        }
    }
}

#[cfg(test)]
mod tests;
