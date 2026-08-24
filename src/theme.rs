use std::{env, fs, path::PathBuf};

use ratatui::style::Color;
use serde::Deserialize;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AppTheme {
    pub background: Color,
    pub panel: Color,
    pub foreground: Color,
    pub bright_foreground: Color,
    pub muted: Color,
    pub accent: Color,
    pub selection: Color,
    pub green: Color,
    pub red: Color,
}

#[derive(Debug, Deserialize)]
struct OmarchyColors {
    accent: String,
    selection: String,
    muted: String,
    background: String,
    lighter_background: String,
    foreground: String,
    bright_foreground: String,
    green: String,
    red: String,
}

impl Default for AppTheme {
    fn default() -> Self {
        Self {
            background: Color::Rgb(26, 27, 38),
            panel: Color::Rgb(36, 40, 59),
            foreground: Color::Rgb(169, 177, 214),
            bright_foreground: Color::Rgb(192, 202, 245),
            muted: Color::Rgb(65, 72, 104),
            accent: Color::Rgb(122, 162, 247),
            selection: Color::Rgb(41, 46, 66),
            green: Color::Rgb(158, 206, 106),
            red: Color::Rgb(247, 118, 142),
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
            panel: parse_hex(&colors.lighter_background)?,
            foreground: parse_hex(&colors.foreground)?,
            bright_foreground: parse_hex(&colors.bright_foreground)?,
            muted: parse_hex(&colors.muted)?,
            accent: parse_hex(&colors.accent)?,
            selection: parse_hex(&colors.selection)?,
            green: parse_hex(&colors.green)?,
            red: parse_hex(&colors.red)?,
        })
    }
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
lighter_background = "#556677"
foreground = "#667788"
bright_foreground = "#778899"
green = "#88aa44"
red = "#cc4455"
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
}
