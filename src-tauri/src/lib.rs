use chrono::{DateTime, Local};
use serde::Serialize;
use std::{
    env,
    error::Error,
    fs, io,
    path::{Path, PathBuf},
    sync::Mutex,
};
use tauri::{async_runtime::JoinHandle, ActivationPolicy, AppHandle, Manager, State, WindowEvent};

mod config;
mod downloader;
mod localization;
mod scheduler;
mod tray;

use config::{
    default_schedule_interval_hours, destination_status, fixed_destination_string,
    is_allowed_download_url, load_config, normalized_config, save_config_to_disk, AppConfig,
    DestinationStatus,
};
use downloader::run_job;
use scheduler::{maybe_run_startup_download, restart_scheduler};
use tray::setup_tray;

const LAUNCH_AGENT_LABEL: &str = "com.macttc.downloader";
const JOB_STATUS_CHANGED_EVENT: &str = "job-status-changed";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct JobStatus {
    running: bool,
    last_started_at: Option<String>,
    last_finished_at: Option<String>,
    last_success_at: Option<String>,
    last_error: Option<String>,
    source_url: Option<String>,
    destination_dir: Option<String>,
    message: String,
}

impl Default for JobStatus {
    fn default() -> Self {
        Self {
            running: false,
            last_started_at: None,
            last_finished_at: None,
            last_success_at: None,
            last_error: None,
            source_url: None,
            destination_dir: None,
            message: String::new(),
        }
    }
}

impl JobStatus {
    fn from_config(config: &AppConfig) -> Self {
        let mut status = JobStatus::default();
        if let (Some(last_success_at), Some(source_url)) = (
            config.last_success_at.clone(),
            config.last_success_source_url.clone(),
        ) {
            status.last_success_at = Some(last_success_at);
            status.source_url = Some(source_url);
            status.destination_dir = Some(config.destination_dir.clone());
        }
        status
    }
}

#[derive(Default)]
struct AppState {
    config: Mutex<AppConfig>,
    status: Mutex<JobStatus>,
    scheduler: Mutex<Option<JoinHandle<()>>>,
    tray_controls: Mutex<Option<tray::TrayControls>>,
}

#[tauri::command]
fn get_config(state: State<'_, AppState>) -> Result<AppConfig, String> {
    state
        .config
        .lock()
        .map(|config| config.clone())
        .map_err(lock_error)
}

#[tauri::command]
fn get_status(state: State<'_, AppState>) -> Result<JobStatus, String> {
    state
        .status
        .lock()
        .map(|status| status.clone())
        .map_err(lock_error)
}

#[tauri::command]
fn get_destination_status() -> Result<DestinationStatus, String> {
    destination_status()
}

#[tauri::command]
fn set_download_source(
    app: AppHandle,
    state: State<'_, AppState>,
    url: String,
) -> Result<AppConfig, String> {
    if !is_allowed_download_url(&url) {
        return Err("下載來源只能選擇 NA 或 EU".to_string());
    }

    let mut config = state.config.lock().map_err(lock_error)?;
    config.url = url;
    config.destination_dir = fixed_destination_string()?;
    config.schedule_interval_hours = default_schedule_interval_hours();
    save_config_to_disk(&app, &config)?;
    Ok(config.clone())
}

#[tauri::command]
async fn run_now(
    app: AppHandle,
    state: State<'_, AppState>,
    config: AppConfig,
) -> Result<JobStatus, String> {
    let mut config = normalized_config(config)?;
    {
        let mut stored = state.config.lock().map_err(lock_error)?;
        config.schedule_enabled = stored.schedule_enabled;
        config.schedule_interval_hours = default_schedule_interval_hours();
        config.autostart_enabled = stored.autostart_enabled;
        config.last_success_at = stored.last_success_at.clone();
        config.last_success_source_url = stored.last_success_source_url.clone();
        *stored = config.clone();
        save_config_to_disk(&app, &stored)?;
    }

    run_job(app, state.inner(), config).await?;
    get_status(state)
}

