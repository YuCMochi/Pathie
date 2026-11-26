// Win32 平台層：開啟/選取、環境變數、前景視窗、開機自動啟動(Run 值)。

use std::cell::RefCell;
use std::path::PathBuf;

use windows::core::{w, PCWSTR, PWSTR};
use windows::Win32::Foundation::{CloseHandle, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::Com::{CoInitializeEx, COINIT_APARTMENTTHREADED};
use windows::Win32::System::DataExchange::AddClipboardFormatListener;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Environment::ExpandEnvironmentStringsW;
use windows::Win32::System::Threading::{OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION};
use windows::Win32::UI::Shell::{ILCreateFromPathW, ILFree, PathCreateFromUrlW, SHOpenFolderAndSelectItems, ShellExecuteW};
use windows::Win32::UI::WindowsAndMessaging::*;
use winreg::enums::{HKEY_CURRENT_USER, KEY_SET_VALUE};
use winreg::RegKey;

use crate::logic::{is_browser_exe, FileAction, RATE_DEFAULT, RATE_OPTIONS};
use crate::settings::Settings;

// 開機自動啟動 (Run 值)
pub const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
pub const RUN_VALUE: &str = "Pathie";
// 開機自動啟動時帶的參數，用來跳過「已啟動」通知
pub const AUTOSTART_ARG: &str = "--autostart";
// 啟動（或重複啟動）時直接開啟設定視窗
pub const SETTINGS_ARG: &str = "--settings";

/// 目前前景視窗是否屬於瀏覽器；查不到（權限不足等）一律視為否，維持原行為
pub fn foreground_is_browser() -> bool {
    unsafe {
        let mut pid = 0u32;
        GetWindowThreadProcessId(GetForegroundWindow(), Some(&mut pid));
        let Ok(h) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) else {
            return false;
        };
        let mut buf = [0u16; 520];
        let mut len = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(h, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut len);
        let _ = CloseHandle(h);
        ok.is_ok() && is_browser_exe(&String::from_utf16_lossy(&buf[..len as usize]))
    }
}

/// 在檔案總管開啟所在資料夾並選取該檔案
///
/// 不用 `explorer /select`：它每次都開新視窗；SHOpenFolderAndSelectItems 會重用
/// 已開啟該資料夾的視窗（需要先 CoInitializeEx，見 main）。
pub fn reveal(path: &str) {
    let wide: Vec<u16> = path.encode_utf16().chain(Some(0)).collect();
    unsafe {
        let pidl = ILCreateFromPathW(PCWSTR(wide.as_ptr()));
        if pidl.is_null() {
            return;
        }
        // cidl=0：pidl 本身就是要選取的項目，Explorer 會自己找出父資料夾
        let _ = SHOpenFolderAndSelectItems(pidl, Some(&[]), 0);
        ILFree(Some(pidl));
    }
}

/// 用預設程式開啟路徑或網址（等同在檔案總管雙擊）
pub fn shell_open(target: &str) {
    shell_execute(target, None);
}

fn shell_execute(target: &str, params: Option<&str>) {
    let wide = |s: &str| -> Vec<u16> { s.encode_utf16().chain(Some(0)).collect() };
    let (t, p) = (wide(target), params.map(wide));
    unsafe {
        ShellExecuteW(
            HWND::default(),
            PCWSTR::null(),
            PCWSTR(t.as_ptr()),
            p.as_ref().map_or(PCWSTR::null(), |p| PCWSTR(p.as_ptr())),
            PCWSTR::null(),
            SW_SHOWNORMAL,
        );
    }
}

/// `new Uri(x).LocalPath` 的對應 API：percent-decoding 與 UNC 都交給 shlwapi，
/// 省掉一個 url crate。
pub fn url_to_path(url: &str) -> Option<String> {
    let src: Vec<u16> = url.encode_utf16().chain(Some(0)).collect();
    let mut buf = vec![0u16; 4096];
    let mut len = buf.len() as u32;
    unsafe {
        PathCreateFromUrlW(PCWSTR(src.as_ptr()), PWSTR(buf.as_mut_ptr()), &mut len, 0).ok()?;
    }
    Some(String::from_utf16_lossy(&buf[..len as usize]))
}

pub fn expand_env(s: &str) -> String {
    let src: Vec<u16> = s.encode_utf16().chain(Some(0)).collect();
    unsafe {
        let n = ExpandEnvironmentStringsW(PCWSTR(src.as_ptr()), None);
        if n == 0 {
            return s.to_owned();
        }
        let mut buf = vec![0u16; n as usize];
        let written = ExpandEnvironmentStringsW(PCWSTR(src.as_ptr()), Some(&mut buf));
        if written == 0 {
            return s.to_owned();
        }
        // 回傳值含結尾的 null
        String::from_utf16_lossy(&buf[..(written as usize).saturating_sub(1)])
    }
}

pub fn exe_path() -> String {
    std::env::current_exe()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_default()
}

