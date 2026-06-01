# Changelog

## 0.9.9 - 2026-06-01

### Changed

- Reorganized the Rust backend by splitting `lib.rs` into focused modules for config, downloading, localization, scheduling, and tray menu behavior.
- Updated the main window destination folder display so the label sits inside the path display while the reveal-folder button stays outside the display box.
- Renamed the main download button to `進行下載` / `Download`.
- Updated the status menu text to `開啟MacTTC` / `Open MacTTC`.
- Moved the status menu launch-at-login option before the download schedule section.
- Added a status menu data-time section showing the last successful download time, or an empty-state message when no successful download exists.
- Added a status menu item for opening the TTC Trade website for the recorded NA or EU source.
- Updated the TTC website status menu text to `前往TTC網站` / `Go to TTC Website`.

## 0.9.7 - 2026-05-30

### Changed

- Refreshed the macOS status menu language when the tray icon is clicked, so the menu text follows the current system language before the menu is opened.
- Kept normal development builds focused on the `.app` bundle and documented the safer Tauri packaging policy in `docs/release.md`.

### Removed

- Removed the abandoned automatic upload implementation and its backend remnants, including SavedVariables monitoring, TTC WebClient upload code, Lua parsing, upload logging, and upload-related config fields.
- Removed hidden frontend status/message DOM blocks that were no longer displayed.
- Removed unused frontend translation keys and CSS associated with hidden status summary content.

## 0.9.5 - 2026-05-29

### Baseline

- Menu bar TTC price table downloader for macOS.
- Manual run button, optional 3-hour schedule, and launch-at-login support.
- macOS status menu support.
