use std::collections::HashSet;

use pulldown_cmark::{Alignment, CodeBlockKind, Event, Options, Parser, Tag, TagEnd};
use ratatui::style::{Color, Modifier, Style};
use syntect::{easy::HighlightLines, parsing::SyntaxSet};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use crate::theme::Theme;

#[cfg(test)]
mod tests;

#[derive(Clone)]
pub struct Glyph {
    pub text: String,
    pub style: Style,
    pub link: Option<usize>,
}

pub type Row = Vec<Glyph>;

pub fn text(row: &Row) -> String {
    row.iter().map(|g| g.text.as_str()).collect()
}

pub fn width(row: &[Glyph]) -> usize {
    row.iter().map(|g| g.text.width()).sum()
}

pub fn plain(value: &str, style: Style, link: Option<usize>) -> Row {
    // Never let document-supplied terminal controls reach the backend.
    let value: String = value
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    value
        .graphemes(true)
        .map(|g| Glyph {
            text: g.to_owned(),
            style,
            link,
        })
        .collect()
}

fn wrap(mut row: Row, available: usize, indent: usize) -> Vec<Row> {
    let mut rows = Vec::new();
    let available = available.max(1);
    while width(&row) > available {
        let mut used = 0;
        let end = row
            .iter()
            .position(|g| {
                used += g.text.width();
                used > available
            })
            .unwrap_or(row.len())
            .max(1);
        let split = row[..end]
            .iter()
            .rposition(|g| g.text == " ")
            .filter(|&n| n > indent)
            .unwrap_or(end);
        let mut rest = row.split_off(split);
        while rest.first().is_some_and(|g| g.text == " ") {
            rest.remove(0);
        }
        rows.push(row);
        row = plain(
            &" ".repeat(indent.min(available.saturating_sub(2))),
            Style::default(),
            None,
        );
        row.extend(rest);
    }
    rows.push(row);
    rows
}

pub struct Heading {
    pub title: String,
    pub anchor: String,
    pub level: usize,
    pub row: usize,
}

#[derive(Default)]
pub struct Document {
    pub rows: Vec<Row>,
    pub headings: Vec<Heading>,
    pub links: Vec<String>,
}

pub struct Renderer {
    syntaxes: SyntaxSet,
    pub theme: Theme,
}

impl Renderer {
    pub fn new() -> Self {
        Self {
            syntaxes: SyntaxSet::load_defaults_newlines(),
            theme: Theme::default(),
        }
    }

