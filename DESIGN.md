# vibeclicker Design

## Product direction

vibeclicker is a compact desktop autoclicker with a Rust input engine and a Tauri v2 frontend. The interface uses semantic HTML, Tailwind CSS, Lucide icons, and the bundled Geist variable font. It does not use DaisyUI. Tauri loads the frontend in the platform webview; end users do not need Rust installed.

## Visual language

- Charcoal surfaces, quiet borders, and neutral contrast in dark mode; matching light surfaces in light mode.
- Follow the operating-system theme by default, with saved System, Light, and Dark choices.
- Compact modular cards, restrained typography, and segmented controls. Avoid hero content and marketing copy.
- Settings and options use concise labels; avoid explanatory descriptions or helper copy inside controls.
- The CPS interval card uses a Behavior segmented control with Static and Human-like options below the CPS input, aligned with the neighboring Click type field.
- The top bar contains Main and Settings navigation plus a toggle switch. In Continuous mode it enables or disables continuous clicking; otherwise it shows idle/active status.
- The main view presents clicks per second and its derived interval, mouse/keyboard output settings, and an action type. Action type changes apply immediately. Single and Double use one mouse button, the global toggle hotkey, and full-width Start/Stop action. Continuous allows multiple mouse buttons, each at the configured CPS, uses any selected button or keyboard key as a hold trigger, and uses both the configured global hotkey and the top-bar toggle as an enable gate. The hotkey card remains visible in Continuous mode.
- Settings contain appearance and always-on-top controls, a Never/Minimize/Close tray behavior selector, an in-app update check, and the GitHub profile link.
- The window is resizable; layout should adapt without fixed-width screens or clipped controls.
- Window bounds are 420–700 px wide with a 558 px default and minimum height. Horizontal resizing remains enabled; the frontend measures the current content after width or layout changes and updates the native maximum height, clamping it between 558 and 1600 px and resizing the window to that exact height so content does not scroll. At the default 700 px width, the landscape layout fits without a vertical scrollbar, and the visible gap below the Toggle Hotkey card matches the 18px navbar-to-first-card gap. The height accounts for the four interval fields fitting on one row at the landscape breakpoint.

## Behavior and safety

- The Rust engine owns click/key injection, keyboard and Mouse 4/5 global hotkeys, saved settings, and authoritative run state.
- The frontend invokes commands and consumes Tauri events; it owns presentation and input capture feedback.
- Mouse output supports left, right, or middle single clicks, double clicks, or continuous clicking. Keyboard mode supports single repeated key presses or Continuous mode with a configurable key. In Continuous mode, holding any selected mouse button or configured key for at least 80 ms starts repetition at the selected rate; releasing the held trigger stops. A shorter press passes through as a normal physical click/key press and does not start generated input. CPS mode optionally applies bounded human-like fluctuations of up to 10% around the configured rate.
- CPS means individual click/key actions per second. Double-click mode spaces paired mouse clicks so the reported physical click rate still matches the selected CPS.
- In keyboard Single and Double modes, the output key must differ from the toggle hotkey. Mouse clicks pause over the application, and key injection pauses while the app is foreground.
- Opening Settings stops clicking and temporarily disables the toggle hotkey; returning to Main allows it again. In Continuous mode, a held trigger must be released before clicking can resume after Settings closes.
- The output key and toggle hotkey must differ in every keyboard mode because the global hotkey gates Continuous mode as well.
- Settings persist locally. System theme tracks Windows unless the user selects a saved override.
- Updates use the signed NSIS installer path. Settings offers a concise Check for updates action with inline checking, download progress, install, and error states.

## Platform and verification

The current release target is Windows. Linux and macOS need platform-specific input backend validation and packaging. The Windows release executable was built at 9.4 MiB; this is executable size, not installed size or runtime memory. Runtime memory should be measured with the complete WebView2 process group. No controlled foreground-target CPS acceptance test has been verified in this runner.

`npm run build`, `cargo test --manifest-path src/Cargo.toml`, and a Tauri Windows `--no-bundle` release build are the current local verification steps. Releases use a tag-triggered GitHub Actions workflow that publishes signed NSIS updater artifacts and `latest.json`. No controlled foreground-target CPS acceptance test has been performed.
