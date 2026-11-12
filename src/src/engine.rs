use crate::settings::{ClickType, Hotkey, InputMode, MouseButton, RateEntryMode};
use std::sync::atomic::AtomicIsize;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{
    mpsc::{self, Receiver, Sender},
    Arc, Mutex, RwLock,
};
#[cfg(windows)]
use std::collections::HashMap;
#[cfg(windows)]
use std::sync::OnceLock;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const VK_XBUTTON1_CODE: u32 = 0x05;
const VK_XBUTTON2_CODE: u32 = 0x06;
#[cfg(windows)]
const VK_LBUTTON_CODE: u32 = 0x01;
#[cfg(windows)]
const VK_RBUTTON_CODE: u32 = 0x02;
#[cfg(windows)]
const VK_MBUTTON_CODE: u32 = 0x04;

#[derive(Debug, Clone)]
pub enum EngineEvent {
    Toggle(bool),
    ContinuousEnabled(bool),
    HotkeyError(String),
    RateSample { clicks: u64, elapsed_ms: u64 },
}

#[derive(Clone)]
pub struct ClickOptions {
    pub mode: InputMode,
    pub mouse_button: MouseButton,
    pub mouse_buttons: Vec<MouseButton>,
    pub click_type: ClickType,
    pub keyboard_key: u32,
}

/// Converts a successful click sample into measured clicks per second.
/// Samples are emitted by the same scheduler used for real clicking, so this
/// is the value to use when validating timing in a safe test window.
pub fn measured_cps(clicks: u64, elapsed_ms: u64) -> f64 {
    if elapsed_ms == 0 {
        0.0
    } else {
        clicks as f64 * 1000.0 / elapsed_ms as f64
    }
}

fn clicks_per_action(options: ClickOptions) -> u64 {
    match (options.mode, options.click_type) {
        (InputMode::Mouse, ClickType::Double) => 2,
        _ => 1,
    }
}

fn selected_mouse_buttons(options: ClickOptions) -> Vec<MouseButton> {
    if options.click_type == ClickType::Continuous && !options.mouse_buttons.is_empty() {
        options.mouse_buttons
    } else { vec![options.mouse_button] }
}

#[cfg(windows)]
fn physical_inputs() -> &'static Mutex<HashMap<u32, Instant>> {
    static INPUTS: OnceLock<Mutex<HashMap<u32, Instant>>> = OnceLock::new();
    INPUTS.get_or_init(|| Mutex::new(HashMap::new()))
}

#[cfg(windows)]
fn continuous_input_is_held(options: ClickOptions) -> bool {
    const HOLD_THRESHOLD: Duration = Duration::from_millis(80);
    physical_inputs().lock().map(|inputs| {
        if options.mode == InputMode::Keyboard {
            return inputs.get(&options.keyboard_key).is_some_and(|pressed_at| pressed_at.elapsed() >= HOLD_THRESHOLD);
        }
        selected_mouse_buttons(options).into_iter().any(|button| {
            let key = match button { MouseButton::Left => VK_LBUTTON_CODE, MouseButton::Right => VK_RBUTTON_CODE, MouseButton::Middle => VK_MBUTTON_CODE };
            inputs.get(&key).is_some_and(|pressed_at| pressed_at.elapsed() >= HOLD_THRESHOLD)
        })
    })
        .unwrap_or(false)
}

fn interval_for_rate(cps_milli: u64, interval_ns: u64, mode: RateEntryMode, options: ClickOptions) -> Duration {
    match mode {
        RateEntryMode::Cps => {
            let cps = cps_milli.max(1000) as f64 / 1000.0;
            Duration::from_secs_f64(clicks_per_action(options) as f64 / cps)
        }
        RateEntryMode::Interval => Duration::from_nanos(interval_ns),
    }
}

fn toggle_if_allowed(running: &AtomicBool, allowed: &AtomicBool, schedule_generation: &AtomicU64) -> Option<bool> {
    if !allowed.load(Ordering::Acquire) {
        return None;
    }
    let next = !running.fetch_xor(true, Ordering::AcqRel);
    schedule_generation.fetch_add(1, Ordering::Release);
    if next && !allowed.load(Ordering::Acquire) {
        running.store(false, Ordering::Release);
        Some(false)
    } else {
        Some(next)
    }
}

fn is_mouse_button_hotkey(key_code: u32) -> bool {
    matches!(key_code, VK_XBUTTON1_CODE | VK_XBUTTON2_CODE)
}

