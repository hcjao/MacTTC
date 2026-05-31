use std::time::Duration;

use chrono::{DateTime, Local, NaiveDateTime};
use tauri::{AppHandle, Manager};

use crate::{
    config::{default_schedule_interval_hours, fixed_destination_string, is_allowed_download_url},
    downloader::run_job,
    lock_error, AppState,
};

pub(crate) fn restart_scheduler(app: AppHandle, state: &AppState) -> Result<(), String> {
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

pub(crate) fn maybe_run_startup_download(app: AppHandle, state: &AppState) {
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

fn parse_local_time_string(value: &str) -> Option<DateTime<Local>> {
    NaiveDateTime::parse_from_str(value, "%Y-%m-%d %H:%M:%S")
        .ok()
        .and_then(|time| time.and_local_timezone(Local).earliest())
}

pub(crate) fn last_success_is_within_one_hour(last_success_at: &str) -> bool {
    let Some(last_success_at) = parse_local_time_string(last_success_at) else {
        return false;
    };
    Local::now()
        .signed_duration_since(last_success_at)
        .num_seconds()
        .abs()
        < 60 * 60
}
