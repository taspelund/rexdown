use std::sync::LazyLock;

use ratatui::style::{Color, Modifier, Style};
use two_face::theme::{EmbeddedLazyThemeSet, EmbeddedThemeName};

static THEMES: LazyLock<EmbeddedLazyThemeSet> = LazyLock::new(two_face::theme::extra);

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub enum Theme {
    #[default]
    Terminal,
    Dark,
    Light,
    Bundled(EmbeddedThemeName),
}

impl Theme {
    pub fn all() -> impl Iterator<Item = Self> {
        use EmbeddedThemeName::*;
        [Self::Terminal, Self::Dark, Self::Light].into_iter().chain(
            [
                Dracula,
                Nord,
                CatppuccinMocha,
                CatppuccinLatte,
                GruvboxDark,
                GruvboxLight,
                SolarizedDark,
                SolarizedLight,
                TwoDark,
                MonokaiExtended,
            ]
            .into_iter()
            .map(Self::Bundled),
        )
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Terminal => "Terminal",
            Self::Dark => "Dark",
            Self::Light => "Light",
            Self::Bundled(name) => name.as_name(),
        }
    }

    pub fn data(self) -> &'static syntect::highlighting::Theme {
        let name = match self {
            Self::Terminal => EmbeddedThemeName::Ansi,
            Self::Dark => EmbeddedThemeName::Base16OceanDark,
            Self::Light => EmbeddedThemeName::InspiredGithub,
            Self::Bundled(name) => name,
        };
        &THEMES[name]
    }

    pub fn background(self) -> Color {
        match self {
            Self::Terminal => Color::Reset,
            Self::Dark => Color::Rgb(24, 27, 33),
            Self::Light => Color::Rgb(250, 250, 248),
            Self::Bundled(_) => self
                .data()
                .settings
                .background
                .map(rgb)
                .unwrap_or(Color::Black),
        }
    }

    pub fn foreground(self) -> Color {
        match self {
            Self::Terminal => Color::Reset,
            Self::Dark => Color::Rgb(220, 224, 230),
            Self::Light => Color::Rgb(35, 40, 48),
            Self::Bundled(_) => self
                .data()
                .settings
                .foreground
                .map(rgb)
                .unwrap_or(Color::White),
        }
    }

    pub fn bar(self) -> Color {
        match self {
            Self::Terminal => Color::Reset,
            Self::Dark => Color::Rgb(35, 43, 57),
            Self::Light => Color::Rgb(225, 232, 240),
            Self::Bundled(_) => self
                .data()
                .settings
                .selection
                .map(rgb)
                .unwrap_or(self.background()),
        }
    }

    pub fn accent(self) -> Color {
        match self {
            Self::Terminal => Color::Cyan,
            Self::Dark => Color::Rgb(105, 210, 220),
            Self::Light => Color::Rgb(0, 100, 125),
            Self::Bundled(_) => self
                .data()
                .settings
                .caret
                .map(rgb)
                .unwrap_or(self.foreground()),
        }
    }

    pub fn error(self) -> Color {
        if self == Self::Terminal {
            return Color::Red;
        }
        match self.background() {
            Color::Rgb(r, g, b) if u16::from(r) + u16::from(g) + u16::from(b) > 384 => {
                Color::Rgb(170, 35, 35)
            }
            _ => Color::Rgb(255, 120, 120),
        }
    }

    pub fn style(self) -> Style {
        Style::default().bg(self.background()).fg(self.foreground())
    }

    pub fn bar_style(self) -> Style {
        let style = self.style().bg(self.bar());
        if self == Self::Terminal {
            // Contrast on both light and dark terminals without guessing RGB.
            style.add_modifier(Modifier::REVERSED)
        } else {
            style
        }
    }

    pub fn code_color(self, color: syntect::highlighting::Color) -> Color {
        // bat's ANSI theme encodes a palette index in r when a=0 and the
        // terminal default when a=1. These are not literal RGB colors.
        match (self, color.a) {
            (Self::Terminal, 0) => Color::Indexed(color.r),
            (Self::Terminal, 1) => Color::Reset,
            _ => rgb(color),
        }
    }

    pub fn resolve(self, mut style: Style) -> Style {
        style.fg = style.fg.map(|color| match (self, color) {
            (_, Color::Cyan) => self.accent(),
            (Self::Terminal, Color::LightBlue) => Color::Blue,
            (Self::Terminal, Color::LightYellow) => Color::Yellow,
            (Self::Terminal, Color::DarkGray) => Color::Reset,
            (Self::Bundled(_), Color::LightBlue | Color::LightYellow) => self.accent(),
            (Self::Bundled(_), Color::DarkGray) => self
                .data()
                .settings
                .gutter_foreground
                .map(rgb)
                .unwrap_or(self.foreground()),
            (Self::Dark, Color::LightBlue) => Color::Rgb(135, 180, 255),
            (Self::Light, Color::LightBlue) => Color::Rgb(30, 80, 180),
            (Self::Dark, Color::LightYellow) => Color::Rgb(235, 205, 130),
            (Self::Light, Color::LightYellow) => Color::Rgb(125, 75, 10),
            (Self::Dark, Color::DarkGray) => Color::Rgb(130, 140, 155),
            (Self::Light, Color::DarkGray) => Color::Rgb(100, 110, 125),
            (_, color) => color,
        });
        style
    }
}

fn rgb(color: syntect::highlighting::Color) -> Color {
    Color::Rgb(color.r, color.g, color.b)
}
