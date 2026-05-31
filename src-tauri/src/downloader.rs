use std::{
    fs::{self, File},
    io,
    path::{Path, PathBuf},
};

use tauri::{AppHandle, Emitter, Manager};
use tokio::{fs as async_fs, io::AsyncWriteExt};
use zip::ZipArchive;

use crate::{
    config::{ensure_destination_exists, save_config_to_disk, AppConfig},
    lock_error, now_string, AppState, JobStatus, JOB_STATUS_CHANGED_EVENT,
};

pub(crate) async fn run_job(
    app: AppHandle,
    state: &AppState,
    config: AppConfig,
) -> Result<(), String> {
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
