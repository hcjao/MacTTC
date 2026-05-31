use std::{fs, path::PathBuf};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

const CONFIG_FILE: &str = "config.json";
pub(crate) const NA_PRICE_TABLE_URL: &str = "https://us.tamrieltradecentre.com/download/PriceTable";
pub(crate) const EU_PRICE_TABLE_URL: &str = "https://eu.tamrieltradecentre.com/download/PriceTable";
const TTC_ADDON_RELATIVE_PATH: &[&str] = &[
    "Documents",
    "Elder Scrolls Online",
    "live",
    "AddOns",
    "TamrielTradeCentre",
];

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AppConfig {
    pub(crate) url: String,
    pub(crate) destination_dir: String,
    #[serde(default)]
    pub(crate) schedule_enabled: bool,
    #[serde(default = "default_schedule_interval_hours")]
    pub(crate) schedule_interval_hours: u64,
    #[serde(default)]
    pub(crate) autostart_enabled: bool,
    #[serde(default)]
    pub(crate) last_success_at: Option<String>,
    #[serde(default)]
    pub(crate) last_success_source_url: Option<String>,
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
pub(crate) struct DestinationStatus {
    path: String,
    exists: bool,
}

pub(crate) fn load_config(app: &AppHandle) -> Result<AppConfig, String> {
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

pub(crate) fn save_config_to_disk(app: &AppHandle, config: &AppConfig) -> Result<(), String> {
    let path = config_path(app)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| format!("無法建立設定資料夾：{error}"))?;
    }
    let content =
        serde_json::to_string_pretty(config).map_err(|error| format!("序列化設定失敗：{error}"))?;
    fs::write(path, content).map_err(|error| format!("儲存設定失敗：{error}"))
}

pub(crate) fn is_allowed_download_url(url: &str) -> bool {
    matches!(url, NA_PRICE_TABLE_URL | EU_PRICE_TABLE_URL)
}

pub(crate) fn normalized_config(mut config: AppConfig) -> Result<AppConfig, String> {
    config.destination_dir = fixed_destination_string()?;
    config.schedule_interval_hours = default_schedule_interval_hours();
    validate_config(&config)?;
    Ok(config)
}

pub(crate) fn destination_status() -> Result<DestinationStatus, String> {
    let path = fixed_destination_dir()?;
    Ok(DestinationStatus {
        exists: path.is_dir(),
        path: path.to_string_lossy().to_string(),
    })
}

pub(crate) fn ensure_destination_exists() -> Result<PathBuf, String> {
    let path = fixed_destination_dir()?;
    if !path.is_dir() {
        return Err("找不到 TamrielTradeCentre AddOn 資料夾，無法下載".to_string());
    }
    Ok(path)
}

pub(crate) fn fixed_destination_string() -> Result<String, String> {
    fixed_destination_dir().map(|path| path.to_string_lossy().to_string())
}

pub(crate) fn default_schedule_interval_hours() -> u64 {
    3
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

fn config_path(app: &AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_config_dir()
        .map(|dir| dir.join(CONFIG_FILE))
        .map_err(|error| format!("無法取得設定資料夾：{error}"))
}

fn fixed_destination_dir() -> Result<PathBuf, String> {
    let mut path = dirs::home_dir().ok_or_else(|| "無法取得使用者 home 目錄".to_string())?;
    for segment in TTC_ADDON_RELATIVE_PATH {
        path.push(segment);
    }
    Ok(path)
}
