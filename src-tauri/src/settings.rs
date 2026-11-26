// 使用者設定：%APPDATA%\Pathie\settings.json。首次啟動沒有 JSON 時，從舊版登錄檔搬一次。

use std::fs;

use serde::{Deserialize, Serialize};

use crate::logic::{FileAction, RATE_DEFAULT, RATE_OPTIONS};
use crate::platform;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Settings {
    pub file_action: FileAction,
    pub allow_unc: bool,
    pub open_url: bool,
    pub skip_url_from_browser: bool,
    pub rate_limit_per_minute: u32,
    pub show_toast: bool,
    pub first_run_done: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            file_action: FileAction::Reveal,
            allow_unc: false,
            open_url: true,
            skip_url_from_browser: true,
            rate_limit_per_minute: RATE_DEFAULT,
            show_toast: true,
            first_run_done: false,
        }
    }
}

impl Settings {
    /// 手改 JSON 可能塞進不合法的值，退回預設而不是信任它
    pub fn sanitized(mut self) -> Self {
        if !RATE_OPTIONS.contains(&self.rate_limit_per_minute) {
            self.rate_limit_per_minute = RATE_DEFAULT;
        }
        self
    }
}

fn file() -> std::path::PathBuf {
    platform::config_dir().join("settings.json")
}

pub fn load() -> Settings {
    match fs::read_to_string(file()) {
        Ok(text) => serde_json::from_str::<Settings>(&text).unwrap_or_default().sanitized(),
        Err(_) => {
            let s = platform::legacy_settings().unwrap_or_default();
            save(&s);
            s
        }
    }
}

pub fn save(s: &Settings) {
    let _ = fs::create_dir_all(platform::config_dir());
    if let Ok(text) = serde_json::to_string_pretty(s) {
        let _ = fs::write(file(), text);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_fields_fall_back_to_defaults() {
        let s: Settings = serde_json::from_str(r#"{"allowUnc":true,"fileAction":"open"}"#).unwrap();
        assert!(s.allow_unc && s.open_url && s.show_toast);
        assert_eq!(s.file_action, FileAction::Open);
    }

    #[test]
    fn invalid_rate_limit_is_reset() {
        let s: Settings = serde_json::from_str(r#"{"rateLimitPerMinute":7}"#).unwrap();
        assert_eq!(s.sanitized().rate_limit_per_minute, RATE_DEFAULT);
    }
}
