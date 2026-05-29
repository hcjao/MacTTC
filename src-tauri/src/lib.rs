use chrono::{DateTime, Local, NaiveDateTime};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::{
    collections::{HashMap, HashSet},
    env,
    error::Error,
    fs::{self, File},
    io::{self, Write},
    path::{Path, PathBuf},
    sync::Mutex,
    time::Duration,
};
use tauri::{
    async_runtime::JoinHandle,
    image::Image,
    menu::{CheckMenuItem, MenuBuilder, MenuItem},
    tray::TrayIconBuilder,
    ActivationPolicy, AppHandle, Emitter, Manager, State, WindowEvent, Wry,
};
use tokio::{fs as async_fs, io::AsyncWriteExt};
use zip::ZipArchive;

const CONFIG_FILE: &str = "config.json";
const UPLOAD_LOG_FILE: &str = "upload.log";
const LAUNCH_AGENT_LABEL: &str = "com.macttc.downloader";
const TRAY_SHOW_ID: &str = "show";
const TRAY_QUIT_ID: &str = "quit";
const TRAY_SCHEDULE_LABEL_ID: &str = "schedule_label";
const TRAY_SCHEDULE_OFF_ID: &str = "schedule_off";
const TRAY_INTERVAL_3_ID: &str = "interval_3";
const TRAY_AUTOSTART_ID: &str = "autostart";
const JOB_STATUS_CHANGED_EVENT: &str = "job-status-changed";
const APP_UPLOAD_FILE_TIME_ENV: &str = "MACTTC_UPLOAD_FILE_TIME";
const TTC_WEB_CLIENT_VERSION: &str = "3.1.0.0";
const TTC_POST_BATCH_SIZE: usize = 100;
const TTC_MIN_SUPPORTED_SAVED_VAR_VERSION: i64 = 7;
const NA_PRICE_TABLE_URL: &str = "https://us.tamrieltradecentre.com/download/PriceTable";
const EU_PRICE_TABLE_URL: &str = "https://eu.tamrieltradecentre.com/download/PriceTable";
const TTC_ADDON_RELATIVE_PATH: &[&str] = &[
    "Documents",
    "Elder Scrolls Online",
    "live",
    "AddOns",
    "TamrielTradeCentre",
];
const TTC_SAVED_VARIABLES_RELATIVE_PATH: &[&str] = &[
    "Documents",
    "Elder Scrolls Online",
    "live",
    "SavedVariables",
    "TamrielTradeCentre.lua",
];

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AppConfig {
    url: String,
    destination_dir: String,
    #[serde(default)]
    schedule_enabled: bool,
    #[serde(default = "default_schedule_interval_hours")]
    schedule_interval_hours: u64,
    #[serde(default)]
    autostart_enabled: bool,
    #[serde(default)]
    auto_upload_enabled: bool,
    #[serde(default)]
    upload_file_time: Option<String>,
    #[serde(default = "default_upload_client_id")]
    upload_client_id: String,
    #[serde(default)]
    upload_latest_auto_record_discover_unix_time: i64,
    #[serde(default)]
    upload_log_enabled: bool,
    #[serde(default)]
    last_success_at: Option<String>,
    #[serde(default)]
    last_success_source_url: Option<String>,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            url: NA_PRICE_TABLE_URL.to_string(),
            destination_dir: fixed_destination_dir()
                .unwrap_or_else(|_| {
                    PathBuf::from("~/Documents/Elder Scrolls Online/live/AddOns/TamrielTradeCentre")
                })
                .to_string_lossy()
                .to_string(),
            schedule_enabled: false,
            schedule_interval_hours: default_schedule_interval_hours(),
            autostart_enabled: false,
            auto_upload_enabled: false,
            upload_file_time: None,
            upload_client_id: default_upload_client_id(),
            upload_latest_auto_record_discover_unix_time: 0,
            upload_log_enabled: false,
            last_success_at: None,
            last_success_source_url: None,
        }
    }
}

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

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct DestinationStatus {
    path: String,
    exists: bool,
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
    uploader: Mutex<Option<JoinHandle<()>>>,
    upload_running: Mutex<bool>,
    tray_controls: Mutex<Option<TrayControls>>,
}

struct TrayControls {
    schedule_off: CheckMenuItem<Wry>,
    interval_3: CheckMenuItem<Wry>,
    autostart: CheckMenuItem<Wry>,
}

struct TrayLabels {
    show: &'static str,
    schedule_title: &'static str,
    schedule_off: &'static str,
    interval_3: &'static str,
    autostart: &'static str,
    quit: &'static str,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ServerRegion {
    Na,
    Eu,
}

struct TtcUploadClient {
    http: reqwest::Client,
    base_url: &'static str,
    client_id: String,
    log_path: Option<PathBuf>,
}

#[derive(Clone, Debug)]
struct UploadTradeEntry {
    model: Value,
    item_uid: i64,
    item_id: Option<i64>,
    item_link: Option<String>,
    discover_unix_time: i64,
    expire_unix_time: i64,
}

#[derive(Default)]
struct GuildInfoStore {
    records: HashMap<i64, (Option<i64>, i64)>,
    updated_guilds: HashSet<i64>,
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
        config.auto_upload_enabled = stored.auto_upload_enabled;
        config.upload_file_time = stored.upload_file_time.clone();
        config.upload_client_id = stored.upload_client_id.clone();
        config.upload_latest_auto_record_discover_unix_time =
            stored.upload_latest_auto_record_discover_unix_time;
        config.upload_log_enabled = stored.upload_log_enabled;
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
                uploader: Mutex::new(None),
                upload_running: Mutex::new(false),
                tray_controls: Mutex::new(None),
            };
            app.manage(state);

            setup_tray(app.handle()).map_err(setup_error)?;
            let app_handle = app.handle().clone();
            let managed_state = app.state::<AppState>();
            sync_launch_agent(managed_state.inner()).map_err(setup_error)?;
            restart_scheduler(app_handle.clone(), managed_state.inner()).map_err(setup_error)?;
            restart_uploader(app_handle.clone(), managed_state.inner()).map_err(setup_error)?;
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

