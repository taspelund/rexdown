use std::{
    fs,
    ops::Range,
    path::PathBuf,
    time::{Duration, Instant},
};

use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind};
use ratatui::{
    Frame,
    layout::{Position, Rect},
    style::{Color, Modifier, Style},
    widgets::{Block, Clear},
};
use unicode_width::UnicodeWidthStr;

use crate::{
    markdown::{self, Document, Renderer, Row},
    palette::{Command, Kind, Palette},
    revision, screenshot,
    theme::Theme,
};

pub struct App {
    path: PathBuf,
    pub screenshot_directory: Option<PathBuf>,
    source: Option<String>,
    pending_source: Option<String>,
    updated_at: Option<time::OffsetDateTime>,
    has_updates: bool,
    changes: Vec<Range<usize>>,
    change_rows: Vec<Range<usize>>,
    change_cursor: Option<usize>,
    error: Option<String>,
    notice: Option<(String, Instant)>,
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
    footer: Vec<(Rect, Command)>,
    pointer: Option<Position>,
    screenshot_requested: bool,
}

impl App {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            screenshot_directory: None,
            source: None,
            pending_source: None,
            updated_at: None,
            has_updates: false,
            changes: Vec::new(),
            change_rows: Vec::new(),
            change_cursor: None,
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
            footer: Vec::new(),
            pointer: None,
            screenshot_requested: false,
        }
    }

    pub fn reload(&mut self) {
        match fs::read_to_string(&self.path) {
            Ok(source) => {
                self.error = None;
                if self.pending_source.as_ref().or(self.source.as_ref()) != Some(&source) {
                    self.has_updates |= self.updated_at.is_some();
                    self.updated_at = Some(time::OffsetDateTime::now_utc());
                    self.pending_source = Some(source);
                    self.dirty = true;
                }
            }
            Err(error) => self.error = Some(error.to_string()),
        }
    }

    pub fn draw(&mut self, frame: &mut Frame) {
        if self
            .notice
            .as_ref()
            .is_some_and(|(_, until)| Instant::now() >= *until)
        {
            self.notice = None;
        }
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
            self.render_document();
        }
        self.clamp();
        self.draw_header(frame);
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
            if body.width > 0
                && self
                    .change_rows
                    .iter()
                    .any(|r| r.contains(&(self.scroll + i)))
            {
                paint(
                    frame,
                    &markdown::plain("▎", self.theme.style().fg(self.theme.accent()), None),
                    Rect::new(body.x - 1, body.y + i as u16, 1, 1),
                    0,
                );
            }
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
                let entry = Rect::new(self.toc.x, self.toc.y + 1 + i as u16, toc_width, 1);
                let style = if self.palette.is_none()
                    && self.pointer.is_some_and(|point| entry.contains(point))
                {
                    let style = self.theme.bar_style();
                    frame.render_widget(Block::default().style(style), entry);
                    style
                } else {
                    self.theme.resolve(Style::default().fg(Color::LightBlue))
                };
                let label = format!(" {}{}", " ".repeat(h.level.saturating_sub(1)), h.title);
                paint(
                    frame,
                    &markdown::plain(&label, style, None),
                    Rect::new(entry.x, entry.y, entry.width.saturating_sub(1), 1),
                    0,
                );
            }
        }
        self.draw_footer(frame);
        if let Some((notice, _)) = &self.notice
            && area.height > 2
        {
            // Overlay the last body row, without reflowing or hiding controls.
            let notice_area = Rect::new(area.x, area.bottom() - 2, area.width, 1);
            let style = self.theme.bar_style();
            frame.render_widget(Clear, notice_area);
            frame.render_widget(Block::default().style(style), notice_area);
            paint(
                frame,
                &markdown::plain(&format!(" {notice}"), style, None),
                notice_area,
                0,
            );
        }
        if let Some(palette) = &mut self.palette {
            palette.draw(frame, self.theme);
        }
        if std::mem::take(&mut self.screenshot_requested) {
            self.notify(
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

    fn render_document(&mut self) {
        let mut anchor = self.document.row_sources.get(self.scroll).map(|range| {
            let first = self
                .document
                .row_sources
                .partition_point(|r| r.start < range.start);
            (range.start, self.scroll - first)
        });
        if let Some(source) = self.pending_source.take() {
            if let Some(old) = &self.source {
                let (mapped, changes) = revision::compare(old, &source, anchor.map_or(0, |a| a.0));
                if let Some(a) = &mut anchor {
                    a.0 = mapped;
                }
                self.changes = changes;
            }
            self.source = Some(source);
            self.change_cursor = None;
        }
        self.document = self.renderer.render(
            self.source.as_deref().unwrap_or(""),
            usize::from(self.body.width),
        );
        for glyph in self.document.rows.iter_mut().flatten() {
            glyph.style = self.theme.resolve(glyph.style);
        }
        if let Some((byte, offset)) = anchor {
            let first = self.row_at(byte);
            let count = self
                .document
                .row_sources
                .iter()
                .skip(first)
                .take_while(|r| Some(*r) == self.document.row_sources.get(first))
                .count();
            self.scroll = first + offset.min(count.saturating_sub(1));
        }
        let mut marked = vec![false; self.document.rows.len()];
        for change in &self.changes {
            let mut found = false;
            for (i, source) in self.document.row_sources.iter().enumerate() {
                if source.start < change.end
                    && change.start < source.end
                    && !self.document.rows[i].is_empty()
                {
                    marked[i] = true;
                    found = true;
                }
            }
            // Removed text and edits to invisible Markdown point to the next
            // surviving row (or the final row for a deletion at EOF).
            if !found && !marked.is_empty() {
                marked[self.row_at(change.start)] = true;
            }
        }
        self.change_rows.clear();
        for (row, marked) in marked.into_iter().enumerate() {
            if marked {
                if let Some(last) = self.change_rows.last_mut()
                    && last.end == row
                {
                    last.end += 1;
                } else {
                    self.change_rows.push(row..row + 1);
                }
            }
        }
        self.dirty = false;
    }

    fn row_at(&self, byte: usize) -> usize {
        self.document
            .row_sources
            .iter()
            .position(|r| r.end > byte)
            .unwrap_or_else(|| self.document.rows.len().saturating_sub(1))
    }

    fn draw_header(&self, frame: &mut Frame) {
        let area = frame.area();
        let header = Rect::new(area.x, area.y, area.width, area.height.min(1));
        let style = self.theme.bar_style();
        frame.render_widget(Block::default().style(style), header);
        if let Some(error) = &self.error {
            let message = format!(" Cannot read file · retrying · {error}");
            paint(
                frame,
                &markdown::plain(&message, style.fg(self.theme.error()), None),
                header,
                0,
            );
            return;
        }
        let mut status = "LIVE".to_string();
        if let Some(at) = self.updated_at {
            let label = if self.has_updates {
                "Updated"
            } else {
                "Loaded"
            };
            if area.width >= 60 {
                status.push_str(&format!(
                    " · {label} {:02}:{:02}:{:02}Z",
                    at.hour(),
                    at.minute(),
                    at.second()
                ));
            } else {
                status.push_str(&format!(" · {:02}:{:02}Z", at.hour(), at.minute()));
            }
        }
        if !self.change_rows.is_empty() && area.width >= 60 {
            status.push_str(&format!(
                " · {} change{}",
                self.change_rows.len(),
                if self.change_rows.len() == 1 { "" } else { "s" }
            ));
        }
        if area.width >= 80 {
            let count = self.document.rows.len();
            status.push_str(&format!(
                " · {}–{}/{count}",
                if count == 0 { 0 } else { self.scroll + 1 },
                (self.scroll + usize::from(self.body.height)).min(count)
            ));
        }
        status.push(' ');
        let status_width = (status.width().min(usize::from(area.width))) as u16;
        let name = format!(
            " {}",
            self.path.file_name().unwrap_or_default().to_string_lossy()
        );
        paint(
            frame,
            &markdown::plain(&name, style.add_modifier(Modifier::BOLD), None),
            Rect::new(
                header.x,
                header.y,
                header.width.saturating_sub(status_width + 1),
                header.height,
            ),
            0,
        );
        paint(
            frame,
            &markdown::plain(&status, style, None),
            Rect::new(
                header.right() - status_width,
                header.y,
                status_width,
                header.height,
            ),
            0,
        );
    }

    fn draw_footer(&mut self, frame: &mut Frame) {
        self.footer.clear();
        let area = frame.area();
        if area.height < 2 {
            return;
        }
        let footer = Rect::new(area.x, area.bottom() - 1, area.width, 1);
        let style = self.theme.bar_style();
        frame.render_widget(Block::default().style(style), footer);
        let mut x = footer.x;
        for (key, label, short, command) in [
            ("Ctrl+P", "commands", "menu", Command::Commands),
            ("t", "contents", "toc", Command::Contents),
            ("r", "refresh", "reload", Command::Refresh),
            ("[", "prev change", "prev", Command::PreviousChange),
            ("]", "next change", "next", Command::NextChange),
            ("?", "keys", "help", Command::Keys),
            ("q", "quit", "quit", Command::Quit),
        ] {
            if matches!(command, Command::PreviousChange | Command::NextChange)
                && (self.change_rows.is_empty() || area.width < 70)
            {
                continue;
            }
            if matches!(command, Command::Refresh) && area.width < 50 {
                continue;
            }
            let label = if area.width < 90 { short } else { label };
            let width = (key.width() + label.width() + 3) as u16;
            if x + width > footer.right() {
                continue;
            }
            let hit = Rect::new(x, footer.y, width, 1);
            let hovered = self.pointer.is_some_and(|p| hit.contains(p));
            let style = if hovered {
                style.add_modifier(Modifier::UNDERLINED)
            } else {
                style
            };
            let mut row = markdown::plain(
                &format!(" {key} "),
                style.add_modifier(Modifier::BOLD),
                None,
            );
            row.extend(markdown::plain(&format!("{label} "), style, None));
            paint(frame, &row, hit, 0);
            self.footer.push((hit, command));
            x += width;
        }
    }

    fn notify(&mut self, message: String) {
        self.notice = Some((message, Instant::now() + Duration::from_secs(8)));
    }

    fn jump_change(&mut self, forward: bool) {
        self.notice = None;
        let count = self.change_rows.len();
        if count == 0 {
            self.notify("No changed text to navigate".to_owned());
            return;
        }
        let index = match (self.change_cursor, forward) {
            (Some(i), true) => (i + 1) % count,
            (Some(i), false) => (i + count - 1) % count,
            (None, true) => self
                .change_rows
                .iter()
                .position(|r| r.start > self.scroll)
                .unwrap_or(0),
            (None, false) => self
                .change_rows
                .iter()
                .rposition(|r| r.start < self.scroll)
                .unwrap_or(count - 1),
        };
        self.change_cursor = Some(index);
        self.scroll = self.change_rows[index].start;
        self.horizontal = 0;
        self.clamp();
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
        self.notice = None;
        self.change_cursor = None;
        self.scroll = self.scroll.saturating_add_signed(amount);
        self.clamp();
    }

    pub fn event(&mut self, event: Event) -> bool {
        if let Event::Key(key) = &event {
            if key.kind == KeyEventKind::Release {
                return true;
            }
            self.pointer = None;
            if key.modifiers.contains(KeyModifiers::CONTROL) {
                match key.code {
                    KeyCode::Char('c') => return false,
                    KeyCode::Char('p') => {
                        self.palette = if self.palette.is_some() {
                            None
                        } else {
                            Some(Palette::new(Kind::Commands, false))
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
                    KeyCode::Esc => self.notice = None,
                    KeyCode::Char('?') => return self.command(Command::Keys),
                    KeyCode::Char('[') => self.jump_change(false),
                    KeyCode::Char(']') => self.jump_change(true),
                    KeyCode::Char('t') => self.show_toc = !self.show_toc,
                    KeyCode::Char('r') => return self.command(Command::Refresh),
                    KeyCode::Char('j') | KeyCode::Down => self.scroll_by(1),
                    KeyCode::Char('k') | KeyCode::Up => self.scroll_by(-1),
                    KeyCode::Char('d') => self.scroll_by(half),
                    KeyCode::Char('u') => self.scroll_by(-half),
                    KeyCode::PageDown | KeyCode::Char(' ') => {
                        self.scroll_by(self.body.height as isize)
                    }
                    KeyCode::PageUp => self.scroll_by(-(self.body.height as isize)),
                    KeyCode::Char('g') | KeyCode::Home => {
                        self.notice = None;
                        self.change_cursor = None;
                        self.scroll = 0;
                        self.horizontal = 0;
                    }
                    KeyCode::Char('G') | KeyCode::End => {
                        self.notice = None;
                        self.change_cursor = None;
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
                self.pointer = Some(point);
                if mouse.kind == MouseEventKind::Down(MouseButton::Left) {
                    if let Some((_, command)) =
                        self.footer.iter().find(|(rect, _)| rect.contains(point))
                    {
                        return self.command(*command);
                    }
                    if self.notice.is_some() && mouse.row == self.body.bottom().saturating_sub(1) {
                        self.notice = None;
                        return true;
                    }
                }
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
                            self.notice = None;
                            self.change_cursor = None;
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
        let from_commands = self.palette.take().is_some();
        match command {
            Command::Commands => self.palette = Some(Palette::new(Kind::Commands, false)),
            Command::Theme => self.palette = Some(Palette::new(Kind::Themes, from_commands)),
            Command::Keys => self.palette = Some(Palette::new(Kind::Keys, from_commands)),
            Command::Quit => return false,
            Command::Screenshot => {
                self.notice = None;
                self.screenshot_requested = true;
            }
            Command::Refresh => {
                self.reload();
                if self.error.is_none() {
                    self.notify(
                        if self.pending_source.is_some() {
                            "Refreshed"
                        } else {
                            "Already up to date"
                        }
                        .to_owned(),
                    );
                }
            }
            Command::PreviousChange => self.jump_change(false),
            Command::NextChange => self.jump_change(true),
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
                self.notice = None;
                self.change_cursor = None;
                self.scroll = h.row;
                self.horizontal = 0;
                self.clamp();
            } else {
                self.notify(format!("Heading not found: {target}"));
            }
        } else {
            self.notify(format!("Link (read-only): {target}"));
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
