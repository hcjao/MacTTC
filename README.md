# MacTTC

Current version: `0.9.9`

MacTTC is a macOS Tauri app for updating the Tamriel Trade Centre AddOn price table for The Elder Scrolls Online.

The app starts as a macOS menu bar status item. The main window is hidden on startup and can be opened from the status menu.

## Interface

### Main Window

The main window includes:

- App title: `MacTTC`
- Language switch: `中文` and `English`
- Download source selector:
  - `NA` with a United States icon
  - `EU` with a European Union icon
- Current selected download URL
- Fixed destination folder path
- Destination folder availability message
- Button to reveal the destination folder
- `進行下載` / `Download` button
- Status area showing:
  - Start time
  - Finish time
  - Last successful download time

The app window uses these size limits:

- Initial width: `900`
- Initial height: `700`
- Minimum width: `720`
- Minimum height: `680`

### Status Menu

The menu bar status item includes:

- `開啟MacTTC` / `Open MacTTC`
- `開機啟動` / `Launch at Login`
- `下載排程` / `Download Schedule` as a disabled section label
- `關閉排程` / `Turn Schedule Off`
- `每 3 小時` / `Every 3 hours`
- `目前資料時間` / `Current Data Time`
- Last successful download time, or `尚無成功下載紀錄` / `No successful download yet`
- `前往TTC網站` / `Go to TTC Website`
- `退出` / `Quit`

The status menu language follows the primary macOS system language. If the primary system language is Chinese, the status menu uses Traditional Chinese. All other primary system languages use English.

The main window language can be switched between Traditional Chinese and English. On first launch, a Chinese primary system language uses Chinese; all other primary system languages use English. The app remembers the last selected main-window language and uses it on the next launch.

## Features

- Manual price table update from the main window.
- Optional menu bar download schedule every 3 hours.
- Optional launch at login.
- Download source is limited to two fixed options:
  - NA: `https://us.tamrieltradecentre.com/download/PriceTable`
  - EU: `https://eu.tamrieltradecentre.com/download/PriceTable`
- Destination folder is fixed to:
  - `~/Documents/Elder Scrolls Online/live/AddOns/TamrielTradeCentre`
- The app checks whether the destination folder exists on startup.
- The download button is disabled when the destination folder is missing.
- Errors are shown with native popup dialogs.
- Closing the main window hides it instead of quitting the app.
- The app remembers the last successful download time and the source URL used for that successful run.
- The status menu can open the TTC Trade website for the recorded download source:
  - NA: `https://us.tamrieltradecentre.com/pc/Trade`
  - EU: `https://eu.tamrieltradecentre.com/pc/Trade`
- When launch at login is enabled, the app opens at login and uses the normal startup download behavior.

## Mechanisms

### Download Flow

When the user clicks `進行下載` / `Download`:

1. Save the selected source preference.
2. Validate the source URL against the allowlist.
3. Recalculate and validate the fixed destination folder.
4. Download the archive from the selected source.
5. Save it temporarily in the app cache directory.
6. Extract it into the destination folder.
7. Delete the temporary archive.
8. Update status timestamps.
9. Persist the successful download time and successful source URL.

Only one download may be active at a time.

### Startup Download

When MacTTC opens, the status message area starts empty.

The Rust backend automatically triggers one download unless the last successful download time is less than 1 hour before the current time.

MacTTC checks the saved app settings for:

- Last successful download time
- Last successful source URL

If a valid successful source URL exists, the startup download uses that recorded source URL. If no successful source URL exists, the startup download uses the currently saved source URL.

Saved source URLs must pass the backend allowlist validation. Invalid or unknown saved successful source URLs are ignored and cleared.

Startup download status changes are emitted from the Rust backend to the frontend, so the UI reflects background runs without requiring a manual refresh.

### Destination Folder

The destination is not editable by the user.

The backend resolves the destination as:

```text
$HOME/Documents/Elder Scrolls Online/live/AddOns/TamrielTradeCentre
```

If the folder does not exist:

- The UI shows the resolved absolute path and a missing-folder message.
- The download button is disabled.
- The backend rejects execution.
- The app does not create the folder automatically.

### Schedule

The macOS menu bar status menu includes a disabled schedule section label.

The user can choose exactly one schedule state:

```text
Off
3 hours
```

The scheduled job uses the currently saved source URL and the fixed destination folder. If a job is already running, the next scheduled job does not start another concurrent run.

### Launch at Login

When launch at login is enabled:

- Enable startup registration through the Tauri autostart plugin.
- On macOS, the plugin uses the LaunchAgent launcher under Tauri's managed autostart flow.
- After startup, use the normal startup download behavior.

When launch at login is disabled:

- Disable startup registration through the Tauri autostart plugin.

### TTC Website

The status menu `前往TTC網站` / `Go to TTC Website` item opens the Trade page for the recorded successful download source.

If the recorded source is NA, the app opens:

```text
https://us.tamrieltradecentre.com/pc/Trade
```

If the recorded source is EU, the app opens:

```text
https://eu.tamrieltradecentre.com/pc/Trade
```

If no successful source has been recorded yet, the app falls back to the currently saved source URL.

### Storage

App settings are stored in the Tauri app config directory.

Saved settings include:

- Selected source URL
- Normalized fixed destination path
- Schedule toggle
- Schedule interval
- Launch-at-login toggle
- Last successful download time
- Last successful source URL

ZIP extraction uses path traversal protection before writing entries into the destination folder.

## Build

### Requirements

- macOS
- Node.js with npm
- Rust toolchain with Cargo
- Tauri macOS prerequisites

### Install Dependencies

```sh
npm install
```

### Run in Development

```sh
npm run tauri:dev
```

### Build for Development Testing

During normal development and testing, build only the `.app` bundle:

```sh
npm run tauri -- build --bundles app
```

Expected output:

```text
src-tauri/target/release/bundle/macos/MacTTC.app
```

### Build for Release

Formal release packaging should follow the project release policy in `docs/release.md`.

The full Tauri release build can produce:

```text
src-tauri/target/release/bundle/macos/MacTTC.app
src-tauri/target/release/bundle/dmg/MacTTC_0.9.9_aarch64.dmg
```

## Other

Implementation stack:

- Frontend: TypeScript and Vite
- Desktop shell: Tauri v2
- Backend: Rust
- Downloads: `reqwest`
- ZIP extraction: Rust `zip` crate

Backend modules:

- `config.rs`: app settings, fixed paths, source allowlist, destination validation
- `downloader.rs`: download job, archive download, ZIP extraction, success recording
- `localization.rs`: status menu language labels and macOS language detection
- `scheduler.rs`: startup download and 3-hour schedule
- `tray.rs`: macOS status menu setup and menu actions
- `lib.rs`: Tauri setup, commands, app state, launch-at-login wiring

User-facing errors must be shown in a native popup dialog. The UI avoids repeatedly showing the same error if it is already visible or was just shown.

The app should quit only through the status menu quit item or normal system termination.