fn setup_tray(app: &AppHandle) -> Result<(), String> {
    let config = app
        .state::<AppState>()
        .config
        .lock()
        .map_err(lock_error)?
        .clone();
    let labels = tray_labels();
    let schedule_label = MenuItem::with_id(
        app,
        TRAY_SCHEDULE_LABEL_ID,
        labels.schedule_title,
        false,
        None::<&str>,
    )
    .map_err(to_string_error)?;
    let schedule_off = CheckMenuItem::with_id(
        app,
        TRAY_SCHEDULE_OFF_ID,
        labels.schedule_off,
        true,
        !config.schedule_enabled,
        None::<&str>,
    )
    .map_err(to_string_error)?;
    let interval_3 = CheckMenuItem::with_id(
        app,
        TRAY_INTERVAL_3_ID,
        labels.interval_3,
        true,
        config.schedule_enabled && config.schedule_interval_hours == 3,
        None::<&str>,
    )
    .map_err(to_string_error)?;
    let autostart = CheckMenuItem::with_id(
        app,
        TRAY_AUTOSTART_ID,
        labels.autostart,
        true,
        config.autostart_enabled,
        None::<&str>,
    )
    .map_err(to_string_error)?;
    let menu = MenuBuilder::new(app)
        .text(TRAY_SHOW_ID, labels.show)
        .separator()
        .item(&schedule_label)
        .item(&schedule_off)
        .item(&interval_3)
        .separator()
        .item(&autostart)
        .separator()
        .text(TRAY_QUIT_ID, labels.quit)
        .build()
        .map_err(to_string_error)?;

    *app.state::<AppState>()
        .tray_controls
        .lock()
        .map_err(lock_error)? = Some(TrayControls {
        schedule_off,
        interval_3,
        autostart,
    });

    let tray_icon = Image::new(include_bytes!("../icons/tray-coin.rgba"), 64, 64);

    let tray = TrayIconBuilder::with_id("main")
        .menu(&menu)
        .icon(tray_icon)
        .icon_as_template(true)
        .tooltip("MacTTC")
        .show_menu_on_left_click(true)
        .on_menu_event(|app, event| match event.id().as_ref() {
            TRAY_SHOW_ID => show_main_window(app),
            TRAY_SCHEDULE_OFF_ID => {
                let _ = disable_schedule(app.clone());
            }
            TRAY_INTERVAL_3_ID => {
                let _ = enable_schedule(app.clone());
            }
            TRAY_AUTOSTART_ID => {
                let _ = toggle_autostart(app.clone());
            }
            TRAY_QUIT_ID => app.exit(0),
            _ => {}
        });

    tray.build(app).map_err(to_string_error)?;
    Ok(())
}

fn disable_schedule(app: AppHandle) -> Result<(), String> {
    let state = app.state::<AppState>();
    {
        let mut config = state.config.lock().map_err(lock_error)?;
        config.schedule_enabled = false;
        config.destination_dir = fixed_destination_string()?;
        save_config_to_disk(&app, &config)?;
    }
    update_tray_checks(state.inner())?;
    restart_scheduler(app.clone(), state.inner())
}

fn enable_schedule(app: AppHandle) -> Result<(), String> {
    let state = app.state::<AppState>();
    {
        let mut config = state.config.lock().map_err(lock_error)?;
        config.schedule_enabled = true;
        config.schedule_interval_hours = default_schedule_interval_hours();
        config.destination_dir = fixed_destination_string()?;
        save_config_to_disk(&app, &config)?;
    }
    update_tray_checks(state.inner())?;
    restart_scheduler(app.clone(), state.inner())
}

fn toggle_autostart(app: AppHandle) -> Result<(), String> {
    let state = app.state::<AppState>();
    {
        let mut config = state.config.lock().map_err(lock_error)?;
        config.autostart_enabled = !config.autostart_enabled;
        config.destination_dir = fixed_destination_string()?;
        save_config_to_disk(&app, &config)?;
    }
    sync_launch_agent(state.inner())?;
    update_tray_checks(state.inner())
}

fn update_tray_checks(state: &AppState) -> Result<(), String> {
    let config = state.config.lock().map_err(lock_error)?.clone();
    if let Some(controls) = state.tray_controls.lock().map_err(lock_error)?.as_ref() {
        controls
            .schedule_off
            .set_checked(!config.schedule_enabled)
            .map_err(to_string_error)?;
        controls
            .interval_3
            .set_checked(config.schedule_enabled)
            .map_err(to_string_error)?;
        controls
            .autostart
            .set_checked(config.autostart_enabled)
            .map_err(to_string_error)?;
    }
    Ok(())
}

fn restart_scheduler(app: AppHandle, state: &AppState) -> Result<(), String> {
    if let Some(handle) = state.scheduler.lock().map_err(lock_error)?.take() {
        handle.abort();
    }

    let config = state.config.lock().map_err(lock_error)?.clone();
    if !config.schedule_enabled {
        return Ok(());
    }

    let interval_hours = default_schedule_interval_hours();
    let handle = tauri::async_runtime::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(interval_hours * 60 * 60));
        interval.tick().await;

        loop {
            interval.tick().await;
            let state = app.state::<AppState>();
            let latest_config = match state.config.lock() {
                Ok(config) => config.clone(),
                Err(_) => continue,
            };
            if latest_config.schedule_enabled {
                let _ = run_job(app.clone(), state.inner(), latest_config).await;
            }
        }
    });

    *state.scheduler.lock().map_err(lock_error)? = Some(handle);
    Ok(())
}

fn restart_uploader(app: AppHandle, state: &AppState) -> Result<(), String> {
    if let Some(handle) = state.uploader.lock().map_err(lock_error)?.take() {
        handle.abort();
    }

    let config = state.config.lock().map_err(lock_error)?.clone();
    if !config.auto_upload_enabled {
        return Ok(());
    }

    let handle = tauri::async_runtime::spawn(async move {
        maybe_run_auto_upload(app.clone()).await;

        let mut interval = tokio::time::interval(Duration::from_secs(60));
        loop {
            interval.tick().await;
            maybe_run_auto_upload(app.clone()).await;
        }
    });

    *state.uploader.lock().map_err(lock_error)? = Some(handle);
    Ok(())
}

fn maybe_run_startup_download(app: AppHandle, state: &AppState) {
    let config = match state.config.lock() {
        Ok(config) => config.clone(),
        _ => return,
    };

    if config
        .last_success_at
        .as_deref()
        .is_some_and(last_success_is_within_one_hour)
    {
        return;
    }

    let startup_url = config
        .last_success_source_url
        .clone()
        .filter(|url| is_allowed_download_url(url))
        .unwrap_or_else(|| config.url.clone());

    if !is_allowed_download_url(&startup_url) {
        return;
    }

    tauri::async_runtime::spawn(async move {
        let mut startup_config = config;
        startup_config.url = startup_url;
        match fixed_destination_string() {
            Ok(destination) => startup_config.destination_dir = destination,
            Err(_) => return,
        }

        let state = app.state::<AppState>();
        let _ = run_job(app.clone(), state.inner(), startup_config).await;
    });
}

fn show_main_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.set_focus();
    }
}

fn tray_labels() -> TrayLabels {
    if system_language_is_chinese() {
        TrayLabels {
            show: "顯示 MacTTC",
            schedule_title: "下載排程",
            schedule_off: "關閉排程",
            interval_3: "每 3 小時",
            autostart: "開機啟動",
            quit: "退出",
        }
    } else {
        TrayLabels {
            show: "Show MacTTC",
            schedule_title: "Download Schedule",
            schedule_off: "Turn Schedule Off",
            interval_3: "Every 3 hours",
            autostart: "Launch at Login",
            quit: "Quit",
        }
    }
}