#[cfg(windows)]
unsafe extern "system" fn continuous_keyboard_hook(
    code: i32,
    wparam: windows::Win32::Foundation::WPARAM,
    lparam: windows::Win32::Foundation::LPARAM,
) -> windows::Win32::Foundation::LRESULT {
    use windows::Win32::UI::WindowsAndMessaging::{
        CallNextHookEx, KBDLLHOOKSTRUCT, HC_ACTION, LLKHF_INJECTED, WM_KEYDOWN, WM_KEYUP,
        WM_SYSKEYDOWN, WM_SYSKEYUP,
    };
    if code == HC_ACTION as i32
        && matches!(wparam.0 as u32, WM_KEYDOWN | WM_SYSKEYDOWN | WM_KEYUP | WM_SYSKEYUP)
    {
        let input = &*(lparam.0 as *const KBDLLHOOKSTRUCT);
        if !input.flags.contains(LLKHF_INJECTED) {
            if let Ok(mut inputs) = physical_inputs().lock() {
                if matches!(wparam.0 as u32, WM_KEYDOWN | WM_SYSKEYDOWN) {
                    inputs.entry(input.vkCode).or_insert_with(Instant::now);
                } else {
                    inputs.remove(&input.vkCode);
                }
            }
        }
    }
    CallNextHookEx(None, code, wparam, lparam)
}

#[cfg(windows)]
unsafe extern "system" fn continuous_mouse_hook(
    code: i32,
    wparam: windows::Win32::Foundation::WPARAM,
    lparam: windows::Win32::Foundation::LPARAM,
) -> windows::Win32::Foundation::LRESULT {
    use windows::Win32::UI::WindowsAndMessaging::{
        CallNextHookEx, MSLLHOOKSTRUCT, HC_ACTION, LLMHF_INJECTED, WM_LBUTTONDOWN,
        WM_LBUTTONUP, WM_MBUTTONDOWN, WM_MBUTTONUP, WM_RBUTTONDOWN, WM_RBUTTONUP,
    };
    if code == HC_ACTION as i32
        && matches!(
            wparam.0 as u32,
            WM_LBUTTONDOWN | WM_LBUTTONUP | WM_MBUTTONDOWN | WM_MBUTTONUP | WM_RBUTTONDOWN | WM_RBUTTONUP
        )
    {
        let input = &*(lparam.0 as *const MSLLHOOKSTRUCT);
        if input.flags & LLMHF_INJECTED == 0 {
            let key = match wparam.0 as u32 {
                WM_LBUTTONDOWN | WM_LBUTTONUP => Some(VK_LBUTTON_CODE),
                WM_RBUTTONDOWN | WM_RBUTTONUP => Some(VK_RBUTTON_CODE),
                WM_MBUTTONDOWN | WM_MBUTTONUP => Some(VK_MBUTTON_CODE),
                _ => None,
            };
            if let (Some(key), Ok(mut inputs)) = (key, physical_inputs().lock()) {
                if matches!(wparam.0 as u32, WM_LBUTTONDOWN | WM_RBUTTONDOWN | WM_MBUTTONDOWN) {
                    inputs.entry(key).or_insert_with(Instant::now);
                } else {
                    inputs.remove(&key);
                }
            }
        }
    }
    CallNextHookEx(None, code, wparam, lparam)
}

enum HotkeyCommand {
    Set(Hotkey, Sender<Result<(), String>>),
    Stop,
}

pub struct ClickEngine {
    running: Arc<AtomicBool>,
    clicking_allowed: Arc<AtomicBool>,
    shutdown: Arc<AtomicBool>,
    interval_ns: Arc<AtomicU64>,
    cps_milli: Arc<AtomicU64>,
    schedule_generation: Arc<AtomicU64>,
    rate_entry_mode: Arc<RwLock<RateEntryMode>>,
    click_options: Arc<RwLock<ClickOptions>>,
    continuous_supported: bool,
    continuous_enabled: Arc<AtomicBool>,
    human_like: Arc<AtomicBool>,
    excluded_window: Arc<AtomicIsize>,
    clicks: Arc<AtomicU64>,
    events: Arc<Mutex<Option<Receiver<EngineEvent>>>>,
    event_tx: Sender<EngineEvent>,
    hotkey_tx: Sender<HotkeyCommand>,
    click_thread: Option<JoinHandle<()>>,
    hotkey_thread: Option<JoinHandle<()>>,
}

