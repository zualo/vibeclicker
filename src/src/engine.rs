use crate::settings::{ClickType, Hotkey, InputMode, MouseButton, RateEntryMode};
use std::sync::atomic::AtomicIsize;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{
    mpsc::{self, Receiver, Sender},
    Arc, Mutex, RwLock,
};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

const VK_XBUTTON1_CODE: u32 = 0x05;
const VK_XBUTTON2_CODE: u32 = 0x06;

#[derive(Debug, Clone)]
pub enum EngineEvent {
    Toggle(bool),
    HotkeyError(String),
    RateSample { clicks: u64, elapsed_ms: u64 },
}

#[derive(Clone, Copy)]
pub struct ClickOptions {
    pub mode: InputMode,
    pub mouse_button: MouseButton,
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
            click_type: ClickType::Single,
            keyboard_key: 0x20,
        }));
        let excluded_window = Arc::new(AtomicIsize::new(0));
        let clicks = Arc::new(AtomicU64::new(0));
        let (event_tx, event_rx) = mpsc::channel();
        let (hotkey_tx, hotkey_rx) = mpsc::channel();
        let (ready_tx, ready_rx) = mpsc::channel();

        let click_running = running.clone();
        let click_allowed = clicking_allowed.clone();
        let click_shutdown = shutdown.clone();
        let click_rate = interval_ns.clone();
        let click_cps = cps_milli.clone();
        let click_generation = schedule_generation.clone();
        let click_mode = rate_entry_mode.clone();
        let click_options_thread = click_options.clone();
        let excluded_window_thread = excluded_window.clone();
        let click_count = clicks.clone();
        let click_events = event_tx.clone();
        let click_thread = thread::spawn(move || {
            let mut next = Instant::now();
            let mut observed_generation = click_generation.load(Ordering::Acquire);
            let mut window_start = Instant::now();
            let mut window_clicks = 0u64;
            while !click_shutdown.load(Ordering::Acquire) {
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
                let options = *click_options_thread.read().unwrap();
                let interval = interval_for_rate(click_cps.load(Ordering::Relaxed), interval_rate, *click_mode.read().unwrap(), options);
                let excluded = excluded_window_thread.load(Ordering::Relaxed);
                let safe_target = match options.mode {
                    InputMode::Mouse => !cursor_over_excluded_window(excluded),
                    InputMode::Keyboard => !excluded_window_is_foreground(excluded),
                };
                if safe_target && send_input(options) {
                    let accepted_clicks = clicks_per_action(options);
                    click_count.fetch_add(accepted_clicks, Ordering::Relaxed);
                    window_clicks += accepted_clicks;
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
        let hotkey_thread = thread::spawn(move || {
            hotkey_loop(
                hotkey_rx,
                hotkey,
                hotkey_running,
                hotkey_allowed,
                hotkey_generation,
                hotkey_events,
                ready_tx,
            )
        });

        match ready_rx.recv_timeout(Duration::from_secs(2)) {
            Ok(Ok(())) => {}
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
        }

        Ok(Self {
            running,
            clicking_allowed,
            shutdown,
            interval_ns,
            cps_milli,
            schedule_generation,
            rate_entry_mode,
            click_options,
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

    pub fn set_click_options(&self, options: ClickOptions) {
        *self.click_options.write().unwrap() = options;
        self.schedule_generation.fetch_add(1, Ordering::Release);
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
    events: Sender<EngineEvent>,
    ready: Sender<Result<(), String>>,
) {
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        GetAsyncKeyState, RegisterHotKey, UnregisterHotKey, HOT_KEY_MODIFIERS,
    };
    use windows::Win32::UI::WindowsAndMessaging::{PeekMessageW, MSG, PM_REMOVE, WM_HOTKEY};
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
        let _ = ready.send(Ok(()));
        let mut msg = MSG::default();
        let mut mouse4_was_down = GetAsyncKeyState(VK_XBUTTON1_CODE as i32) < 0;
        let mut mouse5_was_down = GetAsyncKeyState(VK_XBUTTON2_CODE as i32) < 0;
        loop {
            while let Ok(command) = rx.try_recv() {
                match command {
                    HotkeyCommand::Stop => {
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
                if let Some(next) = toggle_if_allowed(&running, &clicking_allowed, &schedule_generation) {
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
                if let Some(next) = toggle_if_allowed(&running, &clicking_allowed, &schedule_generation) {
                    let _ = events.send(EngineEvent::Toggle(next));
                }
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
    _events: Sender<EngineEvent>,
    ready: Sender<Result<(), String>>,
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
