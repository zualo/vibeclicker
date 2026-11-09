# Product

<!-- impeccable:product-schema 1 -->

## Platform

web

## Users

General-purpose users who want to automate repetitive mouse clicks or keyboard input. Confirmed use cases include game interactions and interval-based clicking; other users may use it wherever repeated input is useful.

## Product Purpose

vibeclicker provides configurable repeated mouse clicks and keyboard input, controlled by a global toggle hotkey. Success means users can choose an input, rate, and toggle key and have the repeated action work predictably while they use another application.

## Positioning

Undecided. No unique mechanism or competitive differentiator has been confirmed; do not invent comparative claims.

## Operating Context

- A Windows desktop utility used alongside another application, including games and other software where repeated input is useful.
- Users configure input and timing in the app, then start and stop the action with a global hotkey.
- Settings are stored locally and applied automatically.

## Capabilities and Constraints

- The click engine is written in Rust and the desktop interface is built with Tauri v2, HTML, Tailwind CSS, Lucide icons, and the bundled Geist font.
- Users do not need to install Rust to run the app.
- Mouse output supports left, right, or middle buttons with single or double click modes. Keyboard output repeats a configurable key.
- Users can set clicks per second and edit the derived interval in hours, minutes, seconds, and milliseconds. CPS counts individual click or key actions; double-click mode spaces pairs so the selected CPS remains the individual-action rate.
- Toggle hotkeys support keyboard keys and Mouse 4/5. The repeated keyboard key cannot match the toggle hotkey.
- Preferences persist locally, including system/light/dark appearance, always-on-top, and the selected tray behavior.
- Clicking stops when Settings opens and the global toggle hotkey is disabled until Main is active again. Input injection pauses while vibeclicker is foreground.
- The initial release target is Windows. Linux and macOS are possible future targets after global input and packaging are validated on each platform.
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

- Add in-app updates with the Tauri updater plugin, signed update artifacts, and GitHub Releases or a hosted update manifest. Users should be able to check for updates, download one, and install it from within the app.

