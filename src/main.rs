// Pathie：常駐系統匣，偵測剪貼簿中的有效路徑並自動開啟。
//
// cfg_attr 是必要的：直接寫 windows_subsystem 會連 `cargo test` 的測試執行檔
// 一起變成 windows subsystem，測試輸出就看不到了。
#![cfg_attr(not(test), windows_subsystem = "windows")]

use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use windows::core::{w, PCWSTR, PWSTR};
use windows::Win32::Foundation::{
    GetLastError, ERROR_ALREADY_EXISTS, HMODULE, HWND, LPARAM, LRESULT, POINT, WPARAM,
};
use windows::Win32::System::DataExchange::{
    AddClipboardFormatListener, RemoveClipboardFormatListener,
};
use windows::Win32::System::Environment::ExpandEnvironmentStringsW;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::CreateMutexW;
use windows::Win32::UI::Shell::{
    PathCreateFromUrlW, ShellExecuteW, Shell_NotifyIconW, NIF_ICON, NIF_INFO, NIF_MESSAGE, NIF_TIP,
    NIIF_INFO, NIM_ADD, NIM_DELETE, NIM_MODIFY, NOTIFYICONDATAW,
};
use windows::Win32::UI::WindowsAndMessaging::*;
use winreg::enums::{HKEY_CURRENT_USER, KEY_SET_VALUE};
use winreg::RegKey;

// 登錄檔位置與值名：開機自動啟動 (Run 值) 與使用者設定
const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const RUN_VALUE: &str = "Pathie";
// 開機自動啟動時帶的參數，用來跳過「已啟動」通知
const AUTOSTART_ARG: &str = "--autostart";
// 「僅開啟資料夾」開關的存放位置（DWORD，缺值視為開啟）
const SETTINGS_KEY: &str = r"Software\Pathie";
const FOLDERS_ONLY_VALUE: &str = "FoldersOnly";
// 防濫用：每分鐘最多自動開啟幾次，超過就自動暫停監控並通知
const RATE_LIMIT_VALUE: &str = "RateLimitPerMinute";
const RATE_WINDOW: Duration = Duration::from_secs(60);
const RATE_OPTIONS: [u32; 4] = [10, 20, 50, 100];
const RATE_DEFAULT: u32 = 20;

const WM_TRAY: u32 = WM_APP + 1;
// 第二個實例啟動時送給既有實例，請它跳通知，讓使用者雙擊 exe 時一定有回饋
const WM_ALREADY_RUNNING: u32 = WM_APP + 2;
const TRAY_UID: u32 = 1;
const CLASS_NAME: PCWSTR = w!("PathieWnd");
const VERSION: &str = env!("CARGO_PKG_VERSION");

const ID_STARTUP: usize = 1;
const ID_TOGGLE: usize = 2;
const ID_EXIT: usize = 3;
const ID_ABOUT: usize = 4;
const ID_FOLDERS_ONLY: usize = 5;
// 頻率上限子選單：ID_RATE_BASE + RATE_OPTIONS 的索引
const ID_RATE_BASE: usize = 100;

// 對應 app.rc 的資源 ID
const IDI_ON: u16 = 2;
const IDI_OFF: u16 = 3;

thread_local! {
    static MONITORING: Cell<bool> = const { Cell::new(true) };
    static FOLDERS_ONLY: Cell<bool> = const { Cell::new(true) };
    static RATE_LIMIT: Cell<u32> = const { Cell::new(RATE_DEFAULT) };
    // 最近 RATE_WINDOW 內每次自動開啟的時間點
    static OPENS: RefCell<VecDeque<Instant>> = const { RefCell::new(VecDeque::new()) };
    static ICON_ON: Cell<HICON> = Cell::new(HICON::default());
    static ICON_OFF: Cell<HICON> = Cell::new(HICON::default());
    // explorer.exe 重啟時廣播的訊息；收到要重新加入托盤圖示，否則圖示消失但程式還在跑
    static WM_TASKBAR_CREATED: Cell<u32> = const { Cell::new(0) };
}