#[tauri::command]
fn reveal_destination(state: State<'_, AppState>) -> Result<(), String> {
    let destination = state
        .config
        .lock()
        .map_err(lock_error)?
        .destination_dir
        .clone();
    std::process::Command::new("open")
        .arg(destination)
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("無法開啟資料夾：{error}"))
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            app.set_activation_policy(ActivationPolicy::Accessory);

            let config = load_config(app.handle()).unwrap_or_default();
            save_config_to_disk(app.handle(), &config).map_err(setup_error)?;
            let status = JobStatus::from_config(&config);
            let state = AppState {
                config: Mutex::new(config),
                status: Mutex::new(status),
                scheduler: Mutex::new(None),
                tray_controls: Mutex::new(None),
            };
            app.manage(state);

            setup_tray(app.handle()).map_err(setup_error)?;
            let app_handle = app.handle().clone();
            let managed_state = app.state::<AppState>();
            sync_launch_agent(managed_state.inner()).map_err(setup_error)?;
            restart_scheduler(app_handle.clone(), managed_state.inner()).map_err(setup_error)?;
            maybe_run_startup_download(app_handle, managed_state.inner());
            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .invoke_handler(tauri::generate_handler![
            get_config,
            get_status,
            get_destination_status,
            set_download_source,
            run_now,
            reveal_destination
        ])
        .run(tauri::generate_context!())
        .expect("error while running MacTTC");
}

pub(crate) fn sync_launch_agent(state: &AppState) -> Result<(), String> {
    let enabled = state.config.lock().map_err(lock_error)?.autostart_enabled;
    let path = launch_agent_path()?;

    if enabled {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| format!("無法建立 LaunchAgents 資料夾：{error}"))?;
        }
        let executable =
            env::current_exe().map_err(|error| format!("無法取得程式路徑：{error}"))?;
        let app_bundle = app_bundle_path(&executable).unwrap_or(executable);
        fs::write(path, launch_agent_plist(&app_bundle))
            .map_err(|error| format!("無法建立開機啟動設定：{error}"))?;
    } else if path.exists() {
        fs::remove_file(path).map_err(|error| format!("無法移除開機啟動設定：{error}"))?;
    }

    Ok(())
}

fn launch_agent_path() -> Result<PathBuf, String> {
    let mut path = dirs::home_dir().ok_or_else(|| "無法取得使用者 home 目錄".to_string())?;
    path.push("Library");
    path.push("LaunchAgents");
    path.push(format!("{LAUNCH_AGENT_LABEL}.plist"));
    Ok(path)
}

fn launch_agent_plist(app_bundle: &Path) -> String {
    let app_bundle = escape_plist_text(&app_bundle.to_string_lossy());
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key>
  <string>{LAUNCH_AGENT_LABEL}</string>
  <key>ProgramArguments</key>
  <array>
    <string>/usr/bin/open</string>
    <string>{app_bundle}</string>
  </array>
  <key>RunAtLoad</key>
  <true/>
</dict>
</plist>
"#
    )
}

fn app_bundle_path(executable: &Path) -> Option<PathBuf> {
    executable
        .ancestors()
        .find(|path| path.extension().is_some_and(|extension| extension == "app"))
        .map(Path::to_path_buf)
}

fn escape_plist_text(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

pub(crate) fn now_string() -> String {
    let now: DateTime<Local> = Local::now();
    now.format("%Y-%m-%d %H:%M:%S").to_string()
}

pub(crate) fn lock_error<T>(error: std::sync::PoisonError<T>) -> String {
    format!("內部狀態鎖定失敗：{error}")
}

pub(crate) fn to_string_error(error: impl std::fmt::Display) -> String {
    error.to_string()
}

fn setup_error(error: String) -> Box<dyn Error> {
    Box::new(io::Error::other(error))
}

#[cfg(test)]
mod tests {
    use super::{
        default_schedule_interval_hours, localization::apple_languages_primary_is_chinese,
        scheduler::last_success_is_within_one_hour,
    };
    use chrono::{Duration as ChronoDuration, Local};

    #[test]
    fn apple_languages_uses_only_primary_language_for_chinese_detection() {
        assert!(apple_languages_primary_is_chinese(
            r#"(
    "zh-Hant-TW",
    "en-US"
)"#
        ));

        assert!(!apple_languages_primary_is_chinese(
            r#"(
    "en-US",
    "zh-Hant-TW"
)"#
        ));

        assert!(!apple_languages_primary_is_chinese(
            r#"(
    "ja-JP",
    "zh-Hant-TW"
)"#
        ));
    }

    #[test]
    fn startup_download_skips_when_last_success_is_recent() {
        let recent = (Local::now() - ChronoDuration::minutes(30))
            .format("%Y-%m-%d %H:%M:%S")
            .to_string();
        let old = (Local::now() - ChronoDuration::minutes(61))
            .format("%Y-%m-%d %H:%M:%S")
            .to_string();

        assert!(last_success_is_within_one_hour(&recent));
        assert!(!last_success_is_within_one_hour(&old));
        assert!(!last_success_is_within_one_hour("not a timestamp"));
    }

    #[test]
    fn schedule_interval_is_fixed_to_three_hours() {
        assert_eq!(default_schedule_interval_hours(), 3);
    }
}
