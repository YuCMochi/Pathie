// 系統匣：左鍵切換監控，右鍵選單只留 暫停/恢復、設定、退出。

use std::sync::atomic::Ordering;

use tauri::image::Image;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{App, AppHandle, Emitter, Manager, Wry};

use crate::app::AppState;

const ICON_ON: &[u8] = include_bytes!("../../icon/on/32x32.ico");
const ICON_OFF: &[u8] = include_bytes!("../../icon/off/32x32.ico");
const TRAY_ID: &str = "main";

pub struct ToggleItem(MenuItem<Wry>);

pub fn setup(app: &App) -> tauri::Result<()> {
    let toggle = MenuItem::with_id(app, "toggle", "暫停監控", true, None::<&str>)?;
    let settings = MenuItem::with_id(app, "settings", "設定…", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
    let sep = PredefinedMenuItem::separator(app)?;
    let menu = Menu::with_items(app, &[&toggle, &settings, &sep, &quit])?;

    TrayIconBuilder::with_id(TRAY_ID)
        .icon(Image::from_bytes(ICON_ON)?)
        .tooltip(format!("Pathie 路徑自動開啟 v{}", env!("CARGO_PKG_VERSION")))
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, ev| match ev.id.as_ref() {
            "toggle" => toggle_monitoring(app),
            "settings" => show_settings(app, None),
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, ev| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = ev
            {
                toggle_monitoring(tray.app_handle());
            }
        })
        .build(app)?;
    app.manage(ToggleItem(toggle));
    Ok(())
}

pub fn toggle_monitoring(app: &AppHandle) {
    let on = !app.state::<AppState>().monitoring.load(Ordering::Relaxed);
    set_monitoring(app, on);
}

pub fn set_monitoring(app: &AppHandle, on: bool) {
    let state = app.state::<AppState>();
    state.monitoring.store(on, Ordering::Relaxed);
    if on {
        // 恢復後重新計數，否則剛恢復又立刻觸發
        state.opens.lock().unwrap().clear();
    }
    if let Some(tray) = app.tray_by_id(TRAY_ID) {
        if let Ok(icon) = Image::from_bytes(if on { ICON_ON } else { ICON_OFF }) {
            let _ = tray.set_icon(Some(icon));
        }
    }
    let _ = app
        .state::<ToggleItem>()
        .0
        .set_text(if on { "暫停監控" } else { "恢復監控" });
    let _ = app.emit("monitoring-changed", on);
}

/// 顯示設定視窗；`page` 指定要切到的分頁（前端監聽 `navigate` 事件）
pub fn show_settings(app: &AppHandle, page: Option<&str>) {
    let Some(win) = app.get_webview_window("settings") else {
        return;
    };
    let _ = win.unminimize();
    let _ = win.show();
    let _ = win.set_focus();
    if let Some(p) = page {
        let _ = win.emit("navigate", p);
    }
}
