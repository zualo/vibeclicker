# Product

<!-- impeccable:product-schema 1 -->

## Platform

web

## Users

General-purpose users who want to automate repetitive mouse clicks or keyboard input. Confirmed use cases include game interactions and interval-based clicking; other users may use it wherever repeated input is useful.

## Product Purpose

vibeclicker provides configurable repeated mouse clicks and keyboard input. Single and Double actions use a global toggle hotkey; Continuous uses a held trigger and a top-bar enable gate. Success means users can choose an input and rate and have the repeated action work predictably while they use another application.

## Positioning

Undecided. No unique mechanism or competitive differentiator has been confirmed; do not invent comparative claims.

## Operating Context

- A Windows desktop utility used alongside another application, including games and other software where repeated input is useful.
- Users configure input and timing in the app, then start and stop Single or Double actions with a global hotkey, or use a held trigger for Continuous mode with its top-bar enable gate.
- Settings are stored locally and applied automatically.

## Capabilities and Constraints

- The click engine is written in Rust and the desktop interface is built with Tauri v2, HTML, Tailwind CSS, Lucide icons, and the bundled Geist font.
- Users do not need to install Rust to run the app.
- Mouse output supports left, right, or middle buttons with single or double click modes. Continuous mouse mode can select multiple buttons, with each button receiving the configured CPS. Keyboard output repeats a configurable key.
- Continuous mode starts repeating after any selected mouse button or configured keyboard key has been held for 80 ms. Releasing the held trigger stops clicking; a shorter press is passed through normally and does not start repetition. The top-bar toggle enables or disables continuous mode. Opening Settings stops clicking, and the held input must be released before continuous clicking can resume.
- Users can set clicks per second and edit the derived interval in hours, minutes, seconds, and milliseconds. CPS counts individual click or key actions; double-click mode spaces pairs so the selected CPS remains the individual-action rate.
- In clicks per second mode, users can enable Human-like Behavior to vary the effective click rate within 10% above or below the selected CPS.
- Toggle hotkeys support keyboard keys and Mouse 4/5. In keyboard Single and Double modes, the repeated key must differ from the toggle hotkey.
- In Continuous mode the configured global hotkey and top-bar switch enable or disable the held-trigger gate. The repeated keyboard key must differ from the toggle hotkey in every keyboard mode.
- Preferences persist locally, including system/light/dark appearance, always-on-top, and the selected tray behavior.
- Settings stops clicking while it is open. The global toggle hotkey is disabled until Main is active again. Input injection pauses while vibeclicker is foreground.
- Settings includes a Check for updates action. Updates are signed, published through GitHub Releases, downloaded through the NSIS updater path, and installed with progress feedback; the first updater-enabled release requires a manual install for existing 0.1.0 users.
- The window defaults to 700×558 px, with a 420–700 px width range and 558 px minimum height. After width or layout changes, the frontend measures the content and updates the native maximum height, then resizes to the measured height to avoid a scrollbar.
- The initial release target is Windows. Linux and macOS are possible future targets after global input and packaging are validated on each platform. New releases use NSIS as the canonical Windows installer and updater format.
- Runtime memory targets and comparative performance are undecided and have not been validated; do not make performance claims.

## Brand Commitments

- Product name: `vibeclicker`, always lowercase.
- The About link opens the maintainer's GitHub profile: https://github.com/zualo.

## Evidence on Hand

- The project contains a runnable Windows desktop application and current implementation source.
- No validated performance benchmarks, memory measurements, or competitive comparisons are available. Do not fabricate them.

## Product Principles

- Support general-purpose repetitive input rather than a single use case.
- Make input type, action rate, and toggle behavior configurable.
- Keep operation predictable and safe while users work in other applications.
- Persist user preferences locally and require no Rust toolchain from end users.

## Future Roadmap

- Continue improving release notes and update messaging as the release process matures.

