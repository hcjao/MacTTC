use chrono::{DateTime, Local, NaiveDateTime};
use serde::{Deserialize, Serialize};
use std::{
    env,
    error::Error,
    fs::{self, File},
    io,
    path::{Path, PathBuf},
    sync::Mutex,
    time::Duration,
};
use tauri::{
    async_runtime::JoinHandle,
    image::Image,
    menu::{CheckMenuItem, MenuBuilder, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    ActivationPolicy, AppHandle, Emitter, Manager, State, WindowEvent, Wry,
};
use tokio::{fs as async_fs, io::AsyncWriteExt};
use zip::ZipArchive;

const CONFIG_FILE: &str = "config.json";
const LAUNCH_AGENT_LABEL: &str = "com.macttc.downloader";
const TRAY_SHOW_ID: &str = "show";
const TRAY_QUIT_ID: &str = "quit";
const TRAY_SCHEDULE_LABEL_ID: &str = "schedule_label";
const TRAY_SCHEDULE_OFF_ID: &str = "schedule_off";
const TRAY_INTERVAL_3_ID: &str = "interval_3";
const TRAY_AUTOSTART_ID: &str = "autostart";
const JOB_STATUS_CHANGED_EVENT: &str = "job-status-changed";
const NA_PRICE_TABLE_URL: &str = "https://us.tamrieltradecentre.com/download/PriceTable";
const EU_PRICE_TABLE_URL: &str = "https://eu.tamrieltradecentre.com/download/PriceTable";
const TTC_ADDON_RELATIVE_PATH: &[&str] = &[
    "Documents",
    "Elder Scrolls Online",
    "live",
    "AddOns",
    "TamrielTradeCentre",
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
    tray_controls: Mutex<Option<TrayControls>>,
}

struct TrayControls {
    show: MenuItem<Wry>,
    schedule_label: MenuItem<Wry>,
    schedule_off: CheckMenuItem<Wry>,
    interval_3: CheckMenuItem<Wry>,
    autostart: CheckMenuItem<Wry>,
    quit: MenuItem<Wry>,
}

struct TrayLabels {
    show: &'static str,
    schedule_title: &'static str,
    schedule_off: &'static str,
    interval_3: &'static str,
    autostart: &'static str,
    quit: &'static str,
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

fn setup_tray(app: &AppHandle) -> Result<(), String> {
    let config = app
        .state::<AppState>()
        .config
        .lock()
        .map_err(lock_error)?
        .clone();
    let labels = tray_labels();
    let show = MenuItem::with_id(app, TRAY_SHOW_ID, labels.show, true, None::<&str>)
        .map_err(to_string_error)?;
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
    let quit = MenuItem::with_id(app, TRAY_QUIT_ID, labels.quit, true, None::<&str>)
        .map_err(to_string_error)?;
    let menu = MenuBuilder::new(app)
        .item(&show)
        .separator()
        .item(&schedule_label)
        .item(&schedule_off)
        .item(&interval_3)
        .separator()
        .item(&autostart)
        .separator()
        .item(&quit)
        .build()
        .map_err(to_string_error)?;

    *app.state::<AppState>()
        .tray_controls
        .lock()
        .map_err(lock_error)? = Some(TrayControls {
        show,
        schedule_label,
        schedule_off,
        interval_3,
        autostart,
        quit,
    });

    let tray_icon = Image::new(include_bytes!("../icons/tray-coin.rgba"), 64, 64);

    let tray = TrayIconBuilder::with_id("main")
        .menu(&menu)
        .icon(tray_icon)
        .icon_as_template(true)
        .tooltip("MacTTC")
        .show_menu_on_left_click(true)
        .on_menu_event(|app, event| match event.id().as_ref() {
            TRAY_SHOW_ID => {
                let _ = refresh_tray(app);
                show_main_window(app);
            }
            TRAY_SCHEDULE_OFF_ID => {
                let _ = refresh_tray(app);
                let _ = disable_schedule(app.clone());
            }
            TRAY_INTERVAL_3_ID => {
                let _ = refresh_tray(app);
                let _ = enable_schedule(app.clone());
            }
            TRAY_AUTOSTART_ID => {
                let _ = refresh_tray(app);
                let _ = toggle_autostart(app.clone());
            }
            TRAY_QUIT_ID => {
                let _ = refresh_tray(app);
                app.exit(0);
            }
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if matches!(
                event,
                TrayIconEvent::Click {
                    button: MouseButton::Left | MouseButton::Right,
                    button_state: MouseButtonState::Down,
                    ..
                }
            ) {
                let _ = refresh_tray(tray.app_handle());
            }
        });

    tray.build(app).map_err(to_string_error)?;
    Ok(())
}

fn refresh_tray(app: &AppHandle) -> Result<(), String> {
    let state = app.state::<AppState>();
    update_tray_labels(state.inner())?;
    update_tray_checks(state.inner())
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

fn update_tray_labels(state: &AppState) -> Result<(), String> {
    let labels = tray_labels();
    if let Some(controls) = state.tray_controls.lock().map_err(lock_error)?.as_ref() {
        controls
            .show
            .set_text(labels.show)
            .map_err(to_string_error)?;
        controls
            .schedule_label
            .set_text(labels.schedule_title)
            .map_err(to_string_error)?;
        controls
            .schedule_off
            .set_text(labels.schedule_off)
            .map_err(to_string_error)?;
        controls
            .interval_3
            .set_text(labels.interval_3)
            .map_err(to_string_error)?;
        controls
            .autostart
            .set_text(labels.autostart)
            .map_err(to_string_error)?;
        controls
            .quit
            .set_text(labels.quit)
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
