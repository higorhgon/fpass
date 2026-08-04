use ratatui::style::Color;
use serde::Deserialize;
use std::{fs, path::PathBuf};

#[derive(Deserialize, Default)]
struct RawConfig {
    general: Option<RawGeneral>,
}

#[derive(Deserialize)]
struct RawGeneral {
    path: Option<String>,
    recency: Option<bool>,
    theme: Option<String>,
    language: Option<String>,
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
    #[serde(alias = "AlertInfo")] alert_info: Option<String>,
    #[serde(alias = "AlertWarn")] alert_warn: Option<String>,
    #[serde(alias = "AlertError")] alert_error: Option<String>,
    #[serde(alias = "Annotation")] annotation: Option<String>,
    #[serde(alias = "Base")] base: Option<String>,
    #[serde(alias = "Guidance")] guidance: Option<String>,
    #[serde(alias = "Important")] important: Option<String>,
    #[serde(alias = "Title")] title: Option<String>,
}

#[derive(Clone)]
pub struct AppConfig {
    pub search_path: String,
    pub recency_enabled: bool,
    pub theme_name: String,
    pub language: String,
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

impl Theme {
    /// Paleta padrão: Catppuccin Mocha.
    fn default() -> Self {
        Self {
            alert_info: Color::Rgb(166, 227, 161),  // Green
            alert_warn: Color::Rgb(249, 226, 175),  // Yellow
            alert_error: Color::Rgb(243, 139, 168), // Red
            annotation: Color::Rgb(203, 166, 247),  // Mauve
            base: Color::Rgb(205, 214, 244),        // Text
            guidance: Color::Rgb(127, 132, 156),    // Overlay1
            important: Color::Rgb(235, 160, 172),   // Maroon
            title: Color::Rgb(116, 199, 236),       // Sapphire
        }
    }
}

/// Resolve o idioma efetivo, em ordem de prioridade:
/// 1. `language` explícito no config.toml (`"en"`/`"pt-BR"`), se preenchido;
/// 2. autodetecção pelo locale do sistema (`$LANG`, com `$LC_ALL` como
///    alternativa);
/// 3. se nada foi configurado nem detectado, o padrão é inglês.
pub fn resolve_language(configured: Option<&str>, lang_env: Option<&str>, lc_all_env: Option<&str>) -> String {
    match configured {
        Some("en") => return "en".to_string(),
        Some("pt-BR") => return "pt-BR".to_string(),
        _ => {}
    }

    let env_lang = lang_env.filter(|s| !s.is_empty()).or(lc_all_env.filter(|s| !s.is_empty()));
    if let Some(v) = env_lang {
        let v = v.to_lowercase();
        if v.starts_with("pt") { return "pt-BR".to_string(); }
        if v.starts_with("en") { return "en".to_string(); }
    }

    "en".to_string()
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

pub fn setup_and_load_config() -> (AppConfig, Theme) {
    let home = std::env::var("HOME").unwrap_or_default();
    let config_dir = PathBuf::from(format!("{}/.config/fpass", home));
    let themes_dir = config_dir.join("themes");
    fs::create_dir_all(&themes_dir).ok();

    let mut config = AppConfig {
        search_path: home.clone(),
        recency_enabled: true,
        theme_name: "default".to_string(),
        language: "en".to_string(),
    };

    let mut configured_language: Option<String> = None;
    let config_path = config_dir.join("config.toml");
    if let Ok(content) = fs::read_to_string(&config_path) {
        if let Ok(raw) = toml::from_str::<RawConfig>(&content) {
            if let Some(general_config) = raw.general {
                if let Some(p) = general_config.path {
                    config.search_path = if p.starts_with("~/") {
                        p.replacen("~/", &format!("{}/", home), 1)
                    } else { p };
                }
                if let Some(r) = general_config.recency { config.recency_enabled = r; }
                if let Some(t) = general_config.theme { config.theme_name = t; }
                configured_language = general_config.language;
            }
        }
    }
    config.language = resolve_language(
        configured_language.as_deref(),
        std::env::var("LANG").ok().as_deref(),
        std::env::var("LC_ALL").ok().as_deref(),
    );

    let mut theme = Theme::default();
    if config.theme_name != "default" {
        if let Ok(entries) = fs::read_dir(&themes_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().map_or(false, |ext| ext == "toml") {
                    if let Ok(content) = fs::read_to_string(&path) {
                        if let Ok(raw_theme) = toml::from_str::<RawThemeFile>(&content) {
                            if let Some(theme_meta) = raw_theme.theme {
                                if theme_meta.name.as_deref() == Some(config.theme_name.as_str()) {
                                    if let Some(colors) = raw_theme.colors {
                                        if let Some(c) = colors.alert_info.and_then(|h| hex_to_color(&h)) { theme.alert_info = c; }
                                        if let Some(c) = colors.alert_warn.and_then(|h| hex_to_color(&h)) { theme.alert_warn = c; }
                                        if let Some(c) = colors.alert_error.and_then(|h| hex_to_color(&h)) { theme.alert_error = c; }
                                        if let Some(c) = colors.annotation.and_then(|h| hex_to_color(&h)) { theme.annotation = c; }
                                        if let Some(c) = colors.base.and_then(|h| hex_to_color(&h)) { theme.base = c; }
                                        if let Some(c) = colors.guidance.and_then(|h| hex_to_color(&h)) { theme.guidance = c; }
                                        if let Some(c) = colors.important.and_then(|h| hex_to_color(&h)) { theme.important = c; }
                                        if let Some(c) = colors.title.and_then(|h| hex_to_color(&h)) { theme.title = c; }
                                    }
                                    break;
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    (config, theme)
}

pub fn ensure_config_exists(config_dir: &PathBuf) {
    let config_path = config_dir.join("config.toml");
    if !config_path.exists() {
        let default_config = r#"[general]
path = "~/"
recency = true
theme = "default"
language = "auto"
"#;
        let _ = fs::write(config_path, default_config);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_to_color_parses_valid_hex_with_hash() {
        assert_eq!(hex_to_color("#ff0000"), Some(Color::Rgb(255, 0, 0)));
    }

    #[test]
    fn hex_to_color_parses_valid_hex_without_hash() {
        assert_eq!(hex_to_color("00ff80"), Some(Color::Rgb(0, 255, 128)));
    }

    #[test]
    fn hex_to_color_rejects_wrong_length() {
        assert_eq!(hex_to_color("#fff"), None);
        assert_eq!(hex_to_color("#ff00"), None);
        assert_eq!(hex_to_color(""), None);
    }

    #[test]
    fn hex_to_color_rejects_invalid_digits() {
        assert_eq!(hex_to_color("#zzzzzz"), None);
    }

    #[test]
    fn resolve_language_explicit_config_wins_over_env() {
        assert_eq!(resolve_language(Some("en"), Some("pt_BR.UTF-8"), None), "en");
        assert_eq!(resolve_language(Some("pt-BR"), Some("en_US.UTF-8"), None), "pt-BR");
    }

    #[test]
    fn resolve_language_auto_detects_from_lang_env() {
        assert_eq!(resolve_language(None, Some("en_US.UTF-8"), None), "en");
        assert_eq!(resolve_language(Some("auto"), Some("en_GB.UTF-8"), None), "en");
        assert_eq!(resolve_language(None, Some("pt_BR.UTF-8"), None), "pt-BR");
        assert_eq!(resolve_language(Some("auto"), Some("pt_PT.UTF-8"), None), "pt-BR");
    }

    #[test]
    fn resolve_language_falls_back_to_lc_all_when_lang_unset() {
        assert_eq!(resolve_language(None, None, Some("pt_BR.UTF-8")), "pt-BR");
        assert_eq!(resolve_language(None, Some(""), Some("en_US.UTF-8")), "en");
    }

    #[test]
    fn resolve_language_defaults_to_english_when_nothing_detected() {
        assert_eq!(resolve_language(None, None, None), "en");
        assert_eq!(resolve_language(None, Some("fr_FR.UTF-8"), None), "en");
    }
}
