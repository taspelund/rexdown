use std::{
    collections::HashMap,
    fmt::Write as _,
    fs::OpenOptions,
    io::{self, Write},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use ratatui::{
    buffer::Buffer,
    style::{Color, Modifier},
};
use unicode_width::UnicodeWidthStr;

/// Save the rendered viewport without overwriting any existing screenshot.
pub fn save(buffer: &Buffer, document: &Path, directory: Option<&Path>) -> io::Result<PathBuf> {
    let directory = match directory {
        Some(path) => std::env::current_dir()?.join(path),
        None => dirs::picture_dir()
            .or_else(|| dirs::home_dir().map(|home| home.join("Pictures")))
            .ok_or_else(|| io::Error::other("Cannot find Pictures; use --screenshot-dir DIR"))?
            .join("Rexdown"),
    };
    // Only query on export, between event reads, so we get the current palette
    // without competing with the application's single-threaded input loop.
    let deadline = Instant::now() + Duration::from_millis(500);
    let svg = render(buffer, |key| {
        let remaining = deadline
            .saturating_duration_since(Instant::now())
            .as_millis() as u64;
        if remaining == 0 {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "color query timed out",
            ));
        }
        xterm_query::query_osc(&format!("\x1b]{key};?\x07"), remaining).map_err(io::Error::other)
    })
    .map_err(|_| io::Error::other("Terminal colors unavailable; choose a bundled theme for SVG"))?;
    std::fs::create_dir_all(&directory)?;
    let stem = document.file_stem().unwrap_or_default().to_string_lossy();
    let now = time::OffsetDateTime::now_utc();
    let name = format!(
        "{stem}-{}_{:02}-{:02}-{:02}Z",
        now.date(),
        now.hour(),
        now.minute(),
        now.second()
    );
    for suffix in 0_u64.. {
        let filename = if suffix == 0 {
            format!("{name}.svg")
        } else {
            format!("{name}-{suffix}.svg")
        };
        let path = directory.join(filename);
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(mut file) => {
                if let Err(error) = file.write_all(svg.as_bytes()) {
                    let _ = std::fs::remove_file(&path);
                    return Err(error);
                }
                return Ok(path);
            }
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    }
    Err(io::Error::other("No unused screenshot filename"))
}