fn system_language_is_chinese() -> bool {
    if let Ok(output) = std::process::Command::new("defaults")
        .args(["read", "-g", "AppleLanguages"])
        .output()
    {
        return apple_languages_primary_is_chinese(&String::from_utf8_lossy(&output.stdout));
    }

    ["LANG", "LC_ALL", "LC_MESSAGES"]
        .iter()
        .filter_map(|key| env::var(key).ok())
        .any(|value| value.to_lowercase().starts_with("zh"))
}

fn apple_languages_primary_is_chinese(languages: &str) -> bool {
    let languages = languages.to_lowercase();

    if let Some(start) = languages.find('"') {
        if let Some(end) = languages[start + 1..].find('"') {
            return languages[start + 1..start + 1 + end].starts_with("zh");
        }
    }

    languages
        .lines()
        .map(|line| {
            line.trim()
                .trim_end_matches(',')
                .trim_matches('"')
                .trim()
                .to_string()
        })
        .find(|line| !line.is_empty() && line != "(" && line != ")")
        .is_some_and(|language| language.starts_with("zh"))
}

async fn run_job(app: AppHandle, state: &AppState, config: AppConfig) -> Result<(), String> {
    {
        let mut status = state.status.lock().map_err(lock_error)?;
        if status.running {
            return Err("已有下載工作正在執行".to_string());
        }
        status.running = true;
        status.last_started_at = Some(now_string());
        status.last_error = None;
        status.source_url = Some(config.url.clone());
        status.destination_dir = Some(config.destination_dir.clone());
        status.message = "正在下載壓縮檔".to_string();
    }
    emit_status(&app, state);

    let result = async {
        ensure_destination_exists()?;
        let archive_path = download_archive(&app, &config.url).await?;
        set_status(state, |status| {
            status.message = "下載完成，正在解壓縮".to_string();
        })?;
        emit_status(&app, state);

        extract_zip(&archive_path, Path::new(&config.destination_dir)).await?;
        let _ = async_fs::remove_file(&archive_path).await;
        Ok::<(), String>(())
    }
    .await;

    let mut status = state.status.lock().map_err(lock_error)?;
    status.running = false;
    let finished_at = now_string();
    status.last_finished_at = Some(finished_at.clone());

    match result {
        Ok(()) => {
            status.last_success_at = Some(finished_at.clone());
            status.last_error = None;
            status.source_url = Some(config.url.clone());
            status.destination_dir = Some(config.destination_dir.clone());
            status.message = "完成下載與解壓縮".to_string();
            drop(status);
            emit_status(&app, state);
            record_success(app, state, finished_at, config.url, config.destination_dir)?;
            Ok(())
        }
        Err(error) => {
            status.last_error = Some(error.clone());
            status.message = "執行失敗".to_string();
            drop(status);
            emit_status(&app, state);
            Err(error)
        }
    }
}

async fn download_archive(app: &AppHandle, url: &str) -> Result<PathBuf, String> {
    let response = reqwest::get(url)
        .await
        .map_err(|error| format!("下載失敗：{error}"))?
        .error_for_status()
        .map_err(|error| format!("伺服器回應錯誤：{error}"))?;

    let temp_dir = app
        .path()
        .app_cache_dir()
        .map_err(|error| format!("無法取得快取資料夾：{error}"))?;
    async_fs::create_dir_all(&temp_dir)
        .await
        .map_err(|error| format!("無法建立快取資料夾：{error}"))?;

    let archive_path = temp_dir.join("latest-download.zip");
    let mut file = async_fs::File::create(&archive_path)
        .await
        .map_err(|error| format!("無法建立暫存檔：{error}"))?;
    let bytes = response
        .bytes()
        .await
        .map_err(|error| format!("讀取下載內容失敗：{error}"))?;
    file.write_all(&bytes)
        .await
        .map_err(|error| format!("寫入暫存檔失敗：{error}"))?;

    Ok(archive_path)
}

async fn extract_zip(archive_path: &Path, destination: &Path) -> Result<(), String> {
    let archive_path = archive_path.to_path_buf();
    let destination = destination.to_path_buf();

    tokio::task::spawn_blocking(move || {
        fs::create_dir_all(&destination).map_err(|error| format!("無法建立目的資料夾：{error}"))?;

        let file = File::open(&archive_path).map_err(|error| format!("無法開啟壓縮檔：{error}"))?;
        let mut archive =
            ZipArchive::new(file).map_err(|error| format!("ZIP 格式錯誤：{error}"))?;

        for index in 0..archive.len() {
            let mut entry = archive
                .by_index(index)
                .map_err(|error| format!("讀取 ZIP 內容失敗：{error}"))?;
            let Some(safe_path) = entry.enclosed_name().map(PathBuf::from) else {
                continue;
            };
            let output_path = destination.join(safe_path);

            if entry.is_dir() {
                fs::create_dir_all(&output_path)
                    .map_err(|error| format!("無法建立資料夾：{error}"))?;
                continue;
            }

            if let Some(parent) = output_path.parent() {
                fs::create_dir_all(parent).map_err(|error| format!("無法建立資料夾：{error}"))?;
            }

            let mut output =
                File::create(&output_path).map_err(|error| format!("無法寫入檔案：{error}"))?;
            io::copy(&mut entry, &mut output)
                .map_err(|error| format!("解壓縮檔案失敗：{error}"))?;
        }

        Ok(())
    })
    .await
    .map_err(|error| format!("解壓縮工作中斷：{error}"))?
}

async fn maybe_run_auto_upload(app: AppHandle) {
    let state = app.state::<AppState>();
    let config = match state.config.lock() {
        Ok(config) if config.auto_upload_enabled => config.clone(),
        _ => return,
    };

    let saved_variables_path = match saved_variables_file_path() {
        Ok(path) => path,
        Err(_) => return,
    };
    if !saved_variables_path.exists() {
        return;
    }

    let modified_time = saved_variables_modified_time(&saved_variables_path)
        .ok()
        .flatten();
    if !should_upload_saved_variables(modified_time, config.upload_file_time.as_deref()) {
        return;
    }

    {
        let mut running = match state.upload_running.lock() {
            Ok(running) => running,
            Err(_) => return,
        };
        if *running {
            return;
        }
        *running = true;
    }

    let upload_result =
        upload_saved_variables_file(&app, state.inner(), config, &saved_variables_path).await;
    if let Err(error) = upload_result {
        set_status(state.inner(), |status| {
            status.last_error = Some(error);
            status.message = "自動上傳失敗".to_string();
        })
        .ok();
        emit_status(&app, state.inner());
    }

    {
        let running_result = state.upload_running.lock();
        if let Ok(mut running) = running_result {
            *running = false;
        }
    }
}

