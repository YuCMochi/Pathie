// 前端（設定視窗）呼叫的 Tauri commands。

use std::sync::atomic::Ordering;

use serde::Serialize;
use tauri::{AppHandle, State};

use crate::app::AppState;
use crate::logic::RATE_OPTIONS;
use crate::platform;
use crate::settings::{self, Settings};
use crate::tray;
use crate::update;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    version: &'static str,
    author: &'static str,
    repository: &'static str,
    rate_options: &'static [u32],
}

#[tauri::command]
pub fn app_info() -> AppInfo {
    AppInfo {
        version: env!("CARGO_PKG_VERSION"),
        author: env!("CARGO_PKG_AUTHORS"),
        repository: env!("CARGO_PKG_REPOSITORY"),
        rate_options: &RATE_OPTIONS,
    }
}

#[tauri::command]
pub fn get_settings(state: State<AppState>) -> Settings {
    state.settings.read().unwrap().clone()
}

/// 整份設定替換並存檔；回傳實際生效（驗證後）的設定
#[tauri::command]
pub fn set_settings(state: State<AppState>, settings: Settings) -> Settings {
    let s = settings.sanitized();
    settings::save(&s);
    *state.settings.write().unwrap() = s.clone();
    s
}

#[tauri::command]
pub fn get_monitoring(state: State<AppState>) -> bool {
    state.monitoring.load(Ordering::Relaxed)
}

#[tauri::command]
pub fn set_monitoring(app: AppHandle, on: bool) {
    tray::set_monitoring(&app, on);
}

#[tauri::command]
pub fn get_startup() -> bool {
    platform::is_startup_enabled()
}

#[tauri::command]
pub fn set_startup(enable: bool) -> bool {
    platform::set_startup(enable);
    platform::is_startup_enabled()
}

#[tauri::command]
pub fn open_repository() {
    platform::shell_open(env!("CARGO_PKG_REPOSITORY"));
}

/// 設定頁載入完成時呼叫：需要的話此刻才顯示視窗，並回傳要切到的分頁
#[tauri::command]
pub fn ui_ready(app: AppHandle, state: State<AppState>) -> Option<String> {
    if state.show_on_ready.swap(false, Ordering::Relaxed) {
        tray::show_settings(&app, None);
    }
    state.initial_page.lock().unwrap().take()
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateStatus {
    version: String,
    /// false：免安裝版或缺少檢查碼，按鈕改成開啟下載頁
    installable: bool,
}

/// 有新版回傳版本資訊，已是最新版回傳 null
#[tauri::command]
pub async fn check_update(state: State<'_, AppState>) -> Result<Option<UpdateStatus>, String> {
    let found = tauri::async_runtime::spawn_blocking(update::check).await.map_err(|e| e.to_string())??;
    let status = found.as_ref().map(|u| UpdateStatus {
        version: u.version.clone(),
        installable: u.sha256.is_some() && platform::is_installed(),
    });
    *state.pending_update.lock().unwrap() = found;
    Ok(status)
}

/// 安裝版：下載、驗證後執行安裝程式並結束 Pathie；免安裝版：開啟 Release 頁
#[tauri::command]
pub async fn install_update(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    let u = state.pending_update.lock().unwrap().clone().ok_or("請先檢查更新")?;
    if u.sha256.is_none() || !platform::is_installed() {
        platform::shell_open(&u.page);
        return Ok(());
    }
    tauri::async_runtime::spawn_blocking(move || update::install(&u)).await.map_err(|e| e.to_string())??;
    app.exit(0);
    Ok(())
}