fn render(
    buffer: &Buffer,
    mut query: impl FnMut(&str) -> io::Result<String>,
) -> io::Result<String> {
    let mut buffer = buffer.clone();
    let mut colors = HashMap::new();
    for row in buffer
        .content
        .chunks_mut(usize::from(buffer.area.width).max(1))
    {
        let mut x = 0;
        while x < row.len() {
            let cell = &mut row[x];
            cell.fg = resolve_color(cell.fg, "10", &mut colors, &mut query)?;
            cell.bg = resolve_color(cell.bg, "11", &mut colors, &mut query)?;
            if cell.modifier.contains(Modifier::REVERSED) {
                std::mem::swap(&mut cell.fg, &mut cell.bg);
            }
            let width = cell.symbol().width().max(1);
            let (fg, bg) = (cell.fg, cell.bg);
            // Ratatui resets continuation cells; the terminal paints them as
            // part of the leading wide glyph, not with default terminal colors.
            for continuation in row.iter_mut().skip(x + 1).take(width - 1) {
                continuation.set_symbol(" ").set_fg(fg).set_bg(bg);
            }
            x += width;
        }
    }
    let mut svg = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{}\" height=\"{}\" viewBox=\"0 0 {} {}\">\n",
        u32::from(buffer.area.width) * 10,
        u32::from(buffer.area.height) * 20,
        u32::from(buffer.area.width) * 10,
        u32::from(buffer.area.height) * 20
    );
    svg.push_str(
        "<g font-family=\"Menlo,Consolas,monospace\" font-size=\"14\" xml:space=\"preserve\">\n",
    );
    // Paint backgrounds first so a wide glyph isn't covered by its continuation cell.
    for y in 0..buffer.area.height {
        for x in 0..buffer.area.width {
            let cell = &buffer[(buffer.area.x + x, buffer.area.y + y)];
            let _ = writeln!(
                svg,
                "<rect x=\"{}\" y=\"{}\" width=\"10\" height=\"20\" fill=\"{}\"/>",
                u32::from(x) * 10,
                u32::from(y) * 20,
                color(cell.bg)
            );
        }
    }
    for y in 0..buffer.area.height {
        for x in 0..buffer.area.width {
            let cell = &buffer[(buffer.area.x + x, buffer.area.y + y)];
            let symbol = cell.symbol();
            if symbol.trim().is_empty() {
                continue;
            }
            let mut attributes = String::new();
            if cell.modifier.contains(Modifier::BOLD) {
                attributes.push_str(" font-weight=\"bold\"");
            }
            if cell.modifier.contains(Modifier::ITALIC) {
                attributes.push_str(" font-style=\"italic\"");
            }
            let mut decoration = Vec::new();
            if cell.modifier.contains(Modifier::UNDERLINED) {
                decoration.push("underline");
            }
            if cell.modifier.contains(Modifier::CROSSED_OUT) {
                decoration.push("line-through");
            }
            if !decoration.is_empty() {
                let _ = write!(attributes, " text-decoration=\"{}\"", decoration.join(" "));
            }
            let escaped = symbol
                // These Unicode scalars are legal in UTF-8 but not XML 1.0.
                .replace(['\u{fffe}', '\u{ffff}'], "\u{fffd}")
                .replace('&', "&amp;")
                .replace('<', "&lt;")
                .replace('>', "&gt;")
                .replace('"', "&quot;")
                .replace('\'', "&apos;");
            let _ = writeln!(
                svg,
                "<text x=\"{}\" y=\"{}\" fill=\"{}\" textLength=\"{}\" lengthAdjust=\"spacingAndGlyphs\"{attributes}>{escaped}</text>",
                u32::from(x) * 10,
                u32::from(y) * 20 + 15,
                color(cell.fg),
                symbol.width() * 10
            );
        }
    }
    svg.push_str("</g>\n</svg>\n");
    Ok(svg)
}

fn resolve_color(
    color: Color,
    default: &str,
    colors: &mut HashMap<String, Color>,
    query: &mut impl FnMut(&str) -> io::Result<String>,
) -> io::Result<Color> {
    let index = match color {
        Color::Rgb(..) => return Ok(color),
        Color::Reset => None,
        Color::Black => Some(0),
        Color::Red => Some(1),
        Color::Green => Some(2),
        Color::Yellow => Some(3),
        Color::Blue => Some(4),
        Color::Magenta => Some(5),
        Color::Cyan => Some(6),
        Color::Gray => Some(7),
        Color::DarkGray => Some(8),
        Color::LightRed => Some(9),
        Color::LightGreen => Some(10),
        Color::LightYellow => Some(11),
        Color::LightBlue => Some(12),
        Color::LightMagenta => Some(13),
        Color::LightCyan => Some(14),
        Color::White => Some(15),
        Color::Indexed(index) => Some(index),
    };
    let key = index.map_or_else(|| default.to_owned(), |index| format!("4;{index}"));
    if let Some(&color) = colors.get(&key) {
        return Ok(color);
    }
    let response = query(&key)?;
    let invalid = || io::Error::new(io::ErrorKind::InvalidData, "invalid terminal color reply");
    let rgb = response
        .strip_prefix(&format!("]{key};rgb:"))
        .ok_or_else(invalid)?
        .trim_end_matches(['\x07', '\x1b']);
    let mut parts = rgb.split('/');
    let mut channels = [0; 3];
    for channel in &mut channels {
        let part = parts.next().ok_or_else(invalid)?;
        if part.is_empty() || part.len() > 4 || !part.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(invalid());
        }
        let value = u32::from_str_radix(part, 16).map_err(|_| invalid())?;
        *channel = (value * 255 / ((1 << (part.len() * 4)) - 1)) as u8;
    }
    if parts.next().is_some() {
        return Err(invalid());
    }
    let color = Color::Rgb(channels[0], channels[1], channels[2]);
    colors.insert(key, color);
    Ok(color)
}