impl ClickEngine {
    pub fn new(cps: f64, hotkey: Hotkey) -> Result<Self, String> {
        if !cps.is_finite() || !(1.0..=2000.0).contains(&cps) {
            return Err("CPS must be between 1 and 2000".into());
        }
        if is_mouse_button_hotkey(hotkey.key_code) && hotkey.modifiers != 0 {
            return Err("Mouse 4 and Mouse 5 hotkeys do not support modifiers".into());
        }
        let running = Arc::new(AtomicBool::new(false));
        let clicking_allowed = Arc::new(AtomicBool::new(true));
        let shutdown = Arc::new(AtomicBool::new(false));
        let interval_ns = Arc::new(AtomicU64::new(10_000_000));
        let cps_milli = Arc::new(AtomicU64::new((cps * 1000.0) as u64));
        let schedule_generation = Arc::new(AtomicU64::new(0));
        let rate_entry_mode = Arc::new(RwLock::new(RateEntryMode::Cps));
        let click_options = Arc::new(RwLock::new(ClickOptions {
            mode: InputMode::Mouse,
            mouse_button: MouseButton::Left,
            mouse_buttons: vec![MouseButton::Left],
            click_type: ClickType::Single,
            keyboard_key: 0x20,
        }));
        let excluded_window = Arc::new(AtomicIsize::new(0));
        let clicks = Arc::new(AtomicU64::new(0));
        let (event_tx, event_rx) = mpsc::channel();
        let (hotkey_tx, hotkey_rx) = mpsc::channel();
        let (ready_tx, ready_rx) = mpsc::channel::<Result<bool, String>>();

        let click_running = running.clone();
        let click_allowed = clicking_allowed.clone();
        let click_shutdown = shutdown.clone();
        let click_rate = interval_ns.clone();
        let click_cps = cps_milli.clone();
        let click_generation = schedule_generation.clone();
        let click_mode = rate_entry_mode.clone();
        let click_options_thread = click_options.clone();
        let click_continuous_enabled = Arc::new(AtomicBool::new(true));
        let click_human_like = Arc::new(AtomicBool::new(false));
        let click_continuous_enabled_thread = click_continuous_enabled.clone();
        let click_human_like_thread = click_human_like.clone();
        let excluded_window_thread = excluded_window.clone();
        let click_count = clicks.clone();
        let click_events = event_tx.clone();
        let click_input_events = event_tx.clone();
        let click_thread = thread::spawn(move || {
            let mut next = Instant::now();
            let mut random_state = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|duration| duration.as_nanos() as u64)
                .unwrap_or(0x9E3779B97F4A7C15)
                | 1;
            let mut observed_generation = click_generation.load(Ordering::Acquire);
            let mut window_start = Instant::now();
            let mut window_clicks = 0u64;
            let mut continuous_blocked_until_release = false;
            let mut was_continuous = false;
            while !click_shutdown.load(Ordering::Acquire) {
                let options = click_options_thread.read().unwrap().clone();
                if options.click_type == ClickType::Continuous {
                    was_continuous = true;
                    #[cfg(windows)]
                    let is_down = continuous_input_is_held(options.clone());
                    #[cfg(not(windows))]
                    let is_down = false;
                    let allowed = click_allowed.load(Ordering::Acquire);
                    if !is_down {
                        continuous_blocked_until_release = false;
                    } else if !allowed {
                        continuous_blocked_until_release = true;
                    }
                    let should_run = is_down && allowed && click_continuous_enabled_thread.load(Ordering::Acquire) && !continuous_blocked_until_release;
                    let was_running = click_running.swap(should_run, Ordering::AcqRel);
                    if was_running != should_run {
                        click_generation.fetch_add(1, Ordering::Release);
                        let _ = click_input_events.send(EngineEvent::Toggle(should_run));
                    }
                } else if was_continuous {
                    was_continuous = false;
                    continuous_blocked_until_release = false;
                    if click_running.swap(false, Ordering::AcqRel) {
                        click_generation.fetch_add(1, Ordering::Release);
                        let _ = click_input_events.send(EngineEvent::Toggle(false));
                    }
                }
                let generation = click_generation.load(Ordering::Acquire);
                if generation != observed_generation {
                    observed_generation = generation;
                    next = Instant::now();
                }
                if !click_running.load(Ordering::Acquire) || !click_allowed.load(Ordering::Acquire)
                {
                    thread::sleep(Duration::from_millis(10));
                    next = Instant::now();
                    window_start = next;
                    window_clicks = 0;
                    continue;
                }
                let interval_rate = click_rate.load(Ordering::Relaxed);
                let now = Instant::now();
                if now < next {
                    let remaining = next - now;
                    if remaining > Duration::from_micros(700) {
                        thread::sleep((remaining - Duration::from_micros(500)).min(Duration::from_millis(50)));
                    } else {
                        std::hint::spin_loop();
                    }
                    continue;
                }
                let rate_mode = *click_mode.read().unwrap();
                let mut interval = interval_for_rate(click_cps.load(Ordering::Relaxed), interval_rate, rate_mode, options.clone());
                if rate_mode == RateEntryMode::Cps && click_human_like_thread.load(Ordering::Relaxed) {
                    // Xorshift gives a cheap per-click sample in [0, 1). Mapping
                    // it to [-0.1, 0.1] keeps CPS within ±10% of the setting.
                    random_state ^= random_state << 13;
                    random_state ^= random_state >> 7;
                    random_state ^= random_state << 17;
                    let sample = (random_state as f64 / u64::MAX as f64) * 0.2 - 0.1;
                    interval = interval.div_f64(1.0 + sample);
                }
                let excluded = excluded_window_thread.load(Ordering::Relaxed);
                let safe_target = match options.mode {
                    InputMode::Mouse => !cursor_over_excluded_window(excluded),
                    InputMode::Keyboard => !excluded_window_is_foreground(excluded),
                };
                if safe_target {
                    let buttons = if options.mode == InputMode::Mouse {
                        selected_mouse_buttons(options.clone())
                    } else {
                        vec![options.mouse_button]
                    };
                    let mut accepted_clicks = 0;
                    for button in buttons {
                        let mut button_options = options.clone();
                        button_options.mouse_button = button;
                        let action_count = clicks_per_action(button_options.clone());
                        if send_input(button_options) { accepted_clicks += action_count; }
                    }
                    if accepted_clicks > 0 { click_count.fetch_add(accepted_clicks, Ordering::Relaxed); window_clicks += accepted_clicks; }
                }
                next += interval;
                if next <= now {
                    next = now + interval;
                }
                if now.duration_since(window_start) >= Duration::from_secs(1) {
                    let elapsed = now.duration_since(window_start).as_millis() as u64;
                    let _ = click_events.send(EngineEvent::RateSample {
                        clicks: window_clicks,
                        elapsed_ms: elapsed,
                    });
                    window_start = now;
                    window_clicks = 0;
                }
            }
        });

