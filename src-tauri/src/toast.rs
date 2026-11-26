// 開啟成功的右下角小浮窗：視窗在 tauri.conf.json 預先建立並隱藏，
// 這裡只負責定位、顯示、送出文字、約 2 秒後隱藏。不搶焦點、滑鼠可穿透。

use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition};

const SHOW_FOR: Duration = Duration::from_millis(2200);
const MARGIN: f64 = 16.0;

/// 每次顯示遞增；計時結束時若已被新的提示取代就不隱藏
static GENERATION: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Serialize)]
struct Payload<'a> {
    label: &'a str,
    name: &'a str,
}

/// 浮窗上顯示的名稱：路徑取檔名（磁碟根目錄就顯示整串），網址去掉 scheme
pub fn display_name(target: &str) -> String {
    if let Some(rest) = target.strip_prefix("https://").or_else(|| target.strip_prefix("http://")) {
        return rest.to_owned();
    }
    Path::new(target)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| target.to_owned())
}

pub fn show(app: &AppHandle, label: &str, target: &str) {
    let Some(win) = app.get_webview_window("toast") else {
        return;
    };
    let name = display_name(target);

    // 定位到主螢幕工作區（不含工作列）右下角
    if let (Ok(Some(mon)), Ok(size)) = (win.primary_monitor(), win.outer_size()) {
        let wa = mon.work_area();
        let m = (MARGIN * mon.scale_factor()) as i32;
        let x = wa.position.x + wa.size.width as i32 - size.width as i32 - m;
        let y = wa.position.y + wa.size.height as i32 - size.height as i32 - m;
        let _ = win.set_position(PhysicalPosition::new(x, y));
    }
    let _ = win.set_ignore_cursor_events(true);
    let _ = app.emit_to("toast", "toast", Payload { label, name: &name });
    let _ = win.show();

    let gen = GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
    std::thread::spawn(move || {
        std::thread::sleep(SHOW_FOR);
        if GENERATION.load(Ordering::SeqCst) == gen {
            let _ = win.hide();
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_name_shortens_paths_and_urls() {
        assert_eq!(display_name(r"C:\a\b\report.pdf"), "report.pdf");
        assert_eq!(display_name(r"C:\a\b"), "b");
        assert_eq!(display_name(r"C:\"), r"C:\");
        assert_eq!(display_name("https://example.com/x"), "example.com/x");
    }
}