pub fn is_startup_enabled() -> bool {
    let Ok(key) = RegKey::predef(HKEY_CURRENT_USER).open_subkey(RUN_KEY) else {
        return false;
    };
    let Ok(val) = key.get_value::<String, _>(RUN_VALUE) else {
        return false;
    };
    let exe = exe_path();
    !exe.is_empty() && run_value_path(&val).eq_ignore_ascii_case(&exe)
}

/// 取出 Run 值裡的 exe 路徑。新格式是 `"路徑" --autostart`，
/// v2.1.0 以前（含 C# 版）寫的是 `"路徑"`，兩種都要認得。
fn run_value_path(val: &str) -> &str {
    let val = val.trim();
    match val.strip_prefix('"').and_then(|rest| rest.split_once('"')) {
        Some((path, _args)) => path,
        None => val.trim_end_matches(AUTOSTART_ARG).trim_end(),
    }
}

pub fn set_startup(enable: bool) {
    let Ok(key) =
        RegKey::predef(HKEY_CURRENT_USER).open_subkey_with_flags(RUN_KEY, KEY_SET_VALUE)
    else {
        return;
    };
    if enable {
        let value = format!("\"{}\" {AUTOSTART_ARG}", exe_path());
        let _ = key.set_value(RUN_VALUE, &value);
    } else {
        let _ = key.delete_value(RUN_VALUE);
    }
}

// --- 設定檔位置與舊版(登錄檔)設定 ---------------------------------------

/// `%APPDATA%\Pathie`
pub fn config_dir() -> PathBuf {
    std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join("Pathie")
}

/// 0.2.x 以前存在 `HKCU\Software\Pathie` 的設定；沒有這個機碼回傳 None（全新安裝）
pub fn legacy_settings() -> Option<Settings> {
    let key = RegKey::predef(HKEY_CURRENT_USER).open_subkey(r"Software\Pathie").ok()?;
    let num = |name: &str| key.get_value::<u32, _>(name).ok();
    let flag = |name: &str, default: bool| num(name).map_or(default, |v| v != 0);
    Some(Settings {
        file_action: num("FileAction").and_then(FileAction::from_u32).unwrap_or_default(),
        allow_unc: flag("AllowUnc", false),
        open_url: flag("OpenUrl", true),
        skip_url_from_browser: flag("SkipUrlFromBrowser", true),
        rate_limit_per_minute: num("RateLimitPerMinute")
            .filter(|n| RATE_OPTIONS.contains(n))
            .unwrap_or(RATE_DEFAULT),
        ..Default::default()
    })
}

// --- 剪貼簿監聽 ------------------------------------------------------------

thread_local! {
    static ON_UPDATE: RefCell<Option<Box<dyn Fn()>>> = const { RefCell::new(None) };
}

/// 在獨立執行緒開一個隱藏視窗訂閱剪貼簿變動，每次變動呼叫 `on_update`。
/// 不用 HWND_MESSAGE：message-only window 搭剪貼簿監聽有已知的雷。
pub fn spawn_clipboard_listener(on_update: impl Fn() + Send + 'static) {
    std::thread::spawn(move || unsafe {
        // SHOpenFolderAndSelectItems 需要 COM；STA 對應有訊息迴圈的執行緒
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        ON_UPDATE.with(|c| *c.borrow_mut() = Some(Box::new(on_update)));
        let Ok(hinst) = GetModuleHandleW(None) else { return };
        let class = w!("PathieClipboardWnd");
        RegisterClassW(&WNDCLASSW {
            lpfnWndProc: Some(clipboard_proc),
            hInstance: hinst.into(),
            lpszClassName: class,
            ..Default::default()
        });
        let Ok(hwnd) = CreateWindowExW(
            WINDOW_EX_STYLE(0),
            class,
            w!("Pathie"),
            WS_OVERLAPPED,
            0,
            0,
            0,
            0,
            None,
            None,
            hinst,
            None,
        ) else {
            return;
        };
        let _ = AddClipboardFormatListener(hwnd);
        let mut msg = MSG::default();
        // > 0：GetMessageW 出錯時回 -1，用 as_bool() 會變成無窮迴圈
        while GetMessageW(&mut msg, HWND::default(), 0, 0).0 > 0 {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    });
}

unsafe extern "system" fn clipboard_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if msg == WM_CLIPBOARDUPDATE {
        ON_UPDATE.with(|c| {
            if let Some(f) = c.borrow().as_ref() {
                f();
            }
        });
        return LRESULT(0);
    }
    DefWindowProcW(hwnd, msg, wp, lp)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_value_path_reads_old_and_new_formats() {
        let exe = r"C:\Program Files\My App\Pathie.exe";
        let cases = [
            format!("\"{exe}\" --autostart"), // 新格式
            format!("\"{exe}\""),             // v2.1.0 以前、C# 版
            format!("  \"{exe}\"  "),
        ];
        for v in &cases {
            assert_eq!(run_value_path(v), exe, "無法解析: {v:?}");
        }
        // 沒加引號（不是我們寫的，但別人手動改過也要能讀）
        assert_eq!(run_value_path(r"C:\x\a.exe"), r"C:\x\a.exe");
        assert_eq!(run_value_path(r"C:\x\a.exe --autostart"), r"C:\x\a.exe");
    }
}