        let hotkey_running = running.clone();
        let hotkey_allowed = clicking_allowed.clone();
        let hotkey_events = event_tx.clone();
        let hotkey_generation = schedule_generation.clone();
        let hotkey_options = click_options.clone();
        let hotkey_continuous_enabled = click_continuous_enabled.clone();
        let hotkey_thread = thread::spawn(move || {
            hotkey_loop(
                hotkey_rx,
                hotkey,
                hotkey_running,
                hotkey_allowed,
                hotkey_generation,
                hotkey_options,
                hotkey_continuous_enabled,
                hotkey_events,
                ready_tx,
            )
        });

        let continuous_supported = match ready_rx.recv_timeout(Duration::from_secs(2)) {
            Ok(Ok(supported)) => supported,
            Ok(Err(error)) => {
                let _ = hotkey_tx.send(HotkeyCommand::Stop);
                shutdown.store(true, Ordering::Release);
                let _ = hotkey_thread.join();
                let _ = click_thread.join();
                return Err(error);
            }
            Err(error) => {
                let _ = hotkey_tx.send(HotkeyCommand::Stop);
                shutdown.store(true, Ordering::Release);
                let _ = hotkey_thread.join();
                let _ = click_thread.join();
                return Err(format!("Hotkey service failed to start: {error}"));
            }
        };