async fn upload_saved_variables_file(
    app: &AppHandle,
    state: &AppState,
    config: AppConfig,
    saved_variables_path: &Path,
) -> Result<(), String> {
    let file_modified_time = saved_variables_modified_time(saved_variables_path)?;
    let content = async_fs::read_to_string(saved_variables_path)
        .await
        .map_err(|error| format!("讀取 TamrielTradeCentre.lua 失敗：{error}"))?;

    let upload_result = parse_and_upload_saved_variables(&config, &content).await?;
    let uploaded_time = file_modified_time
        .map(format_system_time)
        .unwrap_or_else(now_string);

    let mut stored = state.config.lock().map_err(lock_error)?;
    stored.upload_file_time = Some(uploaded_time);
    stored.upload_latest_auto_record_discover_unix_time =
        upload_result.latest_auto_record_discover_unix_time;
    save_config_to_disk(app, &stored)
}

struct UploadResult {
    latest_auto_record_discover_unix_time: i64,
}

async fn parse_and_upload_saved_variables(
    config: &AppConfig,
    content: &str,
) -> Result<UploadResult, String> {
    let Some(start) = content.find('{') else {
        return Ok(UploadResult {
            latest_auto_record_discover_unix_time: config
                .upload_latest_auto_record_discover_unix_time,
        });
    };

    let saved_variables = LuaParser::new(&content[start..])
        .parse()
        .map_err(|error| format!("解析 TamrielTradeCentre.lua 失敗：{error}"))?;
    let region = upload_region_from_url(&config.url);
    let log_path = if config.upload_log_enabled {
        Some(upload_log_path()?)
    } else {
        None
    };
    let client = TtcUploadClient::new(region, config.upload_client_id.clone(), log_path)?;
    let mut guild_store = GuildInfoStore::default();
    let mut guild_ids = HashMap::new();
    let mut raw_entries = Vec::new();
    let mut upload_unregistered_links = true;

    let default_node = object_get(&saved_variables, "Default")?;
    for (account_name, account_node) in default_node {
        let Some(account_model) = account_node.get("$AccountWide") else {
            continue;
        };
        let actual_version = value_i64(account_model.get("ActualVersion")).unwrap_or(0);
        if actual_version < TTC_MIN_SUPPORTED_SAVED_VAR_VERSION {
            continue;
        }
        let Some(settings) = account_model.get("Settings") else {
            continue;
        };
        let data_node_name = match region {
            ServerRegion::Na => "NAData",
            ServerRegion::Eu => "EUData",
        };
        let Some(data_node) = account_model.get(data_node_name) else {
            continue;
        };

        if let Some(culture) = value_str(account_model.get("ClientCulture")) {
            upload_unregistered_links = culture.eq_ignore_ascii_case("en");
        }

        if value_bool(settings.get("EnableAutoRecordStoreEntries")).unwrap_or(false) {
            if let Some(auto_guilds) = data_node
                .get("AutoRecordEntries")
                .and_then(|node| node.get("Guilds"))
                .and_then(Value::as_object)
            {
                for (guild_name, guild_node) in auto_guilds {
                    let guild_id =
                        get_or_fetch_guild_id(&client, &mut guild_ids, guild_name).await?;
                    let kiosk_location_id = value_i64(guild_node.get("KioskLocationID"));
                    let last_update = value_i64(guild_node.get("LastUpdate")).unwrap_or(0);
                    guild_store.record_kiosk_location_id(guild_id, kiosk_location_id, last_update);
                    let Some(kiosk_location_id) = kiosk_location_id else {
                        continue;
                    };
                    raw_entries.extend(parse_auto_trade_entries(
                        guild_node,
                        guild_id,
                        kiosk_location_id,
                    ));
                }
            }
        }

        if value_bool(settings.get("EnableSelfEntriesUpload")).unwrap_or(false) {
            if let Some(guilds) = data_node.get("Guilds").and_then(Value::as_object) {
                for (guild_name, guild_node) in guilds {
                    let guild_id =
                        get_or_fetch_guild_id(&client, &mut guild_ids, guild_name).await?;
                    raw_entries.extend(parse_self_trade_entries(
                        guild_node,
                        account_name,
                        guild_id,
                    ));
                }
            }
        }
    }

    let now = Local::now().timestamp();
    let mut filtered_entries = Vec::<UploadTradeEntry>::new();
    let mut entries_without_item_id = Vec::<UploadTradeEntry>::new();
    let mut item_uid_indexes = HashMap::<i64, usize>::new();

    for entry in raw_entries {
        if entry.discover_unix_time <= config.upload_latest_auto_record_discover_unix_time
            || now > entry.expire_unix_time
            || entry.discover_unix_time > now
            || now - entry.discover_unix_time > 6 * 60 * 60
            || entry.item_uid == 0
        {
            continue;
        }

        if entry.item_id.is_none() {
            entries_without_item_id.push(entry);
            continue;
        }

        if let Some(index) = item_uid_indexes.get(&entry.item_uid).copied() {
            if entry.discover_unix_time > filtered_entries[index].discover_unix_time {
                filtered_entries[index] = entry;
            }
        } else {
            item_uid_indexes.insert(entry.item_uid, filtered_entries.len());
            filtered_entries.push(entry);
        }
    }

    let latest_auto_record_discover_unix_time = filtered_entries
        .iter()
        .map(|entry| entry.discover_unix_time)
        .max()
        .unwrap_or(config.upload_latest_auto_record_discover_unix_time)
        .max(config.upload_latest_auto_record_discover_unix_time);

    for batch in filtered_entries.chunks(TTC_POST_BATCH_SIZE) {
        let models = batch
            .iter()
            .map(|entry| entry.model.clone())
            .collect::<Vec<_>>();
        client.post_auto_recorded_entries(&models).await?;
    }

    if !filtered_entries.is_empty()
        && entries_without_item_id.len() < filtered_entries.len() / 5
        && upload_unregistered_links
    {
        let mut item_links = entries_without_item_id
            .iter()
            .filter_map(|entry| entry.item_link.clone())
            .collect::<Vec<_>>();
        item_links.sort();
        item_links.dedup();
        for batch in item_links.chunks(TTC_POST_BATCH_SIZE) {
            client.record_item_links(batch).await?;
        }
    }

    for (guild_id, (kiosk_location_id, timestamp)) in guild_store.updated_guilds() {
        client
            .verify_kiosk_location(guild_id, kiosk_location_id, timestamp)
            .await?;
    }

    Ok(UploadResult {
        latest_auto_record_discover_unix_time,
    })
}

