# Changelog

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
