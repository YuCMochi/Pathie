// Pathie：常駐系統匣，偵測剪貼簿中的有效路徑並自動開啟。
//
// cfg_attr 是必要的：直接寫 windows_subsystem 會連 `cargo test` 的測試執行檔
// 一起變成 windows subsystem，測試輸出就看不到了。
#![cfg_attr(not(test), windows_subsystem = "windows")]

mod logic;
mod platform;

use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::time::Instant;

use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{
    GetLastError, ERROR_ALREADY_EXISTS, HMODULE, HWND, LPARAM, LRESULT, POINT, WPARAM,
};
use windows::Win32::System::Com::{CoInitializeEx, COINIT_APARTMENTTHREADED};
use windows::Win32::System::DataExchange::{
    AddClipboardFormatListener, RemoveClipboardFormatListener,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::CreateMutexW;
use windows::Win32::UI::Shell::{
    Shell_NotifyIconW, NIF_ICON, NIF_INFO, NIF_MESSAGE, NIF_TIP,
    NIIF_INFO, NIM_ADD, NIM_DELETE, NIM_MODIFY, NOTIFYICONDATAW,
};
use windows::Win32::UI::WindowsAndMessaging::*;
use winreg::enums::HKEY_CURRENT_USER;
use winreg::RegKey;

use logic::*;
use platform::*;

// 登錄檔位置與值名：開機自動啟動 (Run 值) 與使用者設定
// 「檔案行為」的存放位置（DWORD：0 不處理／1 選取／2 直接開啟，缺值視為 1）
const SETTINGS_KEY: &str = r"Software\Pathie";
const FILE_ACTION_VALUE: &str = "FileAction";
// 是否處理 UNC 網路路徑（DWORD，缺值視為關閉）
const ALLOW_UNC_VALUE: &str = "AllowUnc";
// 是否自動開啟網址（DWORD，缺值視為開啟）
const OPEN_URL_VALUE: &str = "OpenUrl";
// 複製時前景視窗是瀏覽器就略過網址（DWORD，缺值視為開啟）
const SKIP_BROWSER_VALUE: &str = "SkipUrlFromBrowser";
// 防濫用：每分鐘最多自動開啟幾次，超過就自動暫停監控並通知
const RATE_LIMIT_VALUE: &str = "RateLimitPerMinute";

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
const ID_UNC: usize = 5;
const ID_OPEN_URL: usize = 6;
const ID_SKIP_BROWSER: usize = 7;
// 頻率上限子選單：ID_RATE_BASE + RATE_OPTIONS 的索引
const ID_RATE_BASE: usize = 100;
// 檔案行為子選單：ID_FILE_BASE + FileAction 的值
const ID_FILE_BASE: usize = 200;

// 對應 app.rc 的資源 ID
const IDI_ON: u16 = 2;
const IDI_OFF: u16 = 3;

thread_local! {
    static MONITORING: Cell<bool> = const { Cell::new(true) };
    static ALLOW_UNC: Cell<bool> = const { Cell::new(false) };
    static OPEN_URL: Cell<bool> = const { Cell::new(true) };
    static SKIP_BROWSER: Cell<bool> = const { Cell::new(true) };
    static FILE_ACTION: Cell<FileAction> = const { Cell::new(FileAction::Reveal) };
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

        // SHOpenFolderAndSelectItems 需要 COM；STA 對應有訊息迴圈的 UI 執行緒
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let hinst: HMODULE =GetModuleHandleW(None).expect("GetModuleHandleW");
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
        FILE_ACTION.set(load_file_action());
        ALLOW_UNC.set(load_allow_unc());
        OPEN_URL.set(load_bool(OPEN_URL_VALUE, true));
        SKIP_BROWSER.set(load_bool(SKIP_BROWSER_VALUE, true));
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
                ID_UNC => {
                    let on = !ALLOW_UNC.get();
                    ALLOW_UNC.set(on);
                    save_allow_unc(on);
                }
                ID_OPEN_URL => {
                    let on = !OPEN_URL.get();
                    OPEN_URL.set(on);
                    save_bool(OPEN_URL_VALUE, on);
                }
                ID_SKIP_BROWSER => {
                    let on = !SKIP_BROWSER.get();
                    SKIP_BROWSER.set(on);
                    save_bool(SKIP_BROWSER_VALUE, on);
                }
                id @ ID_FILE_BASE.. => {
                    if let Some(a) = FileAction::from_u32((id - ID_FILE_BASE) as u32) {
                        FILE_ACTION.set(a);
                        save_file_action(a);
                    }
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


fn handle_clipboard(hwnd: HWND) {
    // clipboard-win 內建重試：WM_CLIPBOARDUPDATE 當下來源程式往往還佔著剪貼簿，
    // 直接 OpenClipboard 會拿到 ACCESS_DENIED
    let Ok(text) = clipboard_win::get_clipboard_string() else {
        return;
    };
    // 網址直接開啟（已限制為 http/https），其餘照路徑規則處理。
    // 網址若被開關擋下就直接結束，不能落入路徑分支（www.x 之類可能被當成檔名）
    if let Some(url) = parse_url(&text) {
        if !OPEN_URL.get() || (SKIP_BROWSER.get() && foreground_is_browser()) {
            return;
        }
        return act_on(hwnd, url, Act::Open);
    }
    let Some(path) = normalize(&text) else {
        return;
    };
    if is_unc(&path) && !ALLOW_UNC.get() {
        return;
    }
    if let Some(act) = decide(&path, FILE_ACTION.get()) {
        act_on(hwnd, path.to_string_lossy().into_owned(), act);
    }
}

/// 頻率檢查後執行動作；網址與路徑共用同一個計數
fn act_on(hwnd: HWND, target: String, act: Act) {
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
    match act {
        Act::Open => shell_open(&target),
        Act::Reveal => reveal(&target),
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
    if let Ok(sub) = CreatePopupMenu() {
        let cur = FILE_ACTION.get();
        for (a, label) in [
            (FileAction::Ignore, w!("不處理")),
            (FileAction::Reveal, w!("在資料夾中選取")),
            (FileAction::Open, w!("直接開啟（執行檔除外）")),
        ] {
            let check = if a == cur { MF_CHECKED } else { MF_UNCHECKED };
            let _ = AppendMenuW(sub, MF_STRING | check, ID_FILE_BASE + a as usize, label);
        }
        let _ = AppendMenuW(menu, MF_POPUP, sub.0 as usize, w!("複製檔案時"));
    }
    let unc = if ALLOW_UNC.get() { MF_CHECKED } else { MF_UNCHECKED };
    let _ = AppendMenuW(menu, MF_STRING | unc, ID_UNC, w!("處理網路路徑 (\\\\host\\share)"));
    let check = |on: bool| if on { MF_CHECKED } else { MF_UNCHECKED };
    let _ = AppendMenuW(menu, MF_STRING | check(OPEN_URL.get()), ID_OPEN_URL, w!("自動開啟網址"));
    let _ = AppendMenuW(
        menu,
        MF_STRING | check(SKIP_BROWSER.get()),
        ID_SKIP_BROWSER,
        w!("從瀏覽器複製的網址不開啟"),
    );
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


fn load_file_action() -> FileAction {
    RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey(SETTINGS_KEY)
        .and_then(|k| k.get_value::<u32, _>(FILE_ACTION_VALUE))
        .ok()
        .and_then(FileAction::from_u32)
        .unwrap_or(FileAction::Reveal)
}

fn load_bool(name: &str, default: bool) -> bool {
    RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey(SETTINGS_KEY)
        .and_then(|k| k.get_value::<u32, _>(name))
        .map_or(default, |v| v != 0)
}

fn save_bool(name: &str, on: bool) {
    if let Ok((key, _)) = RegKey::predef(HKEY_CURRENT_USER).create_subkey(SETTINGS_KEY) {
        let _ = key.set_value(name, &u32::from(on));
    }
}

fn load_allow_unc() -> bool {
    load_bool(ALLOW_UNC_VALUE, false)
}

fn save_allow_unc(on: bool) {
    save_bool(ALLOW_UNC_VALUE, on);
}

fn save_file_action(a: FileAction) {
    if let Ok((key, _)) = RegKey::predef(HKEY_CURRENT_USER).create_subkey(SETTINGS_KEY) {
        let _ = key.set_value(FILE_ACTION_VALUE, &(a as u32));
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
    fn fill_wide_truncates_and_keeps_null() {
        let mut buf = [0xFFFFu16; 4];
        fill_wide(&mut buf, "路徑自動開啟");
        assert_eq!(String::from_utf16_lossy(&buf[..3]), "路徑自");
        assert_eq!(buf[3], 0, "截斷後最後一格必須是 null");

        let mut buf = [0xFFFFu16; 8];
        fill_wide(&mut buf, "ab");
        assert_eq!(&buf[..3], &[b'a' as u16, b'b' as u16, 0]);
    }
}