        Ok(Self {
            running,
            clicking_allowed,
            shutdown,
            interval_ns,
            cps_milli,
            schedule_generation,
            rate_entry_mode,
            click_options,
            continuous_supported,
            continuous_enabled: click_continuous_enabled,
            human_like: click_human_like,
            excluded_window,
            clicks,
            events: Arc::new(Mutex::new(Some(event_rx))),
            event_tx,
            hotkey_tx,
            click_thread: Some(click_thread),
            hotkey_thread: Some(hotkey_thread),
        })
    }

    pub fn set_interval_ms(&self, interval_ms: f64) -> Result<(), String> {
        if !interval_ms.is_finite() || !(0.5..=86_400_000.0).contains(&interval_ms) {
            return Err("Interval must be between 0.5 milliseconds and 24 hours".into());
        }
        self.interval_ns.store((interval_ms * 1_000_000.0).round() as u64, Ordering::Release);
        self.schedule_generation.fetch_add(1, Ordering::Release);
        Ok(())
    }

    pub fn set_cps(&self, cps: f64) -> Result<(), String> {
        if !cps.is_finite() || !(1.0..=2000.0).contains(&cps) {
            return Err("CPS must be between 1 and 2000".into());
        }
        self.cps_milli.store((cps * 1000.0) as u64, Ordering::Release);
        self.schedule_generation.fetch_add(1, Ordering::Release);
        Ok(())
    }

    pub fn set_rate_entry_mode(&self, mode: RateEntryMode) {
        *self.rate_entry_mode.write().unwrap() = mode;
        self.schedule_generation.fetch_add(1, Ordering::Release);
    }

    pub fn set_human_like(&self, enabled: bool) { self.human_like.store(enabled, Ordering::Release); self.schedule_generation.fetch_add(1, Ordering::Release); }
    pub fn set_continuous_enabled(&self, enabled: bool) { self.continuous_enabled.store(enabled, Ordering::Release); self.schedule_generation.fetch_add(1, Ordering::Release); if !enabled { self.stop(); } }

    pub fn set_click_options(&self, options: ClickOptions) -> Result<(), String> {
        if options.click_type == ClickType::Continuous && !self.continuous_supported {
            return Err("Continuous mode is unavailable because Windows could not monitor held inputs".into());
        }
        *self.click_options.write().unwrap() = options;
        self.schedule_generation.fetch_add(1, Ordering::Release);
        Ok(())
    }
    pub fn continuous_supported(&self) -> bool {
        self.continuous_supported
    }

    pub fn set_excluded_window(&self, native_handle: isize) {
        self.excluded_window.store(native_handle, Ordering::Release);
    }

    pub fn set_hotkey(&self, hotkey: Hotkey) -> Result<(), String> {
        if is_mouse_button_hotkey(hotkey.key_code) && hotkey.modifiers != 0 {
            return Err("Mouse 4 and Mouse 5 hotkeys do not support modifiers".into());
        }
        let (tx, rx) = mpsc::channel();
        self.hotkey_tx
            .send(HotkeyCommand::Set(hotkey, tx))
            .map_err(|_| "Hotkey service stopped".to_string())?;
        rx.recv_timeout(Duration::from_secs(2))
            .map_err(|_| "Timed out changing hotkey".to_string())?
    }

    #[allow(dead_code)]
    pub fn toggle(&self) {
        let next = !self.running.fetch_xor(true, Ordering::AcqRel);
        self.schedule_generation.fetch_add(1, Ordering::Release);
        let _ = self.event_tx.send(EngineEvent::Toggle(next));
    }
    pub fn start(&self) -> Result<(), String> {
        if !self.clicking_allowed.load(Ordering::Acquire) {
            return Err("Clicking is disabled while Settings is open".into());
        }
        if self.click_options.read().unwrap().click_type == ClickType::Continuous {
            return Err("Hold the selected input to click continuously".into());
        }
        if !self.running.swap(true, Ordering::AcqRel) {
            self.schedule_generation.fetch_add(1, Ordering::Release);
            let _ = self.event_tx.send(EngineEvent::Toggle(true));
        }
        Ok(())
    }
    pub fn set_clicking_allowed(&self, allowed: bool) {
        self.clicking_allowed.store(allowed, Ordering::Release);
        if !allowed {
            self.stop();
        }
    }
    pub fn stop(&self) {
        if self.running.swap(false, Ordering::AcqRel) {
            self.schedule_generation.fetch_add(1, Ordering::Release);
            let _ = self.event_tx.send(EngineEvent::Toggle(false));
        }
    }
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::Acquire)
    }
    pub fn click_count(&self) -> u64 {
        self.clicks.load(Ordering::Relaxed)
    }
    pub fn event_receiver(&self) -> Receiver<EngineEvent> {
        self.events
            .lock()
            .unwrap()
            .take()
            .expect("event receiver already taken")
    }
}

impl Drop for ClickEngine {
    fn drop(&mut self) {
        self.running.store(false, Ordering::Release);
        self.shutdown.store(true, Ordering::Release);
        let _ = self.hotkey_tx.send(HotkeyCommand::Stop);
        if let Some(t) = self.hotkey_thread.take() {
            let _ = t.join();
        }
        if let Some(t) = self.click_thread.take() {
            let _ = t.join();
        }
    }
}