impl TtcUploadClient {
    fn new(
        region: ServerRegion,
        client_id: String,
        log_path: Option<PathBuf>,
    ) -> Result<Self, String> {
        let base_url = match region {
            ServerRegion::Na => "https://us.tamrieltradecentre.com",
            ServerRegion::Eu => "https://eu.tamrieltradecentre.com",
        };
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(|error| format!("建立上傳 HTTP client 失敗：{error}"))?;
        Ok(Self {
            http,
            base_url,
            client_id,
            log_path,
        })
    }

    async fn get_guild_id(&self, guild_name: &str) -> Result<i64, String> {
        let url = format!("{}/api/PC/Trade/GetGuildID", self.base_url);
        let response = self
            .http
            .get(url)
            .query(&[("guildName", guild_name)])
            .header("WebClientVersion", TTC_WEB_CLIENT_VERSION)
            .header("ClientID", &self.client_id)
            .send()
            .await
            .map_err(|error| format!("取得 guild ID 失敗：{error}"))?
            .error_for_status()
            .map_err(|error| format!("取得 guild ID 回應錯誤：{error}"))?;
        let response_text = response
            .text()
            .await
            .map_err(|error| format!("讀取 guild ID 回應失敗：{error}"))?;
        let value: Value = serde_json::from_str(&response_text)
            .map_err(|error| format!("解析 guild ID 回應失敗：{error}"))?;
        value_i64(value.get("GuildID")).ok_or_else(|| "guild ID 回應缺少 GuildID".to_string())
    }

    async fn post_json(&self, path: &str, payload: &Value) -> Result<(), String> {
        let url = format!("{}{}", self.base_url, path);
        let payload_text = payload.to_string();
        let payload_size = payload_text.len();
        let response = self
            .http
            .post(url)
            .header("WebClientVersion", TTC_WEB_CLIENT_VERSION)
            .header("ClientID", &self.client_id)
            .header("Content-Type", "application/json; charset=UTF-8")
            .body(payload_text.clone())
            .send()
            .await
            .map_err(|error| {
                self.write_upload_log(&format!(
                    "request_url={}\npayload_size={}\nresponse_status=REQUEST_ERROR\npayload={}\nerror={}",
                    format!("{}{}", self.base_url, path),
                    payload_size,
                    payload_text,
                    error
                ));
                format!("上傳請求失敗：{error}")
            })?;
        let status = response.status();
        self.write_upload_log(&format!(
            "request_url={}\npayload_size={}\nresponse_status={}\npayload={}",
            format!("{}{}", self.base_url, path),
            payload_size,
            status.as_u16(),
            payload_text
        ));
        response
            .error_for_status()
            .map(|_| ())
            .map_err(|error| format!("上傳回應錯誤：{error}"))
    }

    fn write_upload_log(&self, content: &str) {
        let Some(path) = self.log_path.as_ref() else {
            return;
        };
        if let Some(parent) = path.parent() {
            if fs::create_dir_all(parent).is_err() {
                return;
            }
        }
        let Ok(mut file) = fs::OpenOptions::new().create(true).append(true).open(path) else {
            return;
        };
        let _ = writeln!(file, "----- {} -----\n{}\n", now_string(), content);
    }

    async fn post_auto_recorded_entries(&self, entries: &[Value]) -> Result<(), String> {
        if entries.is_empty() {
            return Ok(());
        }
        self.post_json(
            "/api/PC/Trade/PostAutoRecordedEntry",
            &Value::Array(entries.to_vec()),
        )
        .await
    }

    async fn record_item_links(&self, item_links: &[String]) -> Result<(), String> {
        if item_links.is_empty() {
            return Ok(());
        }
        self.post_json("/api/PC/Trade/RecordItemLinks", &json!(item_links))
            .await
    }

    async fn verify_kiosk_location(
        &self,
        guild_id: i64,
        guild_kiosk_location_id: Option<i64>,
        timestamp: i64,
    ) -> Result<(), String> {
        self.post_json(
            "/api/PC/Trade/VerifyKioskLocation",
            &json!({
                "GuildID": guild_id,
                "GuildKioskLocationID": guild_kiosk_location_id,
                "Timestamp": timestamp,
            }),
        )
        .await
    }
}

impl GuildInfoStore {
    fn record_kiosk_location_id(
        &mut self,
        guild_id: i64,
        kiosk_location_id: Option<i64>,
        timestamp: i64,
    ) {
        if let Some((current_kiosk_location_id, current_timestamp)) = self.records.get(&guild_id) {
            if timestamp <= *current_timestamp {
                return;
            }
            if *current_kiosk_location_id != kiosk_location_id {
                self.updated_guilds.insert(guild_id);
            }
        } else {
            self.updated_guilds.insert(guild_id);
        }
        self.records
            .insert(guild_id, (kiosk_location_id, timestamp));
    }

    fn updated_guilds(&self) -> Vec<(i64, (Option<i64>, i64))> {
        self.updated_guilds
            .iter()
            .filter_map(|guild_id| {
                self.records
                    .get(guild_id)
                    .map(|record| (*guild_id, *record))
            })
            .collect()
    }
}

async fn get_or_fetch_guild_id(
    client: &TtcUploadClient,
    guild_ids: &mut HashMap<String, i64>,
    guild_name: &str,
) -> Result<i64, String> {
    if let Some(guild_id) = guild_ids.get(guild_name) {
        return Ok(*guild_id);
    }
    let guild_id = client.get_guild_id(guild_name).await?;
    guild_ids.insert(guild_name.to_string(), guild_id);
    Ok(guild_id)
}

fn parse_auto_trade_entries(
    guild_node: &Value,
    guild_id: i64,
    kiosk_location_id: i64,
) -> Vec<UploadTradeEntry> {
    let mut entries = Vec::new();
    let Some(player_listings) = guild_node.get("PlayerListings").and_then(Value::as_object) else {
        return entries;
    };

    for (player_id, auto_record_entries) in player_listings {
        let Some(auto_record_entries) = auto_record_entries.as_object() else {
            continue;
        };
        for entry_node in auto_record_entries.values() {
            if let (Some(discover_time), Some(expire_time)) = (
                value_i64(entry_node.get("DiscoverTime")),
                value_i64(entry_node.get("ExpireTime")),
            ) {
                if let Some(entry) = build_upload_trade_entry(
                    entry_node,
                    player_id,
                    guild_id,
                    Some(kiosk_location_id),
                    discover_time,
                    expire_time,
                ) {
                    entries.push(entry);
                }
            }
        }
    }
    entries
}

