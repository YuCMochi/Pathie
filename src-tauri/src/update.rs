// 檢查更新與一鍵更新：只在使用者於設定視窗按下按鈕時才連網，啟動時不檢查也不通知。
//
// 流程：讀 GitHub Releases → 挑出新版（logic::pick_update）→ 下載 Pathie-Setup.exe
// → 比對 GitHub 提供的 SHA-256 → 以靜默模式執行安裝程式，Pathie 自己結束，
// 安裝程式裝完會再把 Pathie 開回來（見 installer.iss 的 /UPDATE）。

use std::path::PathBuf;

use sha2::{Digest, Sha256};

use crate::logic::{pick_update, Update};
use crate::platform;

/// 傳給安裝程式的參數：不顯示任何視窗、不重開機，/UPDATE 讓它等 Pathie 結束並在裝完後重新啟動
const INSTALLER_ARGS: &str = "/VERYSILENT /SUPPRESSMSGBOXES /NORESTART /UPDATE";

fn temp_file(name: &str) -> PathBuf {
    std::env::temp_dir().join(name)
}

/// 有新版回傳 Some；已是最新版回傳 None
pub fn check() -> Result<Option<Update>, String> {
    let repo = env!("CARGO_PKG_REPOSITORY").replacen("https://github.com/", "https://api.github.com/repos/", 1);
    let path = temp_file("Pathie-releases.json");
    platform::download(&format!("{repo}/releases?per_page=20"), &path).map_err(|e| format!("無法連線到 GitHub：{e}"))?;
    let body = std::fs::read(&path);
    let _ = std::fs::remove_file(&path);
    let json: serde_json::Value =
        serde_json::from_slice(&body.map_err(|e| e.to_string())?).map_err(|_| "GitHub 回應格式不正確".to_owned())?;
    Ok(pick_update(&json, env!("CARGO_PKG_VERSION")))
}

/// 下載並驗證安裝程式，驗證通過才執行。成功回傳後呼叫端要立刻結束程式。
pub fn install(update: &Update) -> Result<(), String> {
    let expected = update.sha256.as_deref().ok_or("這個版本缺少檢查碼，請到下載頁手動更新")?;
    let path = temp_file("Pathie-Update.exe");
    platform::download(&update.download_url, &path).map_err(|e| format!("下載失敗：{e}"))?;
    let actual = std::fs::read(&path).map(|b| hex(&Sha256::digest(&b)));
    if actual.as_deref().ok() != Some(expected) {
        let _ = std::fs::remove_file(&path);
        return Err("下載的檔案檢查碼不符，已取消更新".into());
    }
    platform::run_installer(&path, INSTALLER_ARGS);
    Ok(())
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_matches_known_sha256() {
        assert_eq!(hex(&Sha256::digest(b"abc")), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
    }
}
