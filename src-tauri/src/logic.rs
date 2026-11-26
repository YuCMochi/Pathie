// 純邏輯：路徑/網址的判斷與頻率限制，不碰 Win32 (除了經 platform 展開環境變數與 file:// 轉換)。

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crate::platform::{expand_env, url_to_path};

pub const BROWSERS: [&str; 9] = ["chrome", "msedge", "firefox", "brave", "opera", "vivaldi", "zen", "arc", "dia"];

pub const RATE_WINDOW: Duration = Duration::from_secs(60);
pub const RATE_OPTIONS: [u32; 4] = [10, 20, 50, 100];
pub const RATE_DEFAULT: u32 = 20;

/// 滑動視窗計數：回傳 true 表示已達上限（此次不計入）；否則記錄這次並回傳 false。
pub fn rate_exceeded(opens: &mut VecDeque<Instant>, now: Instant, limit: u32) -> bool {
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

/// 程序名稱（不含副檔名、不分大小寫）是否為已知瀏覽器
pub fn is_browser_exe(path: &str) -> bool {
    let stem = Path::new(path).file_stem().and_then(|s| s.to_str()).unwrap_or("");
    BROWSERS.iter().any(|b| stem.eq_ignore_ascii_case(b))
}

/// 多行（如在檔案總管選多個檔案「複製為路徑」）只取第一個非空行；
/// 引號含直引號與彎引號，且引號內外的空白一併去掉
pub fn first_line(raw: &str) -> &str {
    let strip = |c: char| c.is_whitespace() || matches!(c, '"' | '“' | '”');
    raw.lines()
        .map(|l| l.trim_matches(strip))
        .find(|l| !l.is_empty())
        .unwrap_or("")
}

/// 只認 `http(s)://` 開頭，或 `www.` 開頭（補上 https://）。
/// 其他 scheme（ms-msdt:、search-ms: 等）與裸網域（main.py、foo.rs 的副檔名恰好是真 TLD）一律不處理。
pub fn parse_url(raw: &str) -> Option<String> {
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

pub fn normalize(raw: &str) -> Option<PathBuf> {
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
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FileAction {
    Ignore = 0,
    #[default]
    Reveal = 1,
    Open = 2,
}

impl FileAction {
    pub fn from_u32(n: u32) -> Option<Self> {
        match n {
            0 => Some(Self::Ignore),
            1 => Some(Self::Reveal),
            2 => Some(Self::Open),
            _ => None,
        }
    }
}

pub enum Act {
    Open,
    Reveal,
}

/// 這些副檔名直接開啟等於執行程式碼，ShellExecute 對本機檔案又不會跳警告，
/// 所以即使使用者選「直接開啟」也一律退回「選取」。
// ponytail: 黑名單會漏（如新的腳本類型），改白名單若誤擋太多再說
pub const DANGEROUS_EXTS: &[&str] = &[
    "exe", "bat", "cmd", "com", "scr", "msi", "msp", "ps1", "vbs", "vbe", "js", "jse", "wsf",
    "wsh", "hta", "lnk", "url", "reg", "cpl", "jar", "dll", "pif", "gadget", "appref-ms",
];

pub fn is_dangerous(p: &Path) -> bool {
    p.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| DANGEROUS_EXTS.iter().any(|d| e.eq_ignore_ascii_case(d)))
}

/// `\\host\share`、`\\?\...` 等 `\\` 開頭的路徑。
/// 光是 exists()/is_dir() 檢查就會主動連 SMB，可能把 NTLM hash 送給對方，所以要在碰檔案系統前擋掉。
pub fn is_unc(p: &Path) -> bool {
    p.as_os_str().to_string_lossy().starts_with(r"\\")
}

/// 決定要對這個路徑做什麼；None 表示不處理。
///
/// 只接受絕對路徑，避免裸檔名（如 gpedit.msc）靠工作目錄湊巧命中系統指令。
/// Rust 的 is_absolute 比 C# 的 Path.IsPathRooted 更嚴格，連 `\Windows\...`
/// 和 `C:foo` 這種「有 root 但非絕對」的寫法也一併擋掉。
/// 資料夾/檔案看檔案系統實際狀態（is_dir）而非副檔名——資料夾可以叫 v1.2。
pub fn decide(p: &Path, files: FileAction) -> Option<Act> {
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
