//! Configuração e temas (TOML) — ~/.config/fpass/config.toml e ~/.config/fpass/themes/*.toml

use ratatui::style::Color;
use serde::Deserialize;
use std::fs;
use std::path::PathBuf;

#[derive(Deserialize, Default)]
struct RawConfig {
    general: Option<RawGeneral>,
}

#[derive(Deserialize)]
struct RawGeneral {
    /// Caminho do password-store (default: $PASSWORD_STORE_DIR ou ~/.password-store)
    store: Option<String>,
    recency: Option<bool>,
    theme: Option<String>,
    /// Segundos até limpar o clipboard (default: 10)
    clip_time: Option<u64>,
}

#[derive(Deserialize, Default)]
struct RawThemeFile {
    theme: Option<RawThemeMeta>,
    colors: Option<RawThemeColors>,
}

#[derive(Deserialize)]
struct RawThemeMeta {
    name: Option<String>,
}

#[derive(Deserialize)]
struct RawThemeColors {
    #[serde(alias = "AlertInfo")]
    alert_info: Option<String>,
    #[serde(alias = "AlertWarn")]
    alert_warn: Option<String>,
    #[serde(alias = "AlertError")]
    alert_error: Option<String>,
    #[serde(alias = "Annotation")]
    annotation: Option<String>,
    #[serde(alias = "Base")]
    base: Option<String>,
    #[serde(alias = "Guidance")]
    guidance: Option<String>,
    #[serde(alias = "Important")]
    important: Option<String>,
    #[serde(alias = "Title")]
    title: Option<String>,
}

#[derive(Clone)]
pub struct AppConfig {
    pub store_path: Option<String>,
    pub recency_enabled: bool,
    pub theme_name: String,
    pub clip_time: u64,
}

#[derive(Clone)]
pub struct Theme {
    pub alert_info: Color,
    pub alert_warn: Color,
    pub alert_error: Color,
    pub annotation: Color,
    pub base: Color,
    pub guidance: Color,
    pub important: Color,
    pub title: Color,
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            alert_info: Color::Green,
            alert_warn: Color::Yellow,
            alert_error: Color::Red,
            annotation: Color::Yellow,
            base: Color::White,
            guidance: Color::DarkGray,
            important: Color::Red,
            title: Color::Cyan,
        }
    }
}

pub fn hex_to_color(hex: &str) -> Option<Color> {
    let hex = hex.trim_start_matches('#');
    if hex.len() == 6 {
        let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
        let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
        let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
        Some(Color::Rgb(r, g, b))
    } else {
        None
    }
}

pub fn config_dir() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_default();
    PathBuf::from(home).join(".config/fpass")
}

pub fn ensure_config_exists(dir: &PathBuf) {
    let config_path = dir.join("config.toml");
    if !config_path.exists() {
        let default_config = r#"[general]
# store = "~/.password-store"   # descomente para usar um store customizado
recency = true
theme = "default"
clip_time = 10
"#;
        let _ = fs::write(config_path, default_config);
    }
}

fn expand_home(p: String, home: &str) -> String {
    if let Some(rest) = p.strip_prefix("~/") {
        format!("{}/{}", home, rest)
    } else {
        p
    }
}

pub fn setup_and_load_config() -> (AppConfig, Theme) {
    let home = std::env::var("HOME").unwrap_or_default();
    let dir = config_dir();
    let themes_dir = dir.join("themes");
    fs::create_dir_all(&themes_dir).ok();

    let mut config = AppConfig {
        store_path: None,
        recency_enabled: true,
        theme_name: "default".to_string(),
        clip_time: 10,
    };

    if let Ok(content) = fs::read_to_string(dir.join("config.toml")) {
        if let Ok(raw) = toml::from_str::<RawConfig>(&content) {
            if let Some(g) = raw.general {
                if let Some(p) = g.store {
                    config.store_path = Some(expand_home(p, &home));
                }
                if let Some(r) = g.recency {
                    config.recency_enabled = r;
                }
                if let Some(t) = g.theme {
                    config.theme_name = t;
                }
                if let Some(c) = g.clip_time {
                    config.clip_time = c.max(1);
                }
            }
        }
    }

    let mut theme = Theme::default();
    if config.theme_name != "default" {
        if let Some(colors) = find_theme_colors(&themes_dir, &config.theme_name) {
            apply_colors(&mut theme, colors);
        }
    }

    (config, theme)
}

fn find_theme_colors(themes_dir: &PathBuf, name: &str) -> Option<RawThemeColors> {
    let entries = fs::read_dir(themes_dir).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.extension().map_or(false, |ext| ext == "toml") {
            continue;
        }
        let Ok(content) = fs::read_to_string(&path) else {
            continue;
        };
        let Ok(raw_theme) = toml::from_str::<RawThemeFile>(&content) else {
            continue;
        };
        if let Some(meta) = raw_theme.theme {
            if meta.name.as_deref() == Some(name) {
                return raw_theme.colors;
            }
        }
    }
    None
}

fn apply_colors(theme: &mut Theme, colors: RawThemeColors) {
    let set = |slot: &mut Color, v: Option<String>| {
        if let Some(c) = v.and_then(|h| hex_to_color(&h)) {
            *slot = c;
        }
    };
    set(&mut theme.alert_info, colors.alert_info);
    set(&mut theme.alert_warn, colors.alert_warn);
    set(&mut theme.alert_error, colors.alert_error);
    set(&mut theme.annotation, colors.annotation);
    set(&mut theme.base, colors.base);
    set(&mut theme.guidance, colors.guidance);
    set(&mut theme.important, colors.important);
    set(&mut theme.title, colors.title);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_to_color_valid() {
        assert_eq!(hex_to_color("#ff0000"), Some(Color::Rgb(255, 0, 0)));
        assert_eq!(hex_to_color("00ff7f"), Some(Color::Rgb(0, 255, 127)));
    }

    #[test]
    fn hex_to_color_invalid() {
        assert_eq!(hex_to_color("#fff"), None);
        assert_eq!(hex_to_color("zzzzzz"), None);
        assert_eq!(hex_to_color(""), None);
    }

    #[test]
    fn expand_home_works() {
        assert_eq!(
            expand_home("~/store".into(), "/home/higor"),
            "/home/higor/store"
        );
        assert_eq!(expand_home("/abs/path".into(), "/home/higor"), "/abs/path");
    }
}
