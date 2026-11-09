use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Hotkey {
    pub modifiers: u32,
    #[serde(alias = "key_code")]
    pub key_code: u32,
}

impl Default for Hotkey {
    fn default() -> Self {
        Self {
            modifiers: 0,
            key_code: 0x75,
        }
    } // F6
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
#[serde(rename_all = "camelCase")]
pub struct AppConfig {
    pub cps: f64,
    #[serde(default = "missing_interval", skip_serializing_if = "Option::is_none")]
    pub interval_ms: Option<f64>,
    pub rate_entry_mode: RateEntryMode,
    pub hotkey: Hotkey,
    pub transparent: bool,
    pub input_mode: InputMode,
    pub keyboard_key: u32,
    pub mouse_button: MouseButton,
    pub click_type: ClickType,
    pub theme: ThemePreference,
    pub transparency_effects: bool,
    pub always_on_top: bool,
    pub tray_behavior: TrayBehavior,
    #[serde(default, skip_serializing, rename = "minimizeToTray")]
    legacy_minimize_to_tray: bool,
    #[serde(default, skip_serializing, rename = "closeToTray")]
    legacy_close_to_tray: bool,
}

fn missing_interval() -> Option<f64> { None }

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum RateEntryMode {
    #[default]
    Cps,
    Interval,
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum InputMode {
    #[default]
    Mouse,
    Keyboard,
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum MouseButton {
    #[default]
    Left,
    Right,
    Middle,
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ClickType {
    #[default]
    Single,
    Double,
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ThemePreference {
    #[default]
    System,
    Light,
    Dark,
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum TrayBehavior {
    #[default]
    Never,
    Minimize,
    Close,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            cps: 100.0,
            interval_ms: Some(10.0),
            rate_entry_mode: RateEntryMode::Cps,
            hotkey: Hotkey::default(),
            transparent: true,
            input_mode: InputMode::Mouse,
            keyboard_key: 0x20,
            mouse_button: MouseButton::Left,
            click_type: ClickType::Single,
            theme: ThemePreference::System,
            transparency_effects: false,
            always_on_top: false,
            tray_behavior: TrayBehavior::Never,
            legacy_minimize_to_tray: false,
            legacy_close_to_tray: false,
        }
    }
}

fn config_path() -> PathBuf {
    if let Some(base) = dirs::data_local_dir().or_else(|| dirs::config_dir()) {
        base.join("RustAutoClicker").join("config.json")
    } else {
        PathBuf::from("rust-autoclicker-config.json")
    }
}

pub fn load_config() -> AppConfig {
    let path = config_path();
    let Ok(contents) = fs::read_to_string(path) else {
        return AppConfig::default();
    };
    let Ok(mut config) = serde_json::from_str::<AppConfig>(&contents) else {
        return AppConfig::default();
    };
    if config.legacy_close_to_tray {
        config.tray_behavior = TrayBehavior::Close;
    } else if config.legacy_minimize_to_tray {
        config.tray_behavior = TrayBehavior::Minimize;
    }
    if !config.cps.is_finite() || config.cps <= 0.0 { config.cps = 100.0; }
    config.cps = config.cps.clamp(1.0, 2000.0);
    let interval_ms = config.interval_ms.unwrap_or(1000.0 / config.cps);
    let interval_ms = if interval_ms.is_finite() && interval_ms > 0.0 {
        interval_ms.clamp(0.5, 86_400_000.0)
    } else { 10.0 };
    config.interval_ms = Some(interval_ms);
    if config.keyboard_key == config.hotkey.key_code {
        config.keyboard_key = if config.hotkey.key_code == 0x20 {
            0x41
        } else {
            0x20
        };
    }
    config
}

pub fn save_config(config: &AppConfig) -> Result<(), String> {
    let path = config_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let text = serde_json::to_string_pretty(config).map_err(|e| e.to_string())?;
    let temp = path.with_extension("json.tmp");
    fs::write(&temp, text).map_err(|e| e.to_string())?;
    fs::rename(temp, path).map_err(|e| e.to_string())
}