fn parse_self_trade_entries(
    guild_node: &Value,
    account_name: &str,
    guild_id: i64,
) -> Vec<UploadTradeEntry> {
    let mut entries = Vec::new();
    let discover_time = value_i64(guild_node.get("LastFullScan")).unwrap_or(0);
    let kiosk_location_id = value_i64(guild_node.get("KioskLocationID"));
    let Some(trade_assets) = guild_node.get("Entries").and_then(Value::as_object) else {
        return entries;
    };
    for trade_asset in trade_assets.values() {
        if let Some(entry) = build_upload_trade_entry(
            trade_asset,
            account_name,
            guild_id,
            kiosk_location_id,
            discover_time,
            discover_time + 60 * 60 * 24 * 7,
        ) {
            entries.push(entry);
        }
    }
    entries
}

fn build_upload_trade_entry(
    trade_asset_node: &Value,
    player_id: &str,
    guild_id: i64,
    guild_kiosk_location_id: Option<i64>,
    discover_unix_time: i64,
    expire_unix_time: i64,
) -> Option<UploadTradeEntry> {
    let (trade_asset, item_uid, item_id, item_link) = build_trade_asset_model(trade_asset_node)?;
    let model = json!({
        "TradeAsset": trade_asset,
        "PlayerID": player_id,
        "GuildID": guild_id,
        "GuildKioskLocationID": guild_kiosk_location_id,
        "DiscoverUnixTime": discover_unix_time,
        "ExpireUnixTime": expire_unix_time,
    });
    Some(UploadTradeEntry {
        model,
        item_uid,
        item_id,
        item_link,
        discover_unix_time,
        expire_unix_time,
    })
}

fn build_trade_asset_model(
    trade_asset_node: &Value,
) -> Option<(Value, i64, Option<i64>, Option<String>)> {
    let item_node = trade_asset_node.get("Item")?;
    let (item, item_uid, item_id, item_link) = build_item_model(item_node);
    let amount = value_i64(trade_asset_node.get("Amount")).unwrap_or(0);
    let total_price = value_i64(trade_asset_node.get("TotalPrice")).unwrap_or(0);
    Some((
        json!({
            "Amount": amount,
            "TotalPrice": total_price,
            "Item": item,
        }),
        item_uid,
        item_id,
        item_link,
    ))
}

fn build_item_model(item_node: &Value) -> (Value, i64, Option<i64>, Option<String>) {
    let item_id = value_i64(item_node.get("ID"));
    let item_uid = value_i64(item_node.get("UID")).unwrap_or(0);
    let item_link = value_str(item_node.get("ItemLink")).map(ToString::to_string);
    let mut item = Map::new();
    insert_optional_i64(&mut item, "ID", item_id);
    insert_optional_i64(&mut item, "UID", Some(item_uid));
    insert_optional_i64(
        &mut item,
        "QualityID",
        value_i64(item_node.get("QualityID")),
    );
    insert_optional_i64(
        &mut item,
        "Category2IDOverWrite",
        value_i64(item_node.get("Category2IDOverWrite")),
    );
    insert_optional_i64(&mut item, "TraitID", value_i64(item_node.get("TraitID")));
    insert_optional_i64(&mut item, "LevelTotal", value_i64(item_node.get("Level")));
    if let Some(potion_effects) = numeric_object_values(item_node.get("PotionEffects")) {
        item.insert("PotionEffectIDs".to_string(), Value::Array(potion_effects));
    }
    if let Some(master_writ_info) = item_node.get("MasterWritInfo") {
        item.insert(
            "MasterWritInfo".to_string(),
            normalize_lua_value(master_writ_info),
        );
    }
    (Value::Object(item), item_uid, item_id, item_link)
}

fn insert_optional_i64(map: &mut Map<String, Value>, key: &str, value: Option<i64>) {
    map.insert(key.to_string(), value.map_or(Value::Null, Value::from));
}

fn numeric_object_values(value: Option<&Value>) -> Option<Vec<Value>> {
    let object = value?.as_object()?;
    let mut values = object
        .iter()
        .filter_map(|(key, value)| key.parse::<i64>().ok().map(|index| (index, value.clone())))
        .collect::<Vec<_>>();
    values.sort_by_key(|(index, _)| *index);
    Some(
        values
            .into_iter()
            .map(|(_, value)| normalize_lua_value(&value))
            .collect(),
    )
}

fn normalize_lua_value(value: &Value) -> Value {
    match value {
        Value::Object(object) => {
            let mut normalized = Map::new();
            for (key, value) in object {
                normalized.insert(key.clone(), normalize_lua_value(value));
            }
            Value::Object(normalized)
        }
        Value::Array(values) => Value::Array(values.iter().map(normalize_lua_value).collect()),
        _ => value.clone(),
    }
}

fn object_get<'a>(value: &'a Value, key: &str) -> Result<&'a Map<String, Value>, String> {
    value
        .get(key)
        .and_then(Value::as_object)
        .ok_or_else(|| format!("TamrielTradeCentre.lua 缺少 {key} 節點"))
}

fn value_i64(value: Option<&Value>) -> Option<i64> {
    match value? {
        Value::Number(number) => number
            .as_i64()
            .or_else(|| number.as_f64().map(|v| v as i64)),
        _ => None,
    }
}

fn value_bool(value: Option<&Value>) -> Option<bool> {
    value?.as_bool()
}

fn value_str(value: Option<&Value>) -> Option<&str> {
    value?.as_str()
}

fn upload_region_from_url(url: &str) -> ServerRegion {
    if url == EU_PRICE_TABLE_URL {
        ServerRegion::Eu
    } else {
        ServerRegion::Na
    }
}

fn saved_variables_file_path() -> Result<PathBuf, String> {
    let mut path = dirs::home_dir().ok_or_else(|| "無法取得使用者 home 目錄".to_string())?;
    for segment in TTC_SAVED_VARIABLES_RELATIVE_PATH {
        path.push(segment);
    }
    Ok(path)
}

fn saved_variables_modified_time(path: &Path) -> Result<Option<std::time::SystemTime>, String> {
    fs::metadata(path)
        .map_err(|error| format!("無法讀取 SavedVariables 檔案資訊：{error}"))
        .map(|metadata| metadata.modified().ok())
}

fn should_upload_saved_variables(
    file_modified_time: Option<std::time::SystemTime>,
    upload_file_time: Option<&str>,
) -> bool {
    let Some(upload_file_time) = upload_file_time.and_then(parse_local_time_string) else {
        return true;
    };
    let Some(file_modified_time) = file_modified_time.map(DateTime::<Local>::from) else {
        return true;
    };
    file_modified_time > upload_file_time
}

struct LuaParser<'a> {
    chars: Vec<char>,
    cursor: usize,
    _source: &'a str,
}

impl<'a> LuaParser<'a> {
    fn new(source: &'a str) -> Self {
        Self {
            chars: source.chars().collect(),
            cursor: 0,
            _source: source,
        }
    }

    fn parse(mut self) -> Result<Value, String> {
        self.skip_spaces_and_comments();
        let value = self.read_value()?;
        self.skip_spaces_and_comments();
        if self.peek().is_some() {
            return Err("結尾包含無法解析的內容".to_string());
        }
        Ok(value)
    }