    pub fn render(&self, source: &str, available: usize) -> Document {
        let mut b = Builder {
            doc: Document::default(),
            line: Vec::new(),
            styles: vec![Style::default()],
            link: None,
            lists: Vec::new(),
            indent: 0,
            quote: 0,
            available: available.max(1),
        };
        let mut heading = None;
        let mut anchors = HashSet::new();
        let mut code: Option<(String, String)> = None;
        let mut table: Option<Table> = None;
        let options = Options::ENABLE_TABLES
            | Options::ENABLE_STRIKETHROUGH
            | Options::ENABLE_TASKLISTS
            | Options::ENABLE_HEADING_ATTRIBUTES;
        for event in Parser::new_ext(source, options) {
            match event {
                Event::Start(Tag::Heading { level, id, .. }) => {
                    b.flush();
                    heading = Some((
                        level as usize,
                        id.map(|s| s.into_string()),
                        b.doc.rows.len(),
                    ));
                    b.push_style(
                        Style::default()
                            .fg(Color::Cyan)
                            .add_modifier(Modifier::BOLD),
                    );
                }
                Event::End(TagEnd::Heading(_)) => {
                    let title = text(&b.line);
                    b.flush();
                    b.styles.pop();
                    if let Some((level, id, row)) = heading.take() {
                        let base = id.unwrap_or_else(|| {
                            title
                                .to_lowercase()
                                .chars()
                                .filter(|c| {
                                    c.is_alphanumeric()
                                        || c.is_whitespace()
                                        || *c == '-'
                                        || *c == '_'
                                })
                                .map(|c| if c.is_whitespace() { '-' } else { c })
                                .collect()
                        });
                        let mut anchor = base.clone();
                        let mut suffix = 1;
                        while !anchors.insert(anchor.clone()) {
                            anchor = format!("{base}-{suffix}");
                            suffix += 1;
                        }
                        b.doc.headings.push(Heading {
                            title,
                            anchor,
                            level,
                            row,
                        });
                    }
                    b.blank();
                }
                Event::Start(Tag::Paragraph) => {}
                Event::End(TagEnd::Paragraph) => {
                    b.flush();
                    b.blank();
                }
                Event::Start(Tag::Strong) => {
                    b.push_style(Style::default().add_modifier(Modifier::BOLD))
                }
                Event::Start(Tag::Emphasis) => {
                    b.push_style(Style::default().add_modifier(Modifier::ITALIC))
                }
                Event::Start(Tag::Strikethrough) => {
                    b.push_style(Style::default().add_modifier(Modifier::CROSSED_OUT))
                }
                Event::End(TagEnd::Strong | TagEnd::Emphasis | TagEnd::Strikethrough) => {
                    b.styles.pop();
                }
                Event::Start(Tag::Link { dest_url, .. }) => {
                    b.link = Some(b.doc.links.len());
                    b.doc.links.push(dest_url.into_string());
                    b.push_style(
                        Style::default()
                            .fg(Color::LightBlue)
                            .add_modifier(Modifier::UNDERLINED),
                    );
                }
                Event::End(TagEnd::Link) => {
                    b.link = None;
                    b.styles.pop();
                }
                Event::Start(Tag::Image { .. }) => b.add("[image: "),
                Event::End(TagEnd::Image) => b.add("]"),
                Event::Start(Tag::List(start)) => {
                    b.flush();
                    b.lists.push(start);
                }
                Event::End(TagEnd::List(_)) => {
                    b.flush();
                    b.lists.pop();
                    b.indent = b.lists.len() * 2;
                    b.blank();
                }
                Event::Start(Tag::Item) => {
                    b.flush();
                    b.indent = b.lists.len().saturating_sub(1) * 2;
                    let marker = match b.lists.last_mut() {
                        Some(Some(n)) => {
                            let s = format!("{n}. ");
                            *n = n.saturating_add(1);
                            s
                        }
                        _ => "• ".to_owned(),
                    };
                    b.add(&marker);
                    b.indent += marker.width();
                }
                Event::End(TagEnd::Item) => b.flush(),
                Event::Start(Tag::BlockQuote(_)) => {
                    b.flush();
                    b.quote += 1;
                }
                Event::End(TagEnd::BlockQuote(_)) => {
                    b.flush();
                    b.quote = b.quote.saturating_sub(1);
                    b.blank();
                }
                Event::Start(Tag::CodeBlock(kind)) => {
                    b.flush();
                    let language = match kind {
                        CodeBlockKind::Fenced(s) => s.into_string(),
                        _ => String::new(),
                    };
                    code = Some((language, String::new()));
                }
                Event::End(TagEnd::CodeBlock) => {
                    if let Some((language, source)) = code.take() {
                        self.code(&mut b, &language, &source);
                    }
                    b.blank();
                }
                Event::Start(Tag::Table(alignments)) => {
                    b.flush();
                    table = Some(Table {
                        alignments,
                        rows: Vec::new(),
                        cells: Vec::new(),
                    });
                }
                Event::End(TagEnd::TableCell) => {
                    if let Some(t) = &mut table {
                        t.cells.push(std::mem::take(&mut b.line));
                    }
                }
                Event::End(TagEnd::TableHead | TagEnd::TableRow) => {
                    if let Some(t) = &mut table {
                        t.rows.push(std::mem::take(&mut t.cells));
                    }
                }
                Event::End(TagEnd::Table) => {
                    if let Some(t) = table.take() {
                        t.render(&mut b);
                    }
                    b.blank();
                }
                Event::Text(s) => {
                    if let Some((_, source)) = &mut code {
                        source.push_str(&s);
                    } else {
                        b.add(&s);
                    }
                }
                Event::Code(s) => {
                    b.push_style(Style::default().fg(Color::LightYellow));
                    b.add(&s);
                    b.styles.pop();
                }
                Event::SoftBreak => b.add(" "),
                Event::HardBreak => b.flush(),
                Event::Rule => {
                    b.flush();
                    b.add(&"─".repeat(b.available.min(72)));
                    b.flush();
                    b.blank();
                }
                Event::TaskListMarker(checked) => b.add(if checked { "[x] " } else { "[ ] " }),
                // HTML is inert text, never terminal markup or executable content.
                Event::Html(s) | Event::InlineHtml(s) => b.add(&s),
                _ => {}
            }
        }
        b.flush();
        while b.doc.rows.last().is_some_and(Vec::is_empty) {
            b.doc.rows.pop();
        }
        b.doc
    }