fn color(color: Color) -> String {
    match color {
        Color::Rgb(r, g, b) => format!("#{r:02x}{g:02x}{b:02x}"),
        // Rexdown resolves UI and syntax colors to RGB before rendering.
        _ => "#000000".to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::layout::Rect;

    #[test]
    fn svg_preserves_reverse_video() {
        let mut buffer = Buffer::empty(Rect::new(0, 0, 1, 1));
        buffer[(0, 0)]
            .set_symbol("X")
            .set_fg(Color::Rgb(204, 214, 234))
            .set_bg(Color::Rgb(16, 22, 35));
        buffer[(0, 0)].modifier = Modifier::REVERSED;
        let svg = render(&buffer, |_| panic!("RGB needs no terminal queries")).unwrap();
        let xml = roxmltree::Document::parse(&svg).unwrap();
        let rect = xml.descendants().find(|n| n.has_tag_name("rect")).unwrap();
        let text = xml.descendants().find(|n| n.has_tag_name("text")).unwrap();
        assert_eq!(rect.attribute("fill"), Some("#ccd6ea"));
        assert_eq!(text.attribute("fill"), Some("#101623"));
    }

    #[test]
    fn svg_resolves_current_defaults_ansi_and_indexed_colors_or_fails() {
        let mut buffer = Buffer::empty(Rect::new(0, 0, 6, 1));
        buffer[(0, 0)].set_symbol("R");
        buffer[(0, 0)].modifier = Modifier::REVERSED;
        buffer[(1, 0)].set_symbol("A").set_fg(Color::LightBlue);
        buffer[(2, 0)].set_symbol("I").set_fg(Color::Indexed(2));
        buffer[(3, 0)].set_symbol("D");
        buffer[(4, 0)]
            .set_symbol("界")
            .set_fg(Color::Rgb(17, 34, 51))
            .set_bg(Color::Rgb(68, 85, 102));
        for (fg, bg) in [("ccd6ea", "101623"), ("101623", "ccd6ea")] {
            let svg = render(&buffer, |key| {
                let rgb = match key {
                    "10" => format!("{}/{}/{}", &fg[..2], &fg[2..4], &fg[4..]),
                    "11" => format!("{}/{}/{}", &bg[..2], &bg[2..4], &bg[4..]),
                    "4;12" => "1234/5678/9abc".to_owned(),
                    "4;2" => "7/9/b".to_owned(),
                    _ => panic!("unexpected color {key}"),
                };
                Ok(format!("]{key};rgb:{rgb}\x07"))
            })
            .unwrap();
            let xml = roxmltree::Document::parse(&svg).unwrap();
            let rects: Vec<_> = xml
                .descendants()
                .filter(|n| n.has_tag_name("rect"))
                .map(|n| n.attribute("fill").unwrap())
                .collect();
            let text: Vec<_> = xml
                .descendants()
                .filter(|n| n.has_tag_name("text"))
                .map(|n| n.attribute("fill").unwrap())
                .collect();
            assert_eq!(
                rects,
                [
                    format!("#{fg}"),
                    format!("#{bg}"),
                    format!("#{bg}"),
                    format!("#{bg}"),
                    "#445566".into(),
                    "#445566".into()
                ]
            );
            assert_eq!(
                text,
                [
                    format!("#{bg}"),
                    "#12569a".into(),
                    "#7799bb".into(),
                    format!("#{fg}"),
                    "#112233".into()
                ]
            );
        }
        assert!(
            render(&buffer, |_| Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "timeout"
            )))
            .is_err()
        );
        for invalid in [
            "]11;rgb:11/22/33\x07",
            "]10;rgb:12345/22/33\x07",
            "]10;rgb:gg/22/33\x07",
            "]10;rgb:11/22\x07",
        ] {
            assert!(render(&buffer, |_| Ok(invalid.to_owned())).is_err());
        }
    }
}
