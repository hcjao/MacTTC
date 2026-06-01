use tauri::{
    image::Image,
    menu::{CheckMenuItem, MenuBuilder, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Manager, Wry,
};

use crate::{
    config::{
        default_schedule_interval_hours, fixed_destination_string, save_config_to_disk,
        trade_url_for_download_source, AppConfig,
    },
    localization::{tray_labels, TrayLabels},
    lock_error,
    scheduler::restart_scheduler,
    sync_autostart, to_string_error, AppState,
};

const TRAY_SHOW_ID: &str = "show";
const TRAY_QUIT_ID: &str = "quit";
const TRAY_SCHEDULE_LABEL_ID: &str = "schedule_label";
const TRAY_SCHEDULE_OFF_ID: &str = "schedule_off";
const TRAY_INTERVAL_3_ID: &str = "interval_3";
const TRAY_DATA_TIME_LABEL_ID: &str = "data_time_label";
const TRAY_DATA_TIME_VALUE_ID: &str = "data_time_value";
const TRAY_AUTOSTART_ID: &str = "autostart";
const TRAY_TTC_WEBSITE_ID: &str = "ttc_website";

pub(crate) struct TrayControls {
    show: MenuItem<Wry>,
    schedule_label: MenuItem<Wry>,
    schedule_off: CheckMenuItem<Wry>,
    interval_3: CheckMenuItem<Wry>,
    data_time_label: MenuItem<Wry>,
    data_time_value: MenuItem<Wry>,
    autostart: CheckMenuItem<Wry>,
    ttc_website: MenuItem<Wry>,
    quit: MenuItem<Wry>,
}

pub(crate) fn setup_tray(app: &AppHandle) -> Result<(), String> {
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
    let data_time_label = MenuItem::with_id(
        app,
        TRAY_DATA_TIME_LABEL_ID,
        labels.data_time_title,
        false,
        None::<&str>,
    )
    .map_err(to_string_error)?;
    let data_time_value = MenuItem::with_id(
        app,
        TRAY_DATA_TIME_VALUE_ID,
        data_time_text(&labels, &config),
        false,
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
    let ttc_website = MenuItem::with_id(
        app,
        TRAY_TTC_WEBSITE_ID,
        labels.ttc_website,
        true,
        None::<&str>,
    )
    .map_err(to_string_error)?;
    let quit = MenuItem::with_id(app, TRAY_QUIT_ID, labels.quit, true, None::<&str>)
        .map_err(to_string_error)?;
    let menu = MenuBuilder::new(app)
        .item(&show)
        .separator()
        .item(&autostart)
        .separator()
        .item(&schedule_label)
        .item(&schedule_off)
        .item(&interval_3)
        .separator()
        .item(&data_time_label)
        .item(&data_time_value)
        .separator()
        .item(&ttc_website)
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
        data_time_label,
        data_time_value,
        autostart,
        ttc_website,
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
            TRAY_TTC_WEBSITE_ID => {
                let _ = refresh_tray(app);
                let _ = open_ttc_website(app);
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
    sync_autostart(&app, state.inner())?;
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
    let config = state.config.lock().map_err(lock_error)?.clone();
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
            .data_time_label
            .set_text(labels.data_time_title)
            .map_err(to_string_error)?;
        controls
            .data_time_value
            .set_text(data_time_text(&labels, &config))
            .map_err(to_string_error)?;
        controls
            .autostart
            .set_text(labels.autostart)
            .map_err(to_string_error)?;
        controls
            .ttc_website
            .set_text(labels.ttc_website)
            .map_err(to_string_error)?;
        controls
            .quit
            .set_text(labels.quit)
            .map_err(to_string_error)?;
    }
    Ok(())
}

fn data_time_text<'a>(labels: &'a TrayLabels, config: &'a AppConfig) -> &'a str {
    config
        .last_success_at
        .as_deref()
        .unwrap_or(labels.data_time_empty)
}

fn open_ttc_website(app: &AppHandle) -> Result<(), String> {
    let config = app
        .state::<AppState>()
        .config
        .lock()
        .map_err(lock_error)?
        .clone();
    let source_url = config
        .last_success_source_url
        .as_deref()
        .unwrap_or(&config.url);
    let trade_url = trade_url_for_download_source(source_url)
        .ok_or_else(|| "找不到可開啟的 TTC 網站來源".to_string())?;

    std::process::Command::new("open")
        .arg(trade_url)
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("無法開啟 TTC 網站：{error}"))
}

fn show_main_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.set_focus();
    }
}