fn main() {
    unsafe {
        // 單一實例。handle 不釋放,活到行程結束。
        let _mutex = CreateMutexW(None, true, w!("Pathie_SingleInstance_Mutex"));
        if GetLastError() == ERROR_ALREADY_EXISTS {
            // 找不到既有視窗就直接結束
            if let Ok(existing) = FindWindowW(CLASS_NAME, PCWSTR::null()) {
                let _ = PostMessageW(existing, WM_ALREADY_RUNNING, WPARAM(0), LPARAM(0));
            }
            return;
        }

        let hinst: HMODULE = GetModuleHandleW(None).expect("GetModuleHandleW");
        let wc = WNDCLASSW {
            lpfnWndProc: Some(wndproc),
            hInstance: hinst.into(),
            lpszClassName: CLASS_NAME,
            ..Default::default()
        };
        RegisterClassW(&wc);

        // 一般視窗但從不 ShowWindow，對齊 C# 隱藏 Form 的做法。
        // 不用 HWND_MESSAGE：message-only window 搭剪貼簿監聽有已知的雷。
        let hwnd = CreateWindowExW(
            WINDOW_EX_STYLE(0),
            CLASS_NAME,
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
        )
        .expect("CreateWindowExW");

        let cx = GetSystemMetrics(SM_CXSMICON);
        let cy = GetSystemMetrics(SM_CYSMICON);
        ICON_ON.set(load_icon(hinst, IDI_ON, cx, cy));
        ICON_OFF.set(load_icon(hinst, IDI_OFF, cx, cy));

        WM_TASKBAR_CREATED.set(RegisterWindowMessageW(w!("TaskbarCreated")));
        FOLDERS_ONLY.set(load_folders_only());
        RATE_LIMIT.set(load_rate_limit());
        let _ = AddClipboardFormatListener(hwnd);
        tray_add(hwnd);

        if is_startup_enabled() {
            // 把舊格式（沒有 --autostart）的開機設定升級成新格式；已是新格式則等於重寫同一個值
            set_startup(true);
        }
        // 開機自動啟動不跳通知，手動雙擊才跳
        if !std::env::args().any(|a| a == AUTOSTART_ARG) {
            show_balloon(
                hwnd,
                &format!("Pathie v{VERSION} 已啟動"),
                "複製檔案或資料夾路徑即可自動開啟。圖示在系統匣，右鍵開啟選單。",
            );
        }

        let mut msg = MSG::default();
        // > 0：GetMessageW 出錯時回 -1，用 as_bool() 會變成無窮迴圈
        while GetMessageW(&mut msg, HWND::default(), 0, 0).0 > 0 {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_CLIPBOARDUPDATE => {
            if MONITORING.get() {
                handle_clipboard(hwnd);
            }
            LRESULT(0)
        }
        WM_TRAY => {
            match lp.0 as u32 {
                WM_LBUTTONUP => toggle_monitoring(hwnd),
                WM_RBUTTONUP => show_menu(hwnd),
                _ => {}
            }
            LRESULT(0)
        }
        WM_COMMAND => {
            match (wp.0 & 0xFFFF) as usize {
                ID_STARTUP => set_startup(!is_startup_enabled()),
                ID_TOGGLE => toggle_monitoring(hwnd),
                ID_FOLDERS_ONLY => {
                    let on = !FOLDERS_ONLY.get();
                    FOLDERS_ONLY.set(on);
                    save_folders_only(on);
                }
                id @ ID_RATE_BASE.. => {
                    if let Some(&n) = RATE_OPTIONS.get(id - ID_RATE_BASE) {
                        RATE_LIMIT.set(n);
                        save_rate_limit(n);
                    }
                }
                ID_ABOUT => show_about(hwnd),
                ID_EXIT => {
                    let _ = DestroyWindow(hwnd);
                }
                _ => {}
            }
            LRESULT(0)
        }
        WM_DESTROY => {
            let _ = Shell_NotifyIconW(NIM_DELETE, &tray_data(hwnd));
            let _ = RemoveClipboardFormatListener(hwnd);
            PostQuitMessage(0);
            LRESULT(0)
        }
        WM_ALREADY_RUNNING => {
            let state = if MONITORING.get() {
                "監控中。"
            } else {
                "目前暫停監控，左鍵點圖示可恢復。"
            };
            show_balloon(
                hwnd,
                &format!("Pathie v{VERSION} 已在執行中"),
                &format!("圖示在系統匣，右鍵開啟選單。{state}"),
            );
            LRESULT(0)
        }
        m if m != 0 && m == WM_TASKBAR_CREATED.get() => {
            tray_add(hwnd);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}

// --- 剪貼簿 ---------------------------------------------------------------

/// 滑動視窗計數：回傳 true 表示已達上限（此次不計入）；否則記錄這次並回傳 false。
fn rate_exceeded(opens: &mut VecDeque<Instant>, now: Instant, limit: u32) -> bool {
    while opens
        .front()
        .is_some_and(|&t| now.duration_since(t) >= RATE_WINDOW)
    {
        opens.pop_front();
    }
    if opens.len() >= limit as usize {
        return true;
    }
    opens.push_back(now);
    false
}

fn handle_clipboard(hwnd: HWND) {
    // clipboard-win 內建重試：WM_CLIPBOARDUPDATE 當下來源程式往往還佔著剪貼簿，
    // 直接 OpenClipboard 會拿到 ACCESS_DENIED
    let Ok(text) = clipboard_win::get_clipboard_string() else {
        return;
    };
    let Some(path) = normalize(&text) else {
        return;
    };
    if !is_valid(&path, FOLDERS_ONLY.get()) {
        return;
    }
    let limit = RATE_LIMIT.get();
    if OPENS.with_borrow_mut(|o| rate_exceeded(o, Instant::now(), limit)) {
        // 自動暫停而非只丟棄：洪水持續時每次都通知本身就是干擾
        unsafe {
            toggle_monitoring(hwnd);
            show_balloon(
                hwnd,
                "Pathie 已暫停監控",
                &format!("偵測到每分鐘超過 {limit} 次的開啟請求，可能有程式在大量複製路徑。左鍵點圖示可恢復。"),
            );
        }
        return;
    }
    shell_open(&path.to_string_lossy());
}

fn normalize(raw: &str) -> Option<PathBuf> {
    // 多行（如在檔案總管選多個檔案「複製為路徑」）只取第一個非空行；
    // 引號含直引號與彎引號，且引號內外的空白一併去掉
    let strip = |c: char| c.is_whitespace() || matches!(c, '"' | '“' | '”');
    let t = raw
        .lines()
        .map(|l| l.trim_matches(strip))
        .find(|l| !l.is_empty())
        .unwrap_or("");
    if t.is_empty() {
        return None;
    }

    // get(..7) 而非 &t[..7]：剪貼簿是任意文字，切在多位元組字元中間會 panic
    let t = match t.get(..7) {
        Some(p) if p.eq_ignore_ascii_case("file://") => {
            url_to_path(t).unwrap_or_else(|| t.to_owned())
        }
        _ => t.to_owned(),
    };

    let t = expand_env(&t.replace('/', "\\"));
    let t = t.trim();
    (!t.is_empty()).then(|| PathBuf::from(t))
}

fn is_valid(p: &Path, folders_only: bool) -> bool {
    // 只接受絕對路徑，避免裸檔名（如 gpedit.msc）靠工作目錄湊巧命中系統指令。
    // Rust 的 is_absolute 比 C# 的 Path.IsPathRooted 更嚴格，連 `\Windows\...`
    // 和 `C:foo` 這種「有 root 但非絕對」的寫法也一併擋掉。
    //
    // folders_only：看檔案系統實際狀態（is_dir）而非副檔名——資料夾可以叫 v1.2，
    // 檔案可以叫 LICENSE。檔案（含 .exe/.lnk）一律不開，避免繞過 Windows 的執行警告。
    p.is_absolute() && if folders_only { p.is_dir() } else { p.exists() }
}

/// 用預設程式開啟路徑或網址（等同在檔案總管雙擊）
fn shell_open(target: &str) {
    let wide: Vec<u16> = target.encode_utf16().chain(Some(0)).collect();
    unsafe {
        ShellExecuteW(
            HWND::default(),
            PCWSTR::null(),
            PCWSTR(wide.as_ptr()),
            PCWSTR::null(),
            PCWSTR::null(),
            SW_SHOWNORMAL,
        );
    }
}

/// `new Uri(x).LocalPath` 的對應 API：percent-decoding 與 UNC 都交給 shlwapi，
/// 省掉一個 url crate。
fn url_to_path(url: &str) -> Option<String> {
    let src: Vec<u16> = url.encode_utf16().chain(Some(0)).collect();
    let mut buf = vec![0u16; 4096];
    let mut len = buf.len() as u32;
    unsafe {
        PathCreateFromUrlW(PCWSTR(src.as_ptr()), PWSTR(buf.as_mut_ptr()), &mut len, 0).ok()?;
    }
    Some(String::from_utf16_lossy(&buf[..len as usize]))
}

fn expand_env(s: &str) -> String {
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

// --- 系統匣 ---------------------------------------------------------------

fn tray_data(hwnd: HWND) -> NOTIFYICONDATAW {
    NOTIFYICONDATAW {
        cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: hwnd,
        uID: TRAY_UID,
        ..Default::default()
    }
}

unsafe fn tray_add(hwnd: HWND) {
    let mut nid = tray_data(hwnd);
    nid.uFlags = NIF_ICON | NIF_MESSAGE | NIF_TIP;
    nid.uCallbackMessage = WM_TRAY;
    nid.hIcon = if MONITORING.get() {
        ICON_ON.get()
    } else {
        ICON_OFF.get()
    };
    fill_wide(&mut nid.szTip, &format!("Pathie 路徑自動開啟 v{VERSION}"));
    let _ = Shell_NotifyIconW(NIM_ADD, &nid);
}

/// 系統匣彈出通知（Windows 10 會轉成右下角的通知卡片）
unsafe fn show_balloon(hwnd: HWND, title: &str, text: &str) {
    let mut nid = tray_data(hwnd);
    nid.uFlags = NIF_INFO;
    nid.dwInfoFlags = NIIF_INFO;
    fill_wide(&mut nid.szInfoTitle, title);
    fill_wide(&mut nid.szInfo, text);
    let _ = Shell_NotifyIconW(NIM_MODIFY, &nid);
}

/// 寫入固定長度的 UTF-16 欄位，超長就截斷並保留結尾的 null
fn fill_wide(dst: &mut [u16], s: &str) {
    let max = dst.len() - 1;
    let mut n = 0;
    for (d, c) in dst.iter_mut().zip(s.encode_utf16().take(max)) {
        *d = c;
        n += 1;
    }
    dst[n] = 0;
}

// ponytail: on/off 只進 32x32，16x16 讓 Windows 縮。兩個尺寸是各自獨立的 .ico
// 檔，不合併成單一多尺寸 .ico 就沒辦法讓 LoadImageW 自動選（C# 版同樣是拿
// 32x32 交給 WinForms 縮，行為對等）。要更銳利就把兩個尺寸合併成一個 .ico。
unsafe fn load_icon(hinst: HMODULE, id: u16, cx: i32, cy: i32) -> HICON {
    LoadImageW(
        hinst,
        PCWSTR(id as usize as *const u16),
        IMAGE_ICON,
        cx,
        cy,
        LR_DEFAULTCOLOR,
    )
    .map(|h| HICON(h.0))
    .unwrap_or_else(|_| LoadIconW(None, IDI_APPLICATION).unwrap_or_default())
}

unsafe fn toggle_monitoring(hwnd: HWND) {
    let on = !MONITORING.get();
    MONITORING.set(on);
    if on {
        // 恢復後重新計數，否則剛恢復又立刻觸發
        OPENS.with_borrow_mut(VecDeque::clear);
    }

    let mut nid = tray_data(hwnd);
    nid.uFlags = NIF_ICON;
    nid.hIcon = if on { ICON_ON.get() } else { ICON_OFF.get() };
    let _ = Shell_NotifyIconW(NIM_MODIFY, &nid);
}

unsafe fn show_menu(hwnd: HWND) {
    let Ok(menu) = CreatePopupMenu() else { return };

    let startup = if is_startup_enabled() {
        MF_CHECKED
    } else {
        MF_UNCHECKED
    };
    let _ = AppendMenuW(menu, MF_STRING | startup, ID_STARTUP, w!("開機自動啟動"));
    let toggle = if MONITORING.get() {
        w!("暫停監控")
    } else {
        w!("恢復監控")
    };
    let _ = AppendMenuW(menu, MF_STRING, ID_TOGGLE, toggle);
    let folders = if FOLDERS_ONLY.get() {
        MF_CHECKED
    } else {
        MF_UNCHECKED
    };
    let _ = AppendMenuW(menu, MF_STRING | folders, ID_FOLDERS_ONLY, w!("僅開啟資料夾"));
    if let Ok(sub) = CreatePopupMenu() {
        let cur = RATE_LIMIT.get();
        for (i, n) in RATE_OPTIONS.iter().enumerate() {
            let label: Vec<u16> = format!("{n} 次\0").encode_utf16().collect();
            let check = if *n == cur { MF_CHECKED } else { MF_UNCHECKED };
            let _ = AppendMenuW(sub, MF_STRING | check, ID_RATE_BASE + i, PCWSTR(label.as_ptr()));
        }
        // 子選單隨父選單一起 DestroyMenu
        let _ = AppendMenuW(menu, MF_POPUP, sub.0 as usize, w!("每分鐘開啟上限"));
    }
    let _ = AppendMenuW(menu, MF_SEPARATOR, 0, PCWSTR::null());
    let _ = AppendMenuW(menu, MF_STRING, ID_ABOUT, w!("關於"));
    let _ = AppendMenuW(menu, MF_STRING, ID_EXIT, w!("退出"));

    let mut pt = POINT::default();
    let _ = GetCursorPos(&mut pt);
    // 先搶前景、結束後補一則訊息，少了這兩步選單點到外面不會消失（經典 Win32 陷阱）
    let _ = SetForegroundWindow(hwnd);
    let _ = TrackPopupMenu(menu, TPM_RIGHTBUTTON, pt.x, pt.y, 0, hwnd, None);
    let _ = PostMessageW(hwnd, WM_NULL, WPARAM(0), LPARAM(0));
    let _ = DestroyMenu(menu);
}

// MessageBox 的文字不能點，所以用「是/否」按鈕代替超連結
unsafe fn show_about(hwnd: HWND) {
    let text = format!(
        "Pathie v{}\n作者：{}\n{}\n\n要開啟 GitHub 頁面嗎？\0",
        VERSION,
        env!("CARGO_PKG_AUTHORS"),
        env!("CARGO_PKG_REPOSITORY"),
    );
    let text: Vec<u16> = text.encode_utf16().collect();
    let answer = MessageBoxW(
        hwnd,
        PCWSTR(text.as_ptr()),
        w!("關於 Pathie"),
        MB_YESNO | MB_ICONINFORMATION | MB_SETFOREGROUND,
    );
    if answer == IDYES {
        shell_open(env!("CARGO_PKG_REPOSITORY"));
    }
}

// --- 開機自動啟動 ---------------------------------------------------------

fn exe_path() -> String {
    std::env::current_exe()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_default()
}

fn is_startup_enabled() -> bool {
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

fn set_startup(enable: bool) {
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

fn load_folders_only() -> bool {
    RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey(SETTINGS_KEY)
        .and_then(|k| k.get_value::<u32, _>(FOLDERS_ONLY_VALUE))
        .map_or(true, |v| v != 0)
}

fn save_folders_only(on: bool) {
    if let Ok((key, _)) = RegKey::predef(HKEY_CURRENT_USER).create_subkey(SETTINGS_KEY) {
        let _ = key.set_value(FOLDERS_ONLY_VALUE, &u32::from(on));
    }
}

fn load_rate_limit() -> u32 {
    RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey(SETTINGS_KEY)
        .and_then(|k| k.get_value::<u32, _>(RATE_LIMIT_VALUE))
        .ok()
        .filter(|n| RATE_OPTIONS.contains(n))
        .unwrap_or(RATE_DEFAULT)
}

fn save_rate_limit(n: u32) {
    if let Ok((key, _)) = RegKey::predef(HKEY_CURRENT_USER).create_subkey(SETTINGS_KEY) {
        let _ = key.set_value(RATE_LIMIT_VALUE, &n);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rate_limit_slides_over_window() {
        let t0 = Instant::now();
        let mut q = VecDeque::new();
        for i in 0..3 {
            assert!(!rate_exceeded(&mut q, t0 + Duration::from_secs(i), 3));
        }
        // 視窗內第 4 次被擋，且不計入
        assert!(rate_exceeded(&mut q, t0 + Duration::from_secs(10), 3));
        assert_eq!(q.len(), 3);
        // 最早那筆滿 60 秒後滑出，空出一格
        assert!(!rate_exceeded(&mut q, t0 + Duration::from_secs(60), 3));
        assert!(rate_exceeded(&mut q, t0 + Duration::from_secs(60), 3));
    }

    #[test]
    fn rejects_non_paths() {
        // commit b4b5317 要擋的就是這些：裸檔名、相對路徑、有 root 但非絕對
        let cases = [
            "gpedit.msc",
            "..\\Windows",
            "hello world",
            "",
            "   ",
            "\\Windows\\System32",
            "C:notrooted",
        ];
        for s in cases {
            let rejected = normalize(s).map_or(true, |p| !is_valid(&p, false));
            assert!(rejected, "應拒絕: {s:?}");
        }
    }

    #[test]
    fn multibyte_input_does_not_panic() {
        // 剪貼簿是任意文字；前綴比對若用 &t[..7] 會切在中文字中間 panic
        for s in ["複製一段中文", "檔", "路徑自動開啟", "日本語のテキスト"] {
            let _ = normalize(s);
        }
    }

    #[test]
    fn accepts_real_path_in_every_form() {
        let dir = std::env::temp_dir().join("clipboard_rs_test 空格");
        std::fs::create_dir_all(&dir).unwrap();
        let s = dir.to_string_lossy().into_owned();
        let slashed = s.replace('\\', "/");

        let forms = [
            s.clone(),
            format!("  {s}  "),
            format!("\"{s}\""),
            format!("“{s}”"),
            format!("\" {s} \""),
            format!("\"{s}\"\r\n\"{s}x\""),
            slashed.clone(),
            format!("file:///{}", slashed.replace(' ', "%20")),
        ];
        for f in &forms {
            let p = normalize(f).unwrap_or_else(|| panic!("normalize 回傳 None: {f:?}"));
            assert!(is_valid(&p, false), "應接受: {f:?} -> {p:?}");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

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

    #[test]
    fn fill_wide_truncates_and_keeps_null() {
        let mut buf = [0xFFFFu16; 4];
        fill_wide(&mut buf, "路徑自動開啟");
        assert_eq!(String::from_utf16_lossy(&buf[..3]), "路徑自");
        assert_eq!(buf[3], 0, "截斷後最後一格必須是 null");

        let mut buf = [0xFFFFu16; 8];
        fill_wide(&mut buf, "ab");
        assert_eq!(&buf[..3], &[b'a' as u16, b'b' as u16, 0]);
    }

    #[test]
    fn expands_environment_variables() {
        let p = normalize("%TEMP%").expect("%TEMP% 應正規化成功");
        assert!(!p.to_string_lossy().contains('%'), "未展開: {p:?}");
        assert!(is_valid(&p, false), "%TEMP% 應展開成有效路徑: {p:?}");
    }

    #[test]
    fn folders_only_blocks_files_by_real_type_not_extension() {
        let base = std::env::temp_dir().join("clipboard_rs_folders_only");
        let dotted = base.join("v1.2"); // 資料夾但有「副檔名」
        let exe = base.join("a.exe");
        let bare = base.join("LICENSE"); // 檔案但沒副檔名
        std::fs::create_dir_all(&dotted).unwrap();
        std::fs::write(&exe, b"").unwrap();
        std::fs::write(&bare, b"").unwrap();

        assert!(is_valid(&base, true));
        assert!(is_valid(&dotted, true));
        assert!(!is_valid(&exe, true));
        assert!(!is_valid(&bare, true));
        // 關閉開關時維持原行為
        assert!(is_valid(&exe, false));
        let _ = std::fs::remove_dir_all(&base);
    }
}