    fn read_value(&mut self) -> Result<Value, String> {
        self.skip_spaces_and_comments();
        match self.peek() {
            Some('{') => self.read_table(),
            Some('"') | Some('\'') => self.read_string().map(Value::String),
            Some('t') | Some('f') => self.read_bool().map(Value::Bool),
            Some('n') => {
                self.expect_literal("nil")?;
                Ok(Value::Null)
            }
            Some('N') => {
                self.expect_literal("NaN")?;
                Ok(Value::Null)
            }
            Some(_) => self.read_number().map(|number| json!(number)),
            None => Err("意外的檔案結尾".to_string()),
        }
    }

    fn read_table(&mut self) -> Result<Value, String> {
        self.expect_char('{')?;
        let mut object = Map::new();
        loop {
            self.skip_spaces_and_comments();
            if self.consume_char('}') {
                break;
            }
            self.expect_char('[')?;
            let key = self.read_key()?;
            self.skip_spaces_and_comments();
            self.expect_char(']')?;
            self.skip_spaces_and_comments();
            self.expect_char('=')?;
            let value = self.read_value()?;
            object.insert(key, value);
            self.skip_spaces_and_comments();
            self.consume_char(',');
        }
        Ok(Value::Object(object))
    }

    fn read_key(&mut self) -> Result<String, String> {
        self.skip_spaces_and_comments();
        match self.peek() {
            Some('"') | Some('\'') => self.read_string(),
            Some('t') | Some('f') => self.read_bool().map(|value| value.to_string()),
            Some(_) => self.read_number().map(|value| {
                if value.fract() == 0.0 {
                    (value as i64).to_string()
                } else {
                    value.to_string()
                }
            }),
            None => Err("讀取 table key 時遇到檔案結尾".to_string()),
        }
    }

    fn read_string(&mut self) -> Result<String, String> {
        let quote = self
            .next()
            .ok_or_else(|| "字串缺少開頭 quote".to_string())?;
        let mut result = String::new();
        let mut escaped = false;
        while let Some(ch) = self.next() {
            if escaped {
                match ch {
                    'n' => result.push('\n'),
                    'r' => result.push('\r'),
                    't' => result.push('\t'),
                    '\\' => result.push('\\'),
                    '"' => result.push('"'),
                    '\'' => result.push('\''),
                    digit if digit.is_ascii_digit() => {
                        let mut digits = digit.to_string();
                        for _ in 0..2 {
                            if self.peek().is_some_and(|next| next.is_ascii_digit()) {
                                digits.push(self.next().unwrap_or_default());
                            } else {
                                break;
                            }
                        }
                        if let Ok(code) = digits.parse::<u8>() {
                            result.push(code as char);
                        }
                    }
                    other => result.push(other),
                }
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == quote {
                return Ok(result);
            } else {
                result.push(ch);
            }
        }
        Err("字串缺少結尾 quote".to_string())
    }

    fn read_number(&mut self) -> Result<f64, String> {
        let mut number = String::new();
        while let Some(ch) = self.peek() {
            if ch.is_ascii_alphanumeric() || matches!(ch, '.' | '+' | '-' | 'x') {
                number.push(ch);
                self.next();
            } else {
                break;
            }
        }
        if let Some(hex) = number
            .strip_prefix("0x")
            .or_else(|| number.strip_prefix("-0x"))
        {
            let parsed =
                i64::from_str_radix(hex, 16).map_err(|_| format!("無法解析數字：{number}"))? as f64;
            return if number.starts_with('-') {
                Ok(-parsed)
            } else {
                Ok(parsed)
            };
        }
        number
            .parse::<f64>()
            .map_err(|_| format!("無法解析數字：{number}"))
    }

    fn read_bool(&mut self) -> Result<bool, String> {
        if self.starts_with("true") {
            self.cursor += 4;
            Ok(true)
        } else if self.starts_with("false") {
            self.cursor += 5;
            Ok(false)
        } else {
            Err("無法解析布林值".to_string())
        }
    }

    fn expect_literal(&mut self, literal: &str) -> Result<(), String> {
        if self.starts_with(literal) {
            self.cursor += literal.chars().count();
            Ok(())
        } else {
            Err(format!("缺少 literal：{literal}"))
        }
    }

    fn skip_spaces_and_comments(&mut self) {
        loop {
            while self.peek().is_some_and(|ch| ch.is_whitespace()) {
                self.next();
            }
            if self.peek() == Some('-') && self.peek_next() == Some('-') {
                self.next();
                self.next();
                if self.peek() == Some('[') && self.peek_next() == Some('[') {
                    self.next();
                    self.next();
                    while self.peek().is_some() {
                        if self.peek() == Some('-')
                            && self.peek_next() == Some('-')
                            && self.peek_offset(2) == Some(']')
                            && self.peek_offset(3) == Some(']')
                        {
                            for _ in 0..4 {
                                self.next();
                            }
                            break;
                        }
                        self.next();
                    }
                } else {
                    while self.peek().is_some_and(|ch| ch != '\n' && ch != '\r') {
                        self.next();
                    }
                }
                continue;
            }
            break;
        }
    }

    fn expect_char(&mut self, expected: char) -> Result<(), String> {
        if self.consume_char(expected) {
            Ok(())
        } else {
            Err(format!("缺少字元：{expected}"))
        }
    }

    fn consume_char(&mut self, expected: char) -> bool {
        if self.peek() == Some(expected) {
            self.next();
            true
        } else {
            false
        }
    }

    fn starts_with(&self, value: &str) -> bool {
        value
            .chars()
            .enumerate()
            .all(|(index, ch)| self.peek_offset(index) == Some(ch))
    }

    fn next(&mut self) -> Option<char> {
        let ch = self.peek()?;
        self.cursor += 1;
        Some(ch)
    }

    fn peek(&self) -> Option<char> {
        self.chars.get(self.cursor).copied()
    }

    fn peek_next(&self) -> Option<char> {
        self.peek_offset(1)
    }

    fn peek_offset(&self, offset: usize) -> Option<char> {
        self.chars.get(self.cursor + offset).copied()
    }
}

fn validate_config(config: &AppConfig) -> Result<(), String> {
    if !is_allowed_download_url(&config.url) {
        return Err("下載來源只能選擇 NA 或 EU".to_string());
    }

    if config.destination_dir != fixed_destination_string()? {
        return Err("目的資料夾只能使用固定的 TamrielTradeCentre AddOn 路徑".to_string());
    }

    if !destination_status()?.exists {
        return Err("找不到 TamrielTradeCentre AddOn 資料夾，無法下載".to_string());
    }

    if config.schedule_interval_hours != default_schedule_interval_hours() {
        return Err("下載排程間隔只能是 3 小時".to_string());
    }

    Ok(())
}

fn load_config(app: &AppHandle) -> Result<AppConfig, String> {
    let path = config_path(app)?;
    if !path.exists() {
        return Ok(AppConfig::default());
    }

    let content = fs::read_to_string(path).map_err(|error| format!("讀取設定失敗：{error}"))?;
    let mut config: AppConfig =
        serde_json::from_str(&content).map_err(|error| format!("設定檔格式錯誤：{error}"))?;

    if !is_allowed_download_url(&config.url) {
        config.url = NA_PRICE_TABLE_URL.to_string();
    }
    if !config
        .last_success_source_url
        .as_deref()
        .is_some_and(is_allowed_download_url)
    {
        config.last_success_at = None;
        config.last_success_source_url = None;
    }
    if config.upload_client_id.trim().is_empty() {
        config.upload_client_id = default_upload_client_id();
    }
    config.auto_upload_enabled = false;
    config.upload_log_enabled = false;
    sync_upload_file_time_env(config.upload_file_time.as_deref());
    config.destination_dir = fixed_destination_string()?;
    config.schedule_interval_hours = default_schedule_interval_hours();

    Ok(config)
}

fn record_success(
    app: AppHandle,
    state: &AppState,
    last_success_at: String,
    source_url: String,
    destination_dir: String,
) -> Result<(), String> {
    let mut config = state.config.lock().map_err(lock_error)?;
    config.url = source_url.clone();
    config.destination_dir = destination_dir;
    config.last_success_at = Some(last_success_at);
    config.last_success_source_url = Some(source_url);
    save_config_to_disk(&app, &config)
}

fn save_config_to_disk(app: &AppHandle, config: &AppConfig) -> Result<(), String> {
    sync_upload_file_time_env(config.upload_file_time.as_deref());
    let path = config_path(app)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| format!("無法建立設定資料夾：{error}"))?;
    }
    let content =
        serde_json::to_string_pretty(config).map_err(|error| format!("序列化設定失敗：{error}"))?;
    fs::write(path, content).map_err(|error| format!("儲存設定失敗：{error}"))
}

