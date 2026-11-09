# vibeclicker Design

## Product direction

vibeclicker is a compact desktop autoclicker with a Rust input engine and a Tauri v2 frontend. The interface uses semantic HTML, Tailwind CSS, Lucide icons, and the bundled Geist variable font. It does not use DaisyUI. Tauri loads the frontend in the platform webview; end users do not need Rust installed.

## Visual language

- Charcoal surfaces, quiet borders, and neutral contrast in dark mode; matching light surfaces in light mode.
- Follow the operating-system theme by default, with saved System, Light, and Dark choices.
- Compact modular cards, restrained typography, and segmented controls. Avoid hero content and marketing copy.
- The top bar contains Main and Settings navigation plus a read-only toggle switch showing idle/active status.
- The main view presents clicks per second and its derived interval, mouse/keyboard output settings, a global toggle hotkey, and a full-width start/stop action.
- Settings contain appearance and always-on-top controls, a Never/Minimize/Close tray behavior selector, and the GitHub profile link.
- The window is resizable; layout should adapt without fixed-width screens or clipped controls.
- Window bounds are 420–700 px wide and fixed at 558 px high. Horizontal resizing remains enabled; vertical resizing is disabled by equal minimum and maximum heights. At the fixed height, the landscape layout fits without a vertical scrollbar and the gap above the Start/Stop button matches the gap between the tab switcher and the first card. The height accounts for the four interval fields fitting on one row at the landscape breakpoint.

## Behavior and safety

- The Rust engine owns click/key injection, keyboard and Mouse 4/5 global hotkeys, saved settings, and authoritative run state.
- The frontend invokes commands and consumes Tauri events; it owns presentation and input capture feedback.
- Mouse output supports left, right, or middle single/double clicks. Keyboard mode repeats a configurable key at the selected rate.
- CPS means individual click/key actions per second. Double-click mode spaces paired mouse clicks so the reported physical click rate still matches the selected CPS.
- The output key cannot equal the toggle hotkey key. Mouse clicks pause over the application, and key injection pauses while the app is foreground.
- Opening Settings stops clicking and temporarily disables the toggle hotkey; returning to Main allows it again.
- Settings persist locally. System theme tracks Windows unless the user selects a saved override.

## Platform and verification

The current release target is Windows. Linux and macOS need platform-specific input backend validation and packaging. The Windows release executable was built at 9.4 MiB; this is executable size, not installed size or runtime memory. Runtime memory should be measured with the complete WebView2 process group. No controlled foreground-target CPS acceptance test has been verified in this runner.

`npm run build`, `cargo test --manifest-path src/Cargo.toml`, and a Tauri Windows `--no-bundle` release build are the current local verification steps. MSI/NSIS packaging also requires the WiX toolchain; its download was blocked in the restricted build environment during this verification.
