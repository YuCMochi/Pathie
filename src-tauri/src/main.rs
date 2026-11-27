// Pathie：常駐系統匣，偵測剪貼簿中的有效路徑並自動開啟。
//
// cfg_attr 是必要的：直接寫 windows_subsystem 會連 `cargo test` 的測試執行檔
// 一起變成 windows subsystem，測試輸出就看不到了。
#![cfg_attr(not(test), windows_subsystem = "windows")]

mod app;
mod commands;
mod logic;
mod platform;
mod settings;
mod toast;
mod tray;
mod update;

use std::sync::atomic::Ordering;

use tauri::{Manager, RunEvent, WindowEvent};

use app::{notify, AppState};

const VERSION: &str = env!("CARGO_PKG_VERSION");

fn main() {
    tauri::Builder::default()
        // 單一實例；第二個實例啟動時請既有實例跳通知，讓使用者雙擊 exe 時一定有回饋
        .plugin(tauri_plugin_single_instance::init(|app, args, _cwd| {
            if args.iter().any(|a| a == platform::SETTINGS_ARG) {
                return tray::show_settings(app, None);
            }
            let state = if app.state::<AppState>().monitoring.load(Ordering::Relaxed) {
                "監控中。"
            } else {
                "目前暫停監控，左鍵點圖示可恢復。"
            };
            notify(
                app,
                &format!("Pathie v{VERSION} 已在執行中"),
                &format!("圖示在系統匣，右鍵開啟選單。{state}"),
            );
        }))
        .plugin(tauri_plugin_notification::init())
        .invoke_handler(tauri::generate_handler![
            commands::app_info,
            commands::get_settings,
            commands::set_settings,
            commands::get_monitoring,
            commands::set_monitoring,
            commands::get_startup,
            commands::set_startup,
            commands::open_repository,
            commands::ui_ready,
            commands::check_update,
            commands::install_update,
        ])
        .setup(|app| {
            let args: Vec<String> = std::env::args().collect();
            let has = |flag: &str| args.iter().any(|a| a == flag);
            let mut loaded = settings::load();
            let state = AppState::new(loaded.clone());
            // 首次啟動自動帶出「使用說明」；開機自動啟動時不打擾，留到下次手動啟動
            if !loaded.first_run_done && !has(platform::AUTOSTART_ARG) {
                loaded.first_run_done = true;
                settings::save(&loaded);
                *state.settings.write().unwrap() = loaded;
                *state.initial_page.lock().unwrap() = Some("guide".into());
                state.show_on_ready.store(true, Ordering::Relaxed);
            }
            if has(platform::SETTINGS_ARG) {
                state.show_on_ready.store(true, Ordering::Relaxed);
            }
            app.manage(state);
            if platform::is_startup_enabled() {
                // 把舊格式（沒有 --autostart）的開機設定升級成新格式；已是新格式則等於重寫同一個值
                platform::set_startup(true);
            }
            tray::setup(app)?;

            // 關閉設定視窗只是隱藏，程式繼續在系統匣
            if let Some(win) = app.get_webview_window("settings") {
                let w = win.clone();
                win.on_window_event(move |e| {
                    if let WindowEvent::CloseRequested { api, .. } = e {
                        api.prevent_close();
                        let _ = w.hide();
                    }
                });
            }

            let handle = app.handle().clone();
            platform::spawn_clipboard_listener(move || app::handle_clipboard(&handle));

            // 開機自動啟動不跳通知，手動雙擊才跳
            if !std::env::args().any(|a| a == platform::AUTOSTART_ARG) {
                notify(
                    app.handle(),
                    &format!("Pathie v{VERSION} 已啟動"),
                    "複製檔案或資料夾路徑即可自動開啟。圖示在系統匣，右鍵開啟選單。",
                );
            }
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("Tauri 啟動失敗")
        .run(|_app, event| {
            // 所有視窗都是隱藏的，沒有視窗時不能結束；只有明確呼叫 exit 才離開
            if let RunEvent::ExitRequested { api, code, .. } = event {
                if code.is_none() {
                    api.prevent_exit();
                }
            }
        });
}
