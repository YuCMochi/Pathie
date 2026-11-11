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
    CloseHandle, GetLastError, ERROR_ALREADY_EXISTS, HMODULE, HWND, LPARAM, LRESULT, POINT, WPARAM,
};
use windows::Win32::System::Com::{CoInitializeEx, COINIT_APARTMENTTHREADED};
use windows::Win32::System::DataExchange::{
    AddClipboardFormatListener, RemoveClipboardFormatListener,
};
use windows::Win32::System::Environment::ExpandEnvironmentStringsW;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::{
    CreateMutexW, OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32,
    PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::Shell::{
    ILCreateFromPathW, ILFree, PathCreateFromUrlW, SHOpenFolderAndSelectItems, ShellExecuteW,
    Shell_NotifyIconW, NIF_ICON, NIF_INFO, NIF_MESSAGE, NIF_TIP,
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
// 「檔案行為」的存放位置（DWORD：0 不處理／1 選取／2 直接開啟，缺值視為 1）
const SETTINGS_KEY: &str = r"Software\Pathie";
const FILE_ACTION_VALUE: &str = "FileAction";
// 是否處理 UNC 網路路徑（DWORD，缺值視為關閉）
const ALLOW_UNC_VALUE: &str = "AllowUnc";
// 是否自動開啟網址（DWORD，缺值視為開啟）
const OPEN_URL_VALUE: &str = "OpenUrl";
// 複製時前景視窗是瀏覽器就略過網址（DWORD，缺值視為開啟）
const SKIP_BROWSER_VALUE: &str = "SkipUrlFromBrowser";
const BROWSERS: [&str; 9] = ["chrome", "msedge", "firefox", "brave", "opera", "vivaldi", "zen", "arc", "dia"];
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

/// 程序名稱（不含副檔名、不分大小寫）是否為已知瀏覽器
fn is_browser_exe(path: &str) -> bool {
    let stem = Path::new(path).file_stem().and_then(|s| s.to_str()).unwrap_or("");
    BROWSERS.iter().any(|b| stem.eq_ignore_ascii_case(b))
}

/// 目前前景視窗是否屬於瀏覽器；查不到（權限不足等）一律視為否，維持原行為
fn foreground_is_browser() -> bool {
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

/// 多行（如在檔案總管選多個檔案「複製為路徑」）只取第一個非空行；
/// 引號含直引號與彎引號，且引號內外的空白一併去掉
fn first_line(raw: &str) -> &str {
    let strip = |c: char| c.is_whitespace() || matches!(c, '"' | '“' | '”');
    raw.lines()
        .map(|l| l.trim_matches(strip))
        .find(|l| !l.is_empty())
        .unwrap_or("")
}

/// 只認 `http(s)://` 開頭，或 `www.` 開頭（補上 https://）。
/// 其他 scheme（ms-msdt:、search-ms: 等）與裸網域（main.py、foo.rs 的副檔名恰好是真 TLD）一律不處理。
fn parse_url(raw: &str) -> Option<String> {
    let t = first_line(raw);
    // 網址不含空白或控制字元；get(..n) 避免切在多位元組字元中間
    if t.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return None;
    }
    let lower = |n: usize| t.get(..n).map(str::to_ascii_lowercase);
    let (rest, has_scheme) = if lower(8).as_deref() == Some("https://") {
        (&t[8..], true)
    } else if lower(7).as_deref() == Some("http://") {
        (&t[7..], true)
    } else if lower(4).as_deref() == Some("www.") {
        (&t[4..], false)
    } else {
        return None;
    };
    // 主機名至少要有一個字元，且不能以 . 或 / 起頭（擋掉 "http://" "www." "https:///x" 之類）
    if rest.is_empty() || rest.starts_with(['.', '/']) {
        return None;
    }
    Some(if has_scheme { t.to_owned() } else { format!("https://{t}") })
}

fn normalize(raw: &str) -> Option<PathBuf> {
    let t = first_line(raw);
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

/// 使用者對「檔案」（非資料夾）的處理方式。資料夾一律直接開啟。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum FileAction {
    Ignore = 0,
    Reveal = 1,
    Open = 2,
}

impl FileAction {
    fn from_u32(n: u32) -> Option<Self> {
        match n {
            0 => Some(Self::Ignore),
            1 => Some(Self::Reveal),
            2 => Some(Self::Open),
            _ => None,
        }
    }
}

enum Act {
    Open,
    Reveal,
}

/// 這些副檔名直接開啟等於執行程式碼，ShellExecute 對本機檔案又不會跳警告，
/// 所以即使使用者選「直接開啟」也一律退回「選取」。
// ponytail: 黑名單會漏（如新的腳本類型），改白名單若誤擋太多再說
const DANGEROUS_EXTS: &[&str] = &[
    "exe", "bat", "cmd", "com", "scr", "msi", "msp", "ps1", "vbs", "vbe", "js", "jse", "wsf",
    "wsh", "hta", "lnk", "url", "reg", "cpl", "jar", "dll", "pif", "gadget", "appref-ms",
];

fn is_dangerous(p: &Path) -> bool {
    p.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| DANGEROUS_EXTS.iter().any(|d| e.eq_ignore_ascii_case(d)))
}

/// `\\host\share`、`\\?\...` 等 `\\` 開頭的路徑。
/// 光是 exists()/is_dir() 檢查就會主動連 SMB，可能把 NTLM hash 送給對方，所以要在碰檔案系統前擋掉。
fn is_unc(p: &Path) -> bool {
    p.as_os_str().to_string_lossy().starts_with(r"\\")
}

/// 決定要對這個路徑做什麼；None 表示不處理。
///
/// 只接受絕對路徑，避免裸檔名（如 gpedit.msc）靠工作目錄湊巧命中系統指令。
/// Rust 的 is_absolute 比 C# 的 Path.IsPathRooted 更嚴格，連 `\Windows\...`
/// 和 `C:foo` 這種「有 root 但非絕對」的寫法也一併擋掉。
/// 資料夾/檔案看檔案系統實際狀態（is_dir）而非副檔名——資料夾可以叫 v1.2。
fn decide(p: &Path, files: FileAction) -> Option<Act> {
    if !p.is_absolute() || !p.exists() {
        return None;
    }
    if p.is_dir() {
        return Some(Act::Open);
    }
    match files {
        FileAction::Ignore => None,
        FileAction::Open if !is_dangerous(p) => Some(Act::Open),
        _ => Some(Act::Reveal),
    }
}

/// 在檔案總管開啟所在資料夾並選取該檔案
///
/// 不用 `explorer /select`：它每次都開新視窗；SHOpenFolderAndSelectItems 會重用
/// 已開啟該資料夾的視窗（需要先 CoInitializeEx，見 main）。
fn reveal(path: &str) {
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
fn shell_open(target: &str) {
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
            let rejected = normalize(s).map_or(true, |p| decide(&p, FileAction::Open).is_none());
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
            assert!(decide(&p, FileAction::Reveal).is_some(), "應接受: {f:?} -> {p:?}");
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
        assert!(decide(&p, FileAction::Reveal).is_some(), "%TEMP% 應展開成有效路徑: {p:?}");
    }

    #[test]
    fn file_action_decides_by_real_type_and_blocks_dangerous_ext() {
        use FileAction::*;
        let base = std::env::temp_dir().join("clipboard_rs_file_action");
        let dotted = base.join("v1.2"); // 資料夾但有「副檔名」
        let exe = base.join("a.EXE");
        let doc = base.join("a.txt");
        let bare = base.join("LICENSE"); // 檔案但沒副檔名
        std::fs::create_dir_all(&dotted).unwrap();
        for f in [&exe, &doc, &bare] {
            std::fs::write(f, b"").unwrap();
        }
        let is = |p: &Path, a, open: bool| match decide(p, a) {
            Some(Act::Open) => open,
            Some(Act::Reveal) => !open,
            None => false,
        };

        // 資料夾：任何設定都直接開
        for a in [Ignore, Reveal, Open] {
            assert!(is(&base, a, true) && is(&dotted, a, true));
        }
        assert!(decide(&doc, Ignore).is_none());
        assert!(is(&doc, Reveal, false));
        assert!(is(&doc, Open, true));
        assert!(is(&bare, Open, true));
        // 危險副檔名（不分大小寫）即使選「直接開啟」也只選取
        assert!(is(&exe, Open, false));
        assert!(decide(&base.join("missing.txt"), Open).is_none());
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn unc_paths_are_detected_in_every_form() {
        for s in [
            r"\\host\share\a.txt",
            "//host/share/a.txt",
            r#""\\host\share""#,
            "file://host/share/a.txt",
            r"\\?\C:\Windows",
        ] {
            let p = normalize(s).unwrap_or_else(|| panic!("normalize 失敗: {s:?}"));
            assert!(is_unc(&p), "應視為 UNC: {s:?} -> {p:?}");
        }
        for s in [r"C:\Windows", "C:/Windows", "%TEMP%"] {
            assert!(!is_unc(&normalize(s).unwrap()), "不該是 UNC: {s:?}");
        }
    }

    #[test]
    fn browser_exe_matches_by_stem() {
        for p in [r"C:\Program Files\Google\Chrome\Application\chrome.exe", r"C:\x\MSEdge.EXE", "firefox.exe", "Zen.exe", "Arc.exe", "Dia.exe"] {
            assert!(is_browser_exe(p), "{p}");
        }
        for p in ["notepad.exe", r"C:\x\chrome-helper.exe", "", "explorer.exe"] {
            assert!(!is_browser_exe(p), "{p}");
        }
    }

    #[test]
    fn parse_url_accepts_only_http_and_www() {
        let ok = [
            ("https://example.com/a?b=1", "https://example.com/a?b=1"),
            ("HTTP://Example.com", "HTTP://Example.com"),
            ("  \"http://example.com\"  ", "http://example.com"),
            ("www.example.com/x", "https://www.example.com/x"),
            ("WWW.example.com", "https://WWW.example.com"),
            ("https://example.com\r\nsecond line", "https://example.com"),
        ];
        for (input, want) in ok {
            assert_eq!(parse_url(input).as_deref(), Some(want), "{input:?}");
        }
        let bad = [
            "main.py",
            "foo.rs",
            "example.com",
            "ms-msdt:/id PCWDiagnostic",
            "search-ms:query=x",
            "javascript:alert(1)",
            "file:///C:/Windows",
            "http://",
            "https:///x",
            "www.",
            "www.a b.com",
            "see https://example.com",
            "",
            "複製一段中文",
        ];
        for s in bad {
            assert!(parse_url(s).is_none(), "應拒絕: {s:?}");
        }
    }
}
