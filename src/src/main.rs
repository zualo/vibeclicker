#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod engine;
mod settings;

use engine::{measured_cps, ClickEngine, ClickOptions, EngineEvent};
use serde::Serialize;
use settings::{load_config, save_config, AppConfig, ClickType, Hotkey, InputMode, TrayBehavior};
use std::sync::Mutex;
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, LogicalSize, Manager, State, WebviewWindow, WindowEvent,
};

struct AppState {
    engine: Mutex<ClickEngine>,
    config: Mutex<AppConfig>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ClickerState {
    active: bool,
    clicks: u64,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct RateSample {
    accepted_clicks: u64,
    elapsed_ms: u64,
    measured_cps: f64,
}

fn click_options(config: &AppConfig) -> ClickOptions {
    ClickOptions {
        mode: config.input_mode,
        mouse_button: config.mouse_button,
        mouse_buttons: config.mouse_buttons.clone(),
        click_type: config.click_type,
        keyboard_key: config.keyboard_key,
    }
}

fn reject_hotkey_collision(config: &AppConfig, hotkey: Hotkey) -> Result<(), String> {
    if config.input_mode == InputMode::Keyboard {
        reject_key_collision(config.keyboard_key, hotkey.key_code)
    } else {
        Ok(())
    }
}

fn reject_key_collision(output_key: u32, toggle_key: u32) -> Result<(), String> {
    if output_key == toggle_key {
        Err("The repeated key and toggle hotkey must be different. Choose another key.".into())
    } else {
        Ok(())
    }
}

fn set_tray_visibility(app: &AppHandle, visible: bool) -> Result<(), String> {
    let tray = app
        .tray_by_id("vibeclicker-tray")
        .ok_or_else(|| "System tray icon is unavailable".to_string())?;
    tray.set_visible(visible).map_err(|error| error.to_string())
}

fn show_main_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

#[tauri::command]
fn get_settings(state: State<'_, AppState>) -> Result<AppConfig, String> {
    state
        .config
        .lock()
        .map(|config| config.clone())
        .map_err(|_| "Settings are unavailable".into())
}

#[tauri::command]
fn start_clicking(state: State<'_, AppState>) -> Result<ClickerState, String> {
    let engine = state
        .engine
        .lock()
        .map_err(|_| "Click engine is unavailable")?;
    engine.start()?;
    Ok(ClickerState {
        active: engine.is_running(),
        clicks: engine.click_count(),
    })
}

#[tauri::command]
fn stop_clicking(state: State<'_, AppState>) -> Result<ClickerState, String> {
    let engine = state
        .engine
        .lock()
        .map_err(|_| "Click engine is unavailable")?;
    engine.stop();
    Ok(ClickerState {
        active: false,
        clicks: engine.click_count(),
    })
}

#[tauri::command]
fn set_clicking_allowed(allowed: bool, state: State<'_, AppState>) -> Result<ClickerState, String> {
    let engine = state
        .engine
        .lock()
        .map_err(|_| "Click engine is unavailable")?;
    engine.set_clicking_allowed(allowed);
    Ok(ClickerState {
        active: engine.is_running(),
        clicks: engine.click_count(),
    })
}

#[tauri::command]
fn set_window_content_height(height: f64, window: WebviewWindow) -> Result<(), String> {
    const MIN_HEIGHT: f64 = 558.0;
    const MAX_HEIGHT: f64 = 1600.0;
    let height = if height.is_finite() {
        height.clamp(MIN_HEIGHT, MAX_HEIGHT)
    } else {
        return Err("Window height is invalid".into());
    };
    window
        .set_max_size(Some(LogicalSize::new(700.0, height)))
        .map_err(|error| error.to_string())?;
    let scale_factor = window.scale_factor().map_err(|error| error.to_string())?;
    let current = window
        .inner_size()
        .map_err(|error| error.to_string())?
        .to_logical::<f64>(scale_factor);
    if (current.height - height).abs() > 0.5 {
        window
            .set_size(LogicalSize::new(current.width, height))
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

#[tauri::command]
fn set_continuous_enabled(enabled: bool, state: State<'_, AppState>) -> Result<ClickerState, String> {
    let mut config = state.config.lock().map_err(|_| "Settings are unavailable")?;
    let engine = state.engine.lock().map_err(|_| "Click engine is unavailable")?;
    let previous = config.continuous_enabled;
    engine.set_continuous_enabled(enabled);
    config.continuous_enabled = enabled;
    if let Err(error) = save_config(&config) {
        config.continuous_enabled = previous;
        engine.set_continuous_enabled(previous);
        return Err(error);
    }
    Ok(ClickerState { active: engine.is_running(), clicks: engine.click_count() })
}

#[tauri::command]
fn register_hotkey(hotkey: Hotkey, state: State<'_, AppState>) -> Result<(), String> {
    let mut config = state
        .config
        .lock()
        .map_err(|_| "Settings are unavailable")?;
    let previous = config.hotkey;
    reject_hotkey_collision(&config, hotkey)?;
    let engine = state
        .engine
        .lock()
        .map_err(|_| "Click engine is unavailable")?;
    engine.set_hotkey(hotkey)?;
    config.hotkey = hotkey;
    if let Err(error) = save_config(&config) {
        let _ = engine.set_hotkey(previous);
        config.hotkey = previous;
        return Err(format!("Hotkey changed but could not be saved: {error}"));
    }
    Ok(())
}

#[tauri::command]
fn set_keyboard_key(key_code: u32, state: State<'_, AppState>) -> Result<(), String> {
    let mut config = state
        .config
        .lock()
        .map_err(|_| "Settings are unavailable")?;
    if config.input_mode == InputMode::Keyboard {
        reject_key_collision(key_code, config.hotkey.key_code)?;
    }
    let previous = config.keyboard_key;
    config.keyboard_key = key_code;
    let engine = state
        .engine
        .lock()
        .map_err(|_| "Click engine is unavailable")?;
    engine.set_click_options(click_options(&config))?;
    if let Err(error) = save_config(&config) {
        config.keyboard_key = previous;
        let _ = engine.set_click_options(click_options(&config));
        return Err(error);
    }
    Ok(())
}

#[tauri::command]
fn save_settings(
    mut config: AppConfig,
    state: State<'_, AppState>,
    window: WebviewWindow,
) -> Result<(), String> {
    if !config.cps.is_finite() || !(1.0..=2000.0).contains(&config.cps) {
        return Err("Click rate must be between 1 and 2000 CPS".into());
    }
    let interval_ms = config.interval_ms.unwrap_or(1000.0 / config.cps);
    if !interval_ms.is_finite() || !(0.5..=86_400_000.0).contains(&interval_ms) {
        return Err("Interval must be between 0.5 milliseconds and 24 hours".into());
    }
    config.interval_ms = Some(interval_ms);
    if config.mouse_buttons.is_empty() { config.mouse_buttons = vec![config.mouse_button]; }
    if config.click_type != ClickType::Continuous { config.mouse_buttons.truncate(1); }
    config.mouse_button = config.mouse_buttons[0];
    reject_hotkey_collision(&config, config.hotkey)?;

    let mut current = state
        .config
        .lock()
        .map_err(|_| "Settings are unavailable")?;
    let engine = state
        .engine
        .lock()
        .map_err(|_| "Click engine is unavailable")?;
    if config.click_type == settings::ClickType::Continuous && !engine.continuous_supported() {
        return Err("Continuous mode is unavailable because Windows could not monitor held inputs".into());
    }
    if config.hotkey != current.hotkey {
        engine.set_hotkey(config.hotkey)?;
    }
    engine.set_interval_ms(interval_ms)?;
    engine.set_cps(config.cps)?;
    engine.set_rate_entry_mode(config.rate_entry_mode);
    engine.set_human_like(config.human_like_behavior && config.rate_entry_mode == settings::RateEntryMode::Cps);
    engine.set_continuous_enabled(config.continuous_enabled);
    engine.set_click_options(click_options(&config))?;
    window
        .set_always_on_top(config.always_on_top)
        .map_err(|e| e.to_string())?;
    let app = window.app_handle();
    let previous_tray_visible = current.tray_behavior != TrayBehavior::Never;
    let next_tray_visible = config.tray_behavior != TrayBehavior::Never;
    set_tray_visibility(&app, next_tray_visible)?;
    config.transparent = current.transparent;
    if let Err(error) = save_config(&config) {
        let _ = engine.set_interval_ms(current.interval_ms.unwrap_or(1000.0 / current.cps));
        let _ = engine.set_cps(current.cps);
        engine.set_rate_entry_mode(current.rate_entry_mode);
        let _ = engine.set_click_options(click_options(&current));
        let _ = window.set_always_on_top(current.always_on_top);
        let _ = set_tray_visibility(&app, previous_tray_visible);
        if config.hotkey != current.hotkey {
            let _ = engine.set_hotkey(current.hotkey);
        }
        return Err(format!("Could not save settings: {error}"));
    }
    *current = config;
    Ok(())
}

#[tauri::command]
fn set_always_on_top(
    enabled: bool,
    state: State<'_, AppState>,
    window: WebviewWindow,
) -> Result<(), String> {
    window
        .set_always_on_top(enabled)
        .map_err(|e| e.to_string())?;
    let mut config = state
        .config
        .lock()
        .map_err(|_| "Settings are unavailable")?;
    let previous = config.always_on_top;
    config.always_on_top = enabled;
    if let Err(error) = save_config(&config) {
        config.always_on_top = previous;
        let _ = window.set_always_on_top(previous);
        return Err(error);
    }
    Ok(())
}

#[tauri::command]
fn set_theme(theme: settings::ThemePreference, state: State<'_, AppState>, window: WebviewWindow) -> Result<(), String> {
    let mut config = state
        .config
        .lock()
        .map_err(|_| "Settings are unavailable")?;
    let previous = config.theme;
    if config.transparency_effects {
        set_native_acrylic(&window, true, theme)?;
    }
    config.theme = theme;
    if let Err(error) = save_config(&config) {
        config.theme = previous;
        // Restore the system tint if persistence fails.
        if config.transparency_effects { let _ = set_native_acrylic(&window, true, previous); }
        return Err(error);
    }
    Ok(())
}

fn set_native_acrylic(window: &WebviewWindow, enabled: bool, theme: settings::ThemePreference) -> Result<(), String> {
    #[cfg(windows)]
    {
        let dark = match theme {
            settings::ThemePreference::Dark => true,
            settings::ThemePreference::Light => false,
            settings::ThemePreference::System => window.theme().map(|theme| theme == tauri::Theme::Dark).unwrap_or(true),
        };
        if enabled {
            let tint = if dark { (32, 32, 32, 150) } else { (242, 242, 242, 150) };
            window_vibrancy::apply_acrylic(window, Some(tint)).map_err(|error| error.to_string())
        } else {
            window_vibrancy::clear_acrylic(window).map_err(|error| error.to_string())
        }
    }
    #[cfg(not(windows))]
    {
        let _ = (window, enabled, theme);
        Err("Transparency effects are only available on Windows".into())
    }
}

#[tauri::command]
fn set_transparency_effects(
    enabled: bool,
    state: State<'_, AppState>,
    window: WebviewWindow,
) -> Result<(), String> {
    let mut config = state.config.lock().map_err(|_| "Settings are unavailable")?;
    if config.transparency_effects == enabled { return Ok(()); }

    set_native_acrylic(&window, enabled, config.theme)?;
    let previous = config.transparency_effects;
    config.transparency_effects = enabled;
    if let Err(error) = save_config(&config) {
        config.transparency_effects = previous;
        let rollback = set_native_acrylic(&window, previous, config.theme);
        return Err(match rollback {
            Ok(()) => format!("Could not save transparency setting: {error}"),
            Err(rollback_error) => format!("Could not save transparency setting: {error}; effect rollback failed: {rollback_error}"),
        });
    }
    Ok(())
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| show_main_window(app)))
        .setup(|app| {
            let config = load_config();
            let mut config = config;
            let engine = ClickEngine::new(config.cps, config.hotkey)
                .map_err(|error| std::io::Error::other(error))?;
            if config.click_type == settings::ClickType::Continuous && !engine.continuous_supported() {
                config.click_type = settings::ClickType::Single;
                let _ = save_config(&config);
            }
            engine.set_interval_ms(config.interval_ms.unwrap_or(1000.0 / config.cps)).map_err(std::io::Error::other)?;
            engine.set_rate_entry_mode(config.rate_entry_mode);
            engine.set_human_like(config.human_like_behavior && config.rate_entry_mode == settings::RateEntryMode::Cps);
            engine.set_continuous_enabled(config.continuous_enabled);
            engine.set_click_options(click_options(&config)).map_err(std::io::Error::other)?;
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.set_always_on_top(config.always_on_top);
                if config.transparency_effects && set_native_acrylic(&window, true, config.theme).is_err() {
                    let _ = set_native_acrylic(&window, false, config.theme);
                    config.transparency_effects = false;
                    let _ = save_config(&config);
                }
                #[cfg(windows)]
                {
                    let hwnd = window.hwnd().map_err(std::io::Error::other)?;
                    engine.set_excluded_window(hwnd.0 as isize);
                }
            }
            let events = engine.event_receiver();
            let handle = app.handle().clone();
            app.manage(AppState {
                engine: Mutex::new(engine),
                config: Mutex::new(config.clone()),
            });
            let show_item = MenuItem::with_id(app, "show", "Show vibeclicker", true, None::<&str>)?;
            let quit_item = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&show_item, &quit_item])?;
            let tray_app = app.handle().clone();
            let tray = TrayIconBuilder::with_id("vibeclicker-tray")
                .icon(
                    app.default_window_icon()
                        .ok_or_else(|| std::io::Error::other("Default window icon is missing"))?
                        .clone(),
                )
                .tooltip("vibeclicker")
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| match event.id().as_ref() {
                    "show" => show_main_window(app),
                    "quit" => app.exit(0),
                    _ => {}
                })
                .on_tray_icon_event(move |_, event| {
                    if matches!(
                        event,
                        TrayIconEvent::Click {
                            button: MouseButton::Left,
                            button_state: MouseButtonState::Up,
                            ..
                        }
                    ) {
                        show_main_window(&tray_app);
                    }
                })
                .build(app)?;
            tray.set_visible(config.tray_behavior != TrayBehavior::Never)?;

