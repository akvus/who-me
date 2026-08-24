use std::{env, fs, path::PathBuf};

use ratatui::style::Color;
use serde::Deserialize;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AppTheme {
    pub background: Color,
    pub dark_background: Color,
    pub darker_background: Color,
    pub panel: Color,
    pub foreground: Color,
    pub dark_foreground: Color,
    pub bright_foreground: Color,
    pub muted: Color,
    pub accent: Color,
    pub selection: Color,
    pub green: Color,
    pub red: Color,
    pub yellow: Color,
    pub orange: Color,
    pub cyan: Color,
    pub blue: Color,
    pub magenta: Color,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TopicVisual {
    pub symbol: &'static str,
    pub color: Color,
}

#[derive(Debug, Deserialize)]
struct OmarchyColors {
    accent: String,
    selection: String,
    muted: String,
    background: String,
    dark_background: String,
    darker_background: String,
    lighter_background: String,
    foreground: String,
    dark_foreground: String,
    bright_foreground: String,
    green: String,
    red: String,
    yellow: String,
    orange: String,
    cyan: String,
    blue: String,
    magenta: String,
}

impl Default for AppTheme {
    fn default() -> Self {
        Self {
            background: Color::Rgb(26, 27, 38),
            dark_background: Color::Rgb(19, 20, 28),
            darker_background: Color::Rgb(14, 14, 20),
            panel: Color::Rgb(36, 40, 59),
            foreground: Color::Rgb(169, 177, 214),
            dark_foreground: Color::Rgb(86, 95, 137),
            bright_foreground: Color::Rgb(192, 202, 245),
            muted: Color::Rgb(65, 72, 104),
            accent: Color::Rgb(122, 162, 247),
            selection: Color::Rgb(41, 46, 66),
            green: Color::Rgb(158, 206, 106),
            red: Color::Rgb(247, 118, 142),
            yellow: Color::Rgb(224, 175, 104),
            orange: Color::Rgb(235, 146, 123),
            cyan: Color::Rgb(68, 157, 171),
            blue: Color::Rgb(122, 162, 247),
            magenta: Color::Rgb(173, 142, 230),
        }
    }
}

impl AppTheme {
    pub fn load() -> Self {
        theme_path()
            .and_then(|path| fs::read_to_string(path).ok())
            .and_then(|source| Self::from_toml(&source).ok())
            .unwrap_or_default()
    }

    fn from_toml(source: &str) -> Result<Self, String> {
        let colors: OmarchyColors = toml::from_str(source).map_err(|error| error.to_string())?;
        Ok(Self {
            background: parse_hex(&colors.background)?,
            dark_background: parse_hex(&colors.dark_background)?,
            darker_background: parse_hex(&colors.darker_background)?,
            panel: parse_hex(&colors.lighter_background)?,
            foreground: parse_hex(&colors.foreground)?,
            dark_foreground: parse_hex(&colors.dark_foreground)?,
            bright_foreground: parse_hex(&colors.bright_foreground)?,
            muted: parse_hex(&colors.muted)?,
            accent: parse_hex(&colors.accent)?,
            selection: parse_hex(&colors.selection)?,
            green: parse_hex(&colors.green)?,
            red: parse_hex(&colors.red)?,
            yellow: parse_hex(&colors.yellow)?,
            orange: parse_hex(&colors.orange)?,
            cyan: parse_hex(&colors.cyan)?,
            blue: parse_hex(&colors.blue)?,
            magenta: parse_hex(&colors.magenta)?,
        })
    }

    pub fn topic_visual(&self, name: &str) -> TopicVisual {
        const SYMBOLS: [&str; 6] = ["◆", "▲", "●", "✦", "■", "◈"];
        let colors = [
            self.accent,
            self.blue,
            self.cyan,
            self.green,
            self.yellow,
            self.orange,
            self.magenta,
        ];
        let hash = stable_hash(name.as_bytes());
        TopicVisual {
            symbol: SYMBOLS[(hash as usize) % SYMBOLS.len()],
            color: colors[((hash >> 16) as usize) % colors.len()],
        }
    }
}

fn stable_hash(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf29ce484222325, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3)
    })
}

fn theme_path() -> Option<PathBuf> {
    if let Some(state) = nonempty_env("XDG_STATE_HOME") {
        return Some(state.join("omarchy/current/theme/colors.toml"));
    }
    nonempty_env("HOME").map(|home| home.join(".local/state/omarchy/current/theme/colors.toml"))
}

fn nonempty_env(name: &str) -> Option<PathBuf> {
    env::var_os(name)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

fn parse_hex(value: &str) -> Result<Color, String> {
    let hex = value
        .strip_prefix('#')
        .ok_or_else(|| format!("expected #RRGGBB color, got {value:?}"))?;
    if hex.len() != 6 {
        return Err(format!("expected #RRGGBB color, got {value:?}"));
    }
    let component = |range| {
        u8::from_str_radix(&hex[range], 16)
            .map_err(|_| format!("expected #RRGGBB color, got {value:?}"))
    };
    Ok(Color::Rgb(
        component(0..2)?,
        component(2..4)?,
        component(4..6)?,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    const COLORS: &str = r##"
accent = "#112233"
selection = "#223344"
muted = "#334455"
background = "#445566"
dark_background = "#354657"
darker_background = "#243546"
lighter_background = "#556677"
foreground = "#667788"
dark_foreground = "#506172"
bright_foreground = "#778899"
green = "#88aa44"
red = "#cc4455"
yellow = "#ddaa44"
orange = "#ee8844"
cyan = "#44aabb"
blue = "#4488dd"
magenta = "#aa55cc"
"##;

    #[test]
    fn parses_an_omarchy_palette() {
        let theme = AppTheme::from_toml(COLORS).unwrap();
        assert_eq!(theme.accent, Color::Rgb(0x11, 0x22, 0x33));
        assert_eq!(theme.panel, Color::Rgb(0x55, 0x66, 0x77));
    }

    #[test]
    fn rejects_invalid_colors() {
        assert!(parse_hex("blue").is_err());
        assert!(parse_hex("#1234").is_err());
        assert!(parse_hex("#zzzzzz").is_err());
    }

    #[test]
    fn topic_visuals_are_stable_and_name_derived() {
        let theme = AppTheme::default();
        let developer = theme.topic_visual("Developer");
        assert_eq!(developer, theme.topic_visual("Developer"));
        assert_ne!(developer, theme.topic_visual("Mountaineer"));
    }
}