#[cfg(windows)]
fn cursor_over_excluded_window(excluded: isize) -> bool {
    use windows::Win32::Foundation::POINT;
    use windows::Win32::UI::WindowsAndMessaging::{
        GetAncestor, GetCursorPos, WindowFromPoint, GA_ROOT,
    };

    if excluded == 0 {
        return false;
    }
    unsafe {
        let mut point = POINT::default();
        if GetCursorPos(&mut point).is_err() {
            return false;
        }
        let under_cursor = WindowFromPoint(point);
        let root = GetAncestor(under_cursor, GA_ROOT);
        root.0 as isize == excluded
    }
}

#[cfg(windows)]
fn excluded_window_is_foreground(excluded: isize) -> bool {
    use windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow;
    excluded != 0 && unsafe { GetForegroundWindow().0 as isize == excluded }
}

#[cfg(not(windows))]
fn cursor_over_excluded_window(_: isize) -> bool {
    false
}

#[cfg(not(windows))]
fn excluded_window_is_foreground(_: isize) -> bool {
    false
}

#[cfg(windows)]
fn send_input(options: ClickOptions) -> bool {
    match options.mode {
        InputMode::Mouse => send_mouse_click(options.mouse_button, options.click_type),
        InputMode::Keyboard => send_key_press(options.keyboard_key),
    }
}

#[cfg(windows)]
fn send_mouse_click(button: MouseButton, click_type: ClickType) -> bool {
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        SendInput, INPUT, INPUT_0, INPUT_MOUSE, MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP,
        MOUSEEVENTF_MIDDLEDOWN, MOUSEEVENTF_MIDDLEUP, MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP,
        MOUSEINPUT,
    };
    let (down, up) = match button {
        MouseButton::Left => (MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP),
        MouseButton::Right => (MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP),
        MouseButton::Middle => (MOUSEEVENTF_MIDDLEDOWN, MOUSEEVENTF_MIDDLEUP),
    };
    let make_input = |flags| INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 {
            mi: MOUSEINPUT {
                dx: 0,
                dy: 0,
                mouseData: 0,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    };
    let inputs = [
        make_input(down),
        make_input(up),
        make_input(down),
        make_input(up),
    ];
    let len = if click_type == ClickType::Double {
        4
    } else {
        2
    };
    // Win32's count is the number of INPUT records accepted by the system;
    // requiring both records means the down/up pair was accepted.
    unsafe { SendInput(&inputs[..len], std::mem::size_of::<INPUT>() as i32) == len as u32 }
}

#[cfg(windows)]
fn send_key_press(key_code: u32) -> bool {
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, VIRTUAL_KEY,
    };
    let make_input = |flags| INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: VIRTUAL_KEY(key_code as u16),
                wScan: 0,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    };
    let inputs = [make_input(Default::default()), make_input(KEYEVENTF_KEYUP)];
    unsafe { SendInput(&inputs, std::mem::size_of::<INPUT>() as i32) == 2 }
}

#[cfg(not(windows))]
fn send_input(_: ClickOptions) -> bool {
    false
}