            if let Some(window) = app.get_webview_window("main") {
                let event_window = window.clone();
                let event_app = app.handle().clone();
                window.on_window_event(move |event| match event {
                    WindowEvent::CloseRequested { api, .. } => {
                        let tray_behavior = event_app
                            .state::<AppState>()
                            .config
                            .lock()
                            .map(|config| config.tray_behavior)
                            .unwrap_or(TrayBehavior::Never);
                        if tray_behavior == TrayBehavior::Close {
                            api.prevent_close();
                            let _ = event_window.hide();
                        }
                    }
                    WindowEvent::Resized(_) | WindowEvent::Focused(false) => {
                        let tray_behavior = event_app
                            .state::<AppState>()
                            .config
                            .lock()
                            .map(|config| config.tray_behavior)
                            .unwrap_or(TrayBehavior::Never);
                        if tray_behavior == TrayBehavior::Minimize
                            && event_window.is_minimized().unwrap_or(false)
                        {
                            let _ = event_window.hide();
                        }
                    }
                    _ => {}
                });
            }
            std::thread::spawn(move || {
                while let Ok(event) = events.recv() {
                    match event {
                        EngineEvent::Toggle(active) => {
                            let clicks = handle
                                .state::<AppState>()
                                .engine
                                .lock()
                                .map(|engine| engine.click_count())
                                .unwrap_or_default();
                            let _ = handle.emit("clicker://state", ClickerState { active, clicks });
                        }
                        EngineEvent::ContinuousEnabled(enabled) => {
                            if let Ok(mut config) = handle.state::<AppState>().config.lock() {
                                config.continuous_enabled = enabled;
                                if let Err(error) = save_config(&config) {
                                    let _ = handle.emit("clicker://error", format!("Continuous toggle changed but could not be saved: {error}"));
                                }
                            }
                            let _ = handle.emit("clicker://continuous-enabled", enabled);
                        }
                        EngineEvent::HotkeyError(message) => {
                            let _ = handle.emit("clicker://error", message);
                        }
                        EngineEvent::RateSample { clicks, elapsed_ms } => {
                            let _ = handle.emit(
                                "clicker://rate",
                                RateSample {
                                    accepted_clicks: clicks,
                                    elapsed_ms,
                                    measured_cps: measured_cps(clicks, elapsed_ms),
                                },
                            );
                        }
                    }
                }
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_settings,
            start_clicking,
            stop_clicking,
            set_clicking_allowed,
            set_window_content_height,
            set_continuous_enabled,
            register_hotkey,
            save_settings,
            set_always_on_top,
            set_theme,
            set_transparency_effects,
            set_keyboard_key
        ])
        .run(tauri::generate_context!())
        .expect("error while running vibeclicker");
}

#[cfg(test)]
mod tests {
    use super::reject_key_collision;

    #[test]
    fn output_key_cannot_match_toggle_hotkey() {
        assert!(reject_key_collision(0x20, 0x20).is_err());
    }

    #[test]
    fn different_output_and_toggle_keys_are_allowed() {
        assert!(reject_key_collision(0x20, 0x75).is_ok());
    }
}