    fn code(&self, b: &mut Builder, language: &str, source: &str) {
        let token = language.split_whitespace().next().unwrap_or("");
        let syntax = self
            .syntaxes
            .find_syntax_by_token(token)
            .unwrap_or_else(|| self.syntaxes.find_syntax_plain_text());
        let mut highlighter = HighlightLines::new(syntax, self.theme.data());
        for line in source.split_inclusive('\n') {
            let expanded = line.replace('\t', "    ");
            let mut row = plain(
                &format!("{}│ ", " ".repeat(b.indent)),
                Style::default().fg(Color::DarkGray),
                None,
            );
            if let Ok(parts) = highlighter.highlight_line(&expanded, &self.syntaxes) {
                for (style, s) in parts {
                    row.extend(plain(
                        s.trim_end_matches('\n'),
                        Style::default().fg(self.theme.code_color(style.foreground)),
                        None,
                    ));
                }
            } else {
                row.extend(plain(
                    expanded.trim_end_matches('\n'),
                    Style::default(),
                    None,
                ));
            }
            b.doc.rows.push(row);
        }
    }
}

struct Builder {
    doc: Document,
    line: Row,
    styles: Vec<Style>,
    link: Option<usize>,
    lists: Vec<Option<u64>>,
    indent: usize,
    quote: usize,
    available: usize,
}

impl Builder {
    fn push_style(&mut self, style: Style) {
        self.styles
            .push(self.styles.last().copied().unwrap_or_default().patch(style));
    }
    fn add(&mut self, s: &str) {
        if self.line.is_empty() {
            self.line.extend(plain(
                &format!("{}{}", " ".repeat(self.indent), "│ ".repeat(self.quote)),
                Style::default().fg(Color::DarkGray),
                None,
            ));
        }
        self.line.extend(plain(
            s,
            self.styles.last().copied().unwrap_or_default(),
            self.link,
        ));
    }
    fn flush(&mut self) {
        if !self.line.is_empty() {
            self.doc.rows.extend(wrap(
                std::mem::take(&mut self.line),
                self.available,
                self.indent + self.quote * 2,
            ));
        }
    }
    fn blank(&mut self) {
        if self.doc.rows.last().is_some_and(|r| !r.is_empty()) {
            self.doc.rows.push(Vec::new());
        }
    }
}

struct Table {
    alignments: Vec<Alignment>,
    rows: Vec<Vec<Row>>,
    cells: Vec<Row>,
}

impl Table {
    fn render(self, b: &mut Builder) {
        let n = self.alignments.len();
        if n == 0 {
            return;
        }
        let mut widths = vec![1; n];
        for row in &self.rows {
            for (cell, w) in row.iter().zip(&mut widths) {
                *w = (*w).max(width(cell));
            }
        }
        let budget = b.available.saturating_sub(3 * n + 1).max(n);
        while widths.iter().sum::<usize>() > budget {
            if let Some(w) = widths.iter_mut().max() {
                *w -= 1;
            }
        }
        for (i, row) in self.rows.into_iter().enumerate() {
            let cells: Vec<_> = row
                .into_iter()
                .zip(&widths)
                .map(|(c, &w)| wrap(c, w, 0))
                .collect();
            for y in 0..cells.iter().map(Vec::len).max().unwrap_or(0) {
                let mut out = plain("│ ", Style::default().fg(Color::DarkGray), None);
                for (j, &w) in widths.iter().enumerate() {
                    let cell = cells
                        .get(j)
                        .and_then(|c| c.get(y))
                        .cloned()
                        .unwrap_or_default();
                    let padding = w.saturating_sub(width(&cell));
                    let left = match self.alignments[j] {
                        Alignment::Right => padding,
                        Alignment::Center => padding / 2,
                        _ => 0,
                    };
                    out.extend(plain(&" ".repeat(left), Style::default(), None));
                    out.extend(cell.into_iter().map(|mut g| {
                        if i == 0 {
                            g.style = g.style.add_modifier(Modifier::BOLD);
                        }
                        g
                    }));
                    out.extend(plain(
                        &format!("{} │ ", " ".repeat(padding - left)),
                        Style::default().fg(Color::DarkGray),
                        None,
                    ));
                }
                out.pop(); // The final border has no inter-column space after it.
                b.doc.rows.push(out);
            }
            if i == 0 {
                let bar = format!(
                    "├{}┤",
                    widths
                        .iter()
                        .map(|w| "─".repeat(w + 2))
                        .collect::<Vec<_>>()
                        .join("┼")
                );
                b.doc
                    .rows
                    .push(plain(&bar, Style::default().fg(Color::DarkGray), None));
            }
        }
    }
}
