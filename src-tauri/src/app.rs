// 應用程式狀態與剪貼簿處理流程。

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, RwLock};
use std::time::Instant;

use tauri::{AppHandle, Manager};
use tauri_plugin_notification::NotificationExt;

use crate::logic::{decide, is_unc, normalize, parse_url, rate_exceeded, Act};
use crate::platform::{foreground_is_browser, reveal, shell_open};
use crate::settings::Settings;
use crate::toast;
use crate::tray;

pub struct AppState {
    pub settings: RwLock<Settings>,
    pub monitoring: AtomicBool,
    /// 最近 RATE_WINDOW 內每次自動開啟的時間點
    pub opens: Mutex<VecDeque<Instant>>,
}

impl AppState {
    pub fn new(settings: Settings) -> Self {
        Self {
            settings: RwLock::new(settings),
            monitoring: AtomicBool::new(true),
            opens: Mutex::new(VecDeque::new()),
        }
    }
}

pub fn notify(app: &AppHandle, title: &str, body: &str) {
    let _ = app.notification().builder().title(title).body(body).show();
}

pub fn handle_clipboard(app: &AppHandle) {
    let state = app.state::<AppState>();
    if !state.monitoring.load(Ordering::Relaxed) {
        return;
    }
    let cfg = state.settings.read().unwrap().clone();
    // clipboard-win 內建重試：WM_CLIPBOARDUPDATE 當下來源程式往往還佔著剪貼簿，
    // 直接 OpenClipboard 會拿到 ACCESS_DENIED
    let Ok(text) = clipboard_win::get_clipboard_string() else {
        return;
    };
    // 網址直接開啟（已限制為 http/https），其餘照路徑規則處理。
    // 網址若被開關擋下就直接結束，不能落入路徑分支（www.x 之類可能被當成檔名）
    if let Some(url) = parse_url(&text) {
        if !cfg.open_url || (cfg.skip_url_from_browser && foreground_is_browser()) {
            return;
        }
        return act_on(app, &cfg, url, Act::Open);
    }
    let Some(path) = normalize(&text) else {
        return;
    };
    if is_unc(&path) && !cfg.allow_unc {
        return;
    }
    if let Some(act) = decide(&path, cfg.file_action) {
        act_on(app, &cfg, path.to_string_lossy().into_owned(), act);
    }
}

/// 頻率檢查後執行動作；網址與路徑共用同一個計數
fn act_on(app: &AppHandle, cfg: &Settings, target: String, act: Act) {
    let limit = cfg.rate_limit_per_minute;
    let state = app.state::<AppState>();
    let exceeded = rate_exceeded(&mut state.opens.lock().unwrap(), Instant::now(), limit);
    if exceeded {
        // 自動暫停而非只丟棄：洪水持續時每次都通知本身就是干擾
        tray::set_monitoring(app, false);
        notify(
            app,
            "Pathie 已暫停監控",
            &format!("偵測到每分鐘超過 {limit} 次的開啟請求，可能有程式在大量複製路徑。左鍵點圖示可恢復。"),
        );
        return;
    }
    match act {
        Act::Open => shell_open(&target),
        Act::Reveal => reveal(&target),
    }
    if cfg.show_toast {
        let label = match act {
            Act::Open => "由 Pathie 開啟",
            Act::Reveal => "由 Pathie 在檔案總管選取",
        };
        toast::show(app, label, &target);
    }
}
