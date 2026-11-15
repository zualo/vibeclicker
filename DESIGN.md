# vibeclicker Design

## Product direction

vibeclicker is a compact desktop autoclicker with a Rust input engine and a Tauri v2 frontend. The interface uses semantic HTML, Tailwind CSS, Lucide icons, and the bundled Geist variable font. It does not use DaisyUI. Tauri loads the frontend in the platform webview; end users do not need Rust installed.

## Visual language

- Charcoal surfaces, quiet borders, and neutral contrast in dark mode; matching light surfaces in light mode.
- Follow the operating-system theme by default, with saved System, Light, and Dark choices.
- Compact modular cards, restrained typography, and segmented controls. Avoid hero content and marketing copy.
- Settings and options use concise labels; avoid explanatory descriptions or helper copy inside controls.
- Reuse the shared interface rules below for new features. Extend an existing component or token when possible; do not introduce feature-specific toast styles, button colors, or control behavior without updating this design system.
- The CPS interval card uses a Behavior segmented control with Machine and Human options below the CPS input, aligned with the neighboring Click type field.
- The top bar contains Main and Settings navigation plus a toggle switch. In Continuous mode it enables or disables continuous clicking; otherwise it shows idle/active status.
- The main view presents clicks per second and its derived interval, mouse/keyboard output settings, and an action type. Action type changes apply immediately. Single and Double use one mouse button, the global toggle hotkey, and full-width Start/Stop action. Continuous allows multiple mouse buttons, each at the configured CPS, uses any selected button or keyboard key as a hold trigger, and uses both the configured global hotkey and the top-bar toggle as an enable gate. The hotkey card remains visible in Continuous mode.
- Settings contain appearance and always-on-top controls, a Never/Minimize/Close tray behavior selector, an in-app update check, and the GitHub profile link. Installed app launches also check for updates in the background; a centered toast reports an available update or a check failure, and installation still requires the user's confirmation.
- The window is resizable; layout should adapt without fixed-width screens or clipped controls.
- Window bounds are 420–700 px wide with a 558 px default and minimum height. Horizontal resizing remains enabled; the frontend measures the current content after width or layout changes and updates the native maximum height, clamping it between 558 and 1600 px and resizing the window to that exact height so content does not scroll. At the default 700 px width, the landscape layout fits without a vertical scrollbar, and the visible gap below the Toggle Hotkey card matches the 18px navbar-to-first-card gap. The height accounts for the four interval fields fitting on one row at the landscape breakpoint.

## Shared interface rules

- **Toasts:** Use one centered, transient toast for status, success, update, and error messages. Keep its raised surface, quiet border, compact text, soft shadow, and brief entrance/fade motion. Use the shared danger text and border treatment for errors. Keep the live status announcement. Toasts do not blur or block the app; the blur is reserved for active keybind capture, where it blocks pointer input until a key is selected or Escape cancels.
- **Buttons:** Reuse the existing button variants. Primary actions use the accent surface and accent text; secondary actions use the raised surface, standard text, and quiet border, with the selected surface on hover. Segmented choices use the field surface and selected surface for the active option. Destructive or error states use the danger tokens. Do not add one-off button colors or hover treatments.
- **States and input:** Keep disabled and busy feedback consistent within each button variant, keep visible keyboard focus, and preserve each control's existing keyboard behavior. Do not make disabled secondary actions display a wait cursor.
- **Color and theme:** Draw new component colors from the existing CSS custom properties so the same rule works in dark, light, and system themes. Add a shared token when the existing palette cannot express a needed state; avoid hard-coded feature-specific colors.

## Behavior and safety

- The Rust engine owns click/key injection, keyboard and Mouse 4/5 global hotkeys, saved settings, and authoritative run state.
- The frontend invokes commands and consumes Tauri events; it owns presentation and input capture feedback.
- Mouse output supports left, right, or middle single clicks, double clicks, or continuous clicking. Keyboard mode supports single repeated key presses or Continuous mode with a configurable key. In Continuous mouse mode, each selected output button is independently mapped to its physical trigger: holding Left, Right, or Middle for at least 80 ms generates only that button's clicks, while holding multiple selected buttons generates each held button at the configured rate. A shorter press passes through as a normal physical click/key press and does not start generated input. Continuous keyboard mode uses the configured key as its trigger and output. CPS mode optionally applies bounded human-like fluctuations of up to 10% around the configured rate.
- CPS means individual click/key actions per second. Double-click mode spaces paired mouse clicks so the reported physical click rate still matches the selected CPS.
- In keyboard Single and Double modes, the output key must differ from the toggle hotkey. Mouse clicks pause over the application, and key injection pauses while the app is foreground.
- Opening Settings stops clicking and temporarily disables the toggle hotkey; returning to Main allows it again. In Continuous mode, a held trigger must be released before clicking can resume after Settings closes. While a keybind is being captured, a blurred overlay blocks pointer interaction with the app until a key is selected or Escape cancels; keyboard capture stays active.
- The output key and toggle hotkey must differ in every keyboard mode because the global hotkey gates Continuous mode as well.
- Settings persist locally. System theme tracks Windows unless the user selects a saved override.
- Updates use the signed NSIS installer path. Settings offers a concise Check for updates action with inline checking, download progress, install, and error states. A background update check runs on installed app startup without interrupting use or installing automatically. Update availability and check errors use the shared centered toast. GitHub Releases publish the updater signature inside `latest.json` without a separate `.sig` asset.

## Platform and verification

The current release target is Windows. Linux and macOS need platform-specific input backend validation and packaging. The Windows release executable was built at 9.4 MiB; this is executable size, not installed size or runtime memory. Runtime memory should be measured with the complete WebView2 process group. No controlled foreground-target CPS acceptance test has been verified in this runner.

`npm run build`, `cargo test --manifest-path src/Cargo.toml`, and a Tauri Windows `--no-bundle` release build are the current local verification steps. Releases use a tag-triggered GitHub Actions workflow that publishes signed NSIS updater artifacts and `latest.json`; standalone updater signature assets are omitted. No controlled foreground-target CPS acceptance test has been performed.
