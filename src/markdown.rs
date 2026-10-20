use std::{collections::HashSet, ops::Range};

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
    /// Source provenance for each rendered row, exactly parallel to `rows`.
    ///
    /// `row_sources[i]` is the byte range of the logical source block that
    /// produced `rows[i]`. Ranges are non-empty, lie within the rendered
    /// source, and start and end on UTF-8 character boundaries. Every row of
    /// one block (including wrapped continuations) carries the same range, so
    /// the set of ranges does not depend on the render width.
    ///
    /// - Paragraphs, headings, raw HTML blocks, and rules use the block range.
    /// - A list item's own line uses the range from its marker through its
    ///   own text (excluding nested blocks); a loose item's first paragraph
    ///   extends from the marker through that paragraph.
    /// - Code blocks use one range per source line (indentation stripped by
    ///   the containing list or quote is not included).
    /// - Tables use one range per header or body row; the header separator
    ///   uses the header range.
    /// - Inserted blank rows repeat the preceding row's range.
    pub row_sources: Vec<Range<usize>>,
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
            src: 0..0,
            block_row: 0,
            open: false,
        };
        let mut heading = None;
        let mut anchors = HashSet::new();
        let mut code: Option<(String, String, Vec<CodeText>)> = None;
        let mut table: Option<Table> = None;
        let options = Options::ENABLE_TABLES
            | Options::ENABLE_STRIKETHROUGH
            | Options::ENABLE_TASKLISTS
            | Options::ENABLE_HEADING_ATTRIBUTES;
        for (event, range) in Parser::new_ext(source, options).into_offset_iter() {
            // Provenance comes only from parser offsets: block starts open a
            // new logical block, inline events widen the current one.
            match &event {
                Event::Start(
                    Tag::Paragraph | Tag::Heading { .. } | Tag::HtmlBlock | Tag::Table(_),
                ) => {
                    if matches!(event, Event::Start(Tag::Heading { .. } | Tag::Table(_))) {
                        b.flush();
                    }
                    b.begin(range.clone());
                }
                Event::Start(Tag::List(_) | Tag::BlockQuote(_))
                | Event::End(
                    TagEnd::Paragraph
                    | TagEnd::Heading(_)
                    | TagEnd::Item
                    | TagEnd::List(_)
                    | TagEnd::BlockQuote(_)
                    | TagEnd::CodeBlock
                    | TagEnd::Table
                    | TagEnd::HtmlBlock,
                ) => b.open = false,
                Event::Start(Tag::Item | Tag::CodeBlock(_)) | Event::Rule => {}
                _ if code.is_none() => b.inline(range.clone()),
                _ => {}
            }
            match event {
                Event::Start(Tag::Heading { level, id, .. }) => {
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
                    // The item's own row covers its marker line; inline text
                    // and a loose first paragraph widen it, nested blocks don't.
                    let line_end = source[range.clone()]
                        .find('\n')
                        .map_or(range.end, |n| range.start + n);
                    let end = if line_end > range.start {
                        line_end
                    } else {
                        range.end
                    };
                    b.begin(range.start..end);
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
                    b.begin(range);
                    let language = match kind {
                        CodeBlockKind::Fenced(s) => s.into_string(),
                        _ => String::new(),
                    };
                    code = Some((language, String::new(), Vec::new()));
                }
                Event::End(TagEnd::CodeBlock) => {
                    if let Some((language, text, pieces)) = code.take() {
                        self.code(&mut b, &language, &text, source, &pieces);
                    }
                    b.blank();
                }
                Event::Start(Tag::Table(alignments)) => {
                    b.flush();
                    table = Some(Table {
                        alignments,
                        rows: Vec::new(),
                        cells: Vec::new(),
                        sources: Vec::new(),
                    });
                }
                Event::Start(Tag::TableHead | Tag::TableRow) => {
                    if let Some(t) = &mut table {
                        t.sources.push(range);
                    }
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
                    if let Some((_, text, pieces)) = &mut code {
                        pieces.push(CodeText {
                            at: text.len(),
                            len: s.len(),
                            exact: source.get(range.clone()) == Some(&*s),
                            range,
                        });
                        text.push_str(&s);
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
                    b.begin(range);
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
            b.doc.row_sources.pop();
        }
        debug_assert_eq!(b.doc.rows.len(), b.doc.row_sources.len());
        b.doc
    }

    fn code(
        &self,
        b: &mut Builder,
        language: &str,
        source: &str,
        document: &str,
        pieces: &[CodeText],
    ) {
        let token = language.split_whitespace().next().unwrap_or("");
        let syntax = self
            .syntaxes
            .find_syntax_by_token(token)
            .unwrap_or_else(|| self.syntaxes.find_syntax_plain_text());
        let mut highlighter = HighlightLines::new(syntax, self.theme.data());
        let mut at = 0;
        for line in source.split_inclusive('\n') {
            let span = at..at + line.len();
            at = span.end;
            let mut origin: Option<Range<usize>> = None;
            for p in pieces {
                let (start, end) = (span.start.max(p.at), span.end.min(p.at + p.len));
                if start >= end {
                    continue;
                }
                let part = if p.exact {
                    p.range.start + start - p.at..p.range.start + end - p.at
                } else {
                    p.range.clone()
                };
                origin = Some(match origin {
                    Some(o) => o.start.min(part.start)..o.end.max(part.end),
                    None => part,
                });
            }
            let origin = origin
                .filter(|r| {
                    r.start < r.end
                        && document.is_char_boundary(r.start)
                        && document.is_char_boundary(r.end)
                        && r.end <= document.len()
                })
                .unwrap_or_else(|| b.src.clone());
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
            b.push_row(row, origin);
        }
    }
}

/// A code block text event: its offset in the collected code text and the
/// source range the parser reported for it.
struct CodeText {
    at: usize,
    len: usize,
    exact: bool,
    range: Range<usize>,
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
    /// Source range of the logical block currently being rendered.
    src: Range<usize>,
    /// First row produced by the current block.
    block_row: usize,
    /// Whether inline content still belongs to the current block.
    open: bool,
}

impl Builder {
    /// Starts a block, or widens the current one when its row is still
    /// pending (a list marker followed by a paragraph or HTML block).
    fn begin(&mut self, range: Range<usize>) {
        if self.line.is_empty() {
            self.src = range;
            self.block_row = self.doc.rows.len();
        } else {
            self.widen(range);
        }
        self.open = true;
    }
    fn inline(&mut self, range: Range<usize>) {
        if self.open {
            self.widen(range);
        } else {
            self.begin(range);
        }
    }
    fn widen(&mut self, range: Range<usize>) {
        if range.start < self.src.start || range.end > self.src.end {
            self.src = self.src.start.min(range.start)..self.src.end.max(range.end);
            let src = self.src.clone();
            for s in &mut self.doc.row_sources[self.block_row..] {
                *s = src.clone();
            }
        }
    }
    fn push_row(&mut self, row: Row, src: Range<usize>) {
        self.doc.rows.push(row);
        self.doc.row_sources.push(src);
    }
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
            for row in wrap(
                std::mem::take(&mut self.line),
                self.available,
                self.indent + self.quote * 2,
            ) {
                self.push_row(row, self.src.clone());
            }
        }
    }
    fn blank(&mut self) {
        if self.doc.rows.last().is_some_and(|r| !r.is_empty()) {
            let src = self.doc.row_sources.last().cloned().unwrap_or_default();
            self.push_row(Vec::new(), src);
        }
    }
}

struct Table {
    alignments: Vec<Alignment>,
    rows: Vec<Vec<Row>>,
    cells: Vec<Row>,
    /// Source range of each header/body row, parallel to `rows`.
    sources: Vec<Range<usize>>,
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
        let fallback = b.src.clone();
        for (i, row) in self.rows.into_iter().enumerate() {
            let src = self.sources.get(i).cloned().unwrap_or(fallback.clone());
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
                b.push_row(out, src.clone());
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
                b.push_row(plain(&bar, Style::default().fg(Color::DarkGray), None), src);
            }
        }
    }
}