fn config_path(app: &AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_config_dir()
        .map(|dir| dir.join(CONFIG_FILE))
        .map_err(|error| format!("無法取得設定資料夾：{error}"))
}

fn upload_log_path() -> Result<PathBuf, String> {
    dirs::config_dir()
        .map(|dir| dir.join("com.macttc.downloader").join(UPLOAD_LOG_FILE))
        .ok_or_else(|| "無法取得設定資料夾".to_string())
}

fn set_status(state: &AppState, update: impl FnOnce(&mut JobStatus)) -> Result<(), String> {
    let mut status = state.status.lock().map_err(lock_error)?;
    update(&mut status);
    Ok(())
}

fn emit_status(app: &AppHandle, state: &AppState) {
    if let Ok(status) = state.status.lock().map(|status| status.clone()) {
        let _ = app.emit(JOB_STATUS_CHANGED_EVENT, status);
    }
}

fn is_allowed_download_url(url: &str) -> bool {
    matches!(url, NA_PRICE_TABLE_URL | EU_PRICE_TABLE_URL)
}

fn normalized_config(mut config: AppConfig) -> Result<AppConfig, String> {
    config.destination_dir = fixed_destination_string()?;
    config.schedule_interval_hours = default_schedule_interval_hours();
    validate_config(&config)?;
    Ok(config)
}

fn destination_status() -> Result<DestinationStatus, String> {
    let path = fixed_destination_dir()?;
    Ok(DestinationStatus {
        exists: path.is_dir(),
        path: path.to_string_lossy().to_string(),
    })
}

fn ensure_destination_exists() -> Result<PathBuf, String> {
    let path = fixed_destination_dir()?;
    if !path.is_dir() {
        return Err("找不到 TamrielTradeCentre AddOn 資料夾，無法下載".to_string());
    }
    Ok(path)
}

fn fixed_destination_string() -> Result<String, String> {
    fixed_destination_dir().map(|path| path.to_string_lossy().to_string())
}

fn fixed_destination_dir() -> Result<PathBuf, String> {
    let mut path = dirs::home_dir().ok_or_else(|| "無法取得使用者 home 目錄".to_string())?;
    for segment in TTC_ADDON_RELATIVE_PATH {
        path.push(segment);
    }
    Ok(path)
}

fn sync_launch_agent(state: &AppState) -> Result<(), String> {
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

fn now_string() -> String {
    let now: DateTime<Local> = Local::now();
    now.format("%Y-%m-%d %H:%M:%S").to_string()
}

fn format_system_time(time: std::time::SystemTime) -> String {
    DateTime::<Local>::from(time)
        .format("%Y-%m-%d %H:%M:%S")
        .to_string()
}

fn parse_local_time_string(value: &str) -> Option<DateTime<Local>> {
    NaiveDateTime::parse_from_str(value, "%Y-%m-%d %H:%M:%S")
        .ok()
        .and_then(|time| time.and_local_timezone(Local).earliest())
}

fn last_success_is_within_one_hour(last_success_at: &str) -> bool {
    let Some(last_success_at) = parse_local_time_string(last_success_at) else {
        return false;
    };
    Local::now()
        .signed_duration_since(last_success_at)
        .num_seconds()
        .abs()
        < 60 * 60
}

fn lock_error<T>(error: std::sync::PoisonError<T>) -> String {
    format!("內部狀態鎖定失敗：{error}")
}

fn to_string_error(error: impl std::fmt::Display) -> String {
    error.to_string()
}

fn setup_error(error: String) -> Box<dyn Error> {
    Box::new(io::Error::other(error))
}

fn default_schedule_interval_hours() -> u64 {
    3
}

fn sync_upload_file_time_env(upload_file_time: Option<&str>) {
    if let Some(upload_file_time) = upload_file_time {
        env::set_var(APP_UPLOAD_FILE_TIME_ENV, upload_file_time);
    } else {
        env::remove_var(APP_UPLOAD_FILE_TIME_ENV);
    }
}

fn default_upload_client_id() -> String {
    let now = Local::now().timestamp_nanos_opt().unwrap_or_default() as u128;
    let pid = std::process::id() as u128;
    let seed = now ^ (pid << 32);
    format!(
        "{:08x}-{:04x}-4{:03x}-8{:03x}-{:012x}",
        (seed & 0xffff_ffff) as u32,
        ((seed >> 32) & 0xffff) as u16,
        ((seed >> 48) & 0x0fff) as u16,
        ((seed >> 60) & 0x0fff) as u16,
        ((seed >> 16) & 0xffff_ffff_ffff) as u64
    )
}

#[cfg(test)]
mod tests {
    use super::{
        apple_languages_primary_is_chinese, default_schedule_interval_hours,
        last_success_is_within_one_hour,
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