#[cfg(windows)]
fn hotkey_loop(
    rx: Receiver<HotkeyCommand>,
    initial: Hotkey,
    running: Arc<AtomicBool>,
    clicking_allowed: Arc<AtomicBool>,
    schedule_generation: Arc<AtomicU64>,
    click_options: Arc<RwLock<ClickOptions>>,
    continuous_enabled: Arc<AtomicBool>,
    events: Sender<EngineEvent>,
    ready: Sender<Result<bool, String>>,
) {
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        GetAsyncKeyState, RegisterHotKey, UnregisterHotKey, HOT_KEY_MODIFIERS,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        DispatchMessageW, PeekMessageW, SetWindowsHookExW, UnhookWindowsHookEx, MSG,
        PM_REMOVE, WH_KEYBOARD_LL, WH_MOUSE_LL, WM_HOTKEY,
    };
    let id: i32 = 0x51A7;
    let mut current = initial;
    let mut registered = !is_mouse_button_hotkey(current.key_code);
    unsafe {
        if registered
            && RegisterHotKey(
                None,
                id,
                HOT_KEY_MODIFIERS(current.modifiers),
                current.key_code,
            )
            .is_err()
        {
            let message = "Default hotkey is unavailable".to_string();
            let _ = ready.send(Err(message.clone()));
            let _ = events.send(EngineEvent::HotkeyError(message));
            return;
        }
        let mut keyboard_hook = SetWindowsHookExW(WH_KEYBOARD_LL, Some(continuous_keyboard_hook), None, 0).ok();
        let mut mouse_hook = if keyboard_hook.is_some() {
            SetWindowsHookExW(WH_MOUSE_LL, Some(continuous_mouse_hook), None, 0).ok()
        } else {
            None
        };
        let continuous_supported = keyboard_hook.is_some() && mouse_hook.is_some();
        if !continuous_supported {
            if let Some(hook) = keyboard_hook.take() { let _ = UnhookWindowsHookEx(hook); }
            if let Some(hook) = mouse_hook.take() { let _ = UnhookWindowsHookEx(hook); }
        }
        let _ = ready.send(Ok(continuous_supported));
        let mut msg = MSG::default();
        let mut mouse4_was_down = GetAsyncKeyState(VK_XBUTTON1_CODE as i32) < 0;
        let mut mouse5_was_down = GetAsyncKeyState(VK_XBUTTON2_CODE as i32) < 0;
        loop {
            while let Ok(command) = rx.try_recv() {
                match command {
                    HotkeyCommand::Stop => {
                        if let Some(hook) = keyboard_hook { let _ = UnhookWindowsHookEx(hook); }
                        if let Some(hook) = mouse_hook { let _ = UnhookWindowsHookEx(hook); }
                        if registered {
                            let _ = UnregisterHotKey(None, id);
                        }
                        return;
                    }
                    HotkeyCommand::Set(next, reply) => {
                        if is_mouse_button_hotkey(next.key_code) && next.modifiers != 0 {
                            let _ = reply.send(Err(
                                "Mouse 4 and Mouse 5 hotkeys do not support modifiers".into(),
                            ));
                            continue;
                        }
                        if registered {
                            let _ = UnregisterHotKey(None, id);
                        }
                        let result = if is_mouse_button_hotkey(next.key_code) {
                            Ok(())
                        } else {
                            RegisterHotKey(
                                None,
                                id,
                                HOT_KEY_MODIFIERS(next.modifiers),
                                next.key_code,
                            )
                            .map_err(|_| "That hotkey is already in use or invalid".to_string())
                        };
                        if result.is_ok() {
                            current = next;
                            registered = !is_mouse_button_hotkey(current.key_code);
                            mouse4_was_down = GetAsyncKeyState(VK_XBUTTON1_CODE as i32) < 0;
                            mouse5_was_down = GetAsyncKeyState(VK_XBUTTON2_CODE as i32) < 0;
                        } else {
                            registered = !is_mouse_button_hotkey(current.key_code)
                                && RegisterHotKey(
                                    None,
                                    id,
                                    HOT_KEY_MODIFIERS(current.modifiers),
                                    current.key_code,
                                )
                                .is_ok();
                        }
                        let _ = reply.send(result);
                    }
                }
            }
            let result = PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE);
            if result.as_bool()
                && msg.message == WM_HOTKEY
                && msg.wParam.0 as i32 == id
                && registered
            {
                if click_options.read().unwrap().click_type == ClickType::Continuous && clicking_allowed.load(Ordering::Acquire) {
                    let next = !continuous_enabled.fetch_xor(true, Ordering::AcqRel);
                    if running.swap(false, Ordering::AcqRel) {
                        let _ = events.send(EngineEvent::Toggle(false));
                    }
                    let _ = events.send(EngineEvent::ContinuousEnabled(next));
                } else if let Some(next) = toggle_if_allowed(&running, &clicking_allowed, &schedule_generation) {
                    let _ = events.send(EngineEvent::Toggle(next));
                }
            }
            let mouse4_is_down = GetAsyncKeyState(VK_XBUTTON1_CODE as i32) < 0;
            let mouse5_is_down = GetAsyncKeyState(VK_XBUTTON2_CODE as i32) < 0;
            let selected_mouse_button_pressed = match current.key_code {
                VK_XBUTTON1_CODE => mouse4_is_down && !mouse4_was_down,
                VK_XBUTTON2_CODE => mouse5_is_down && !mouse5_was_down,
                _ => false,
            };
            mouse4_was_down = mouse4_is_down;
            mouse5_was_down = mouse5_is_down;
            if selected_mouse_button_pressed {
                if click_options.read().unwrap().click_type == ClickType::Continuous && clicking_allowed.load(Ordering::Acquire) {
                    let next = !continuous_enabled.fetch_xor(true, Ordering::AcqRel);
                    if running.swap(false, Ordering::AcqRel) {
                        let _ = events.send(EngineEvent::Toggle(false));
                    }
                    let _ = events.send(EngineEvent::ContinuousEnabled(next));
                } else {
                    if let Some(next) = toggle_if_allowed(&running, &clicking_allowed, &schedule_generation) {
                        let _ = events.send(EngineEvent::Toggle(next));
                    }
                }
            }
            if result.as_bool() {
                let _ = DispatchMessageW(&msg);
            }
            if !result.as_bool() {
                thread::sleep(Duration::from_millis(3));
            }
        }
    }
}
#[cfg(not(windows))]
fn hotkey_loop(
    rx: Receiver<HotkeyCommand>,
    _initial: Hotkey,
    _running: Arc<AtomicBool>,
    _clicking_allowed: Arc<AtomicBool>,
    _schedule_generation: Arc<AtomicU64>,
    _click_options: Arc<RwLock<ClickOptions>>,
    _continuous_enabled: Arc<AtomicBool>,
    _events: Sender<EngineEvent>,
    ready: Sender<Result<bool, String>>,
) {
    let _ = ready.send(Err("Global hotkeys require Windows".into()));
    while let Ok(HotkeyCommand::Set(_, reply)) = rx.recv() {
        let _ = reply.send(Err("Global hotkeys require Windows".into()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::Hotkey;

    fn options(mode: InputMode, click_type: ClickType) -> ClickOptions {
        ClickOptions {
            mode,
            mouse_button: MouseButton::Left,
            mouse_buttons: vec![MouseButton::Left],
            click_type,
            keyboard_key: 0x20,
        }
    }

    #[test]
    fn measured_cps_handles_empty_window() {
        assert_eq!(measured_cps(100, 0), 0.0);
    }

    #[test]
    fn measured_cps_converts_elapsed_window() {
        assert!((measured_cps(120, 1_000) - 120.0).abs() < f64::EPSILON);
        assert!((measured_cps(60, 500) - 120.0).abs() < f64::EPSILON);
    }

    #[test]
    fn constructor_rejects_nonfinite_cps_before_starting_threads() {
        assert!(ClickEngine::new(f64::NAN, Hotkey::default()).is_err());
        assert!(ClickEngine::new(f64::INFINITY, Hotkey::default()).is_err());
        assert!(ClickEngine::new(0.0, Hotkey::default()).is_err());
    }

    #[test]
    fn configured_interval_counts_individual_clicks_for_double_click_mode() {
        let single = interval_for_rate(100_000, 10_000_000, RateEntryMode::Cps, options(InputMode::Mouse, ClickType::Single));
        let double = interval_for_rate(100_000, 10_000_000, RateEntryMode::Cps, options(InputMode::Mouse, ClickType::Double));
        let keyboard = interval_for_rate(100_000, 10_000_000, RateEntryMode::Cps, options(InputMode::Keyboard, ClickType::Double));

        assert_eq!(single, Duration::from_millis(10));
        assert_eq!(double, Duration::from_millis(20));
        assert_eq!(keyboard, Duration::from_millis(10));
        assert_eq!(
            clicks_per_action(options(InputMode::Mouse, ClickType::Double)),
            2
        );
        assert_eq!(interval_for_rate(100_000, 86_400_000_000_000, RateEntryMode::Interval, options(InputMode::Mouse, ClickType::Single)), Duration::from_secs(86_400));
    }

    #[test]
    fn toggle_hotkey_is_ignored_when_clicking_is_disallowed() {
        let running = AtomicBool::new(false);
        let allowed = AtomicBool::new(false);

        let generation = AtomicU64::new(0);
        assert_eq!(toggle_if_allowed(&running, &allowed, &generation), None);
        assert!(!running.load(Ordering::Acquire));
    }

    #[test]
    fn toggle_hotkey_still_works_when_clicking_is_allowed() {
        let running = AtomicBool::new(false);
        let allowed = AtomicBool::new(true);

        let generation = AtomicU64::new(0);
        assert_eq!(toggle_if_allowed(&running, &allowed, &generation), Some(true));
        assert!(running.load(Ordering::Acquire));
        assert_eq!(toggle_if_allowed(&running, &allowed, &generation), Some(false));
        assert!(!running.load(Ordering::Acquire));
    }

    #[test]
    fn mouse_four_and_five_are_recognized_as_mouse_hotkeys() {
        assert!(is_mouse_button_hotkey(VK_XBUTTON1_CODE));
        assert!(is_mouse_button_hotkey(VK_XBUTTON2_CODE));
        assert!(!is_mouse_button_hotkey(0x75));
    }
}
