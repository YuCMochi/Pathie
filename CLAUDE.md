# Pathie

Windows 系統匣小工具：偵測剪貼簿內容，若是有效的絕對路徑或 http(s) 網址就自動開啟。
Rust + Tauri 2 後端，React 前端（設定視窗與開啟成功浮窗）。方向與待辦見 BACKLOG.md。

## 結構
- `src-tauri/src/logic.rs`：純邏輯（解析路徑/網址、安全判斷、頻率上限），不碰 Win32，含多數測試
- `src-tauri/src/platform.rs`：Win32 層（剪貼簿監聽、ShellExecute、開機啟動、舊登錄檔設定）；日後 Mac 版換這層
- `src-tauri/src/settings.rs`：`%APPDATA%\Pathie\settings.json` 讀寫；無 JSON 時從登錄檔 `HKCU\Software\Pathie` 搬一次
- `src-tauri/src/app.rs`：共用狀態與 `handle_clipboard` / `act_on`
- `src-tauri/src/tray.rs`、`toast.rs`、`commands.rs`、`main.rs`：系統匣、浮窗、前端 IPC、啟動流程
- `ui/src/`：`App.tsx` 設定視窗、`toast.tsx` 浮窗、`pages/` 各分頁、`components/ui/` shadcn 風格元件、
  `lib/store.ts` 前端狀態（以後端為準）、`index.css` 色票（換皮只改這裡）
- `installer.iss`：Inno Setup 安裝程式；`icon/`：圖示來源

## 慣例
- 前端：React 19 + TS + Tailwind v4 + shadcn 風格（Radix）、圖示用 lucide；Tailwind 尺寸是 rem，
  不要把 `html` 字級改掉（曾因此讓開關歪掉）
- 設定欄位新增要同步：`settings.rs`、`ui/src/lib/store.ts` 的 `Settings` 型別、對應分頁
- 不做失敗提示（太吵），只有開啟成功浮窗與頻率上限/重複啟動的系統通知
- 版本號在 `src-tauri/Cargo.toml`（與 Cargo.lock），升版 chore 直接在 main

## 指令
- 測試：`cd src-tauri && cargo test`
- 前端型別檢查+打包：`npm run build`
- 開發：`npm run tauri -- dev`（debug 版會連 Vite，單獨執行 debug exe 會顯示 localhost 拒絕連線）
- 發佈建置：`npm run tauri -- build --no-bundle` → `src-tauri/target/release/Pathie.exe`
- 啟動參數：`--settings` 直接開設定視窗、`--autostart` 開機自動啟動（不彈首次導覽與啟動通知）
- 除錯 WebView：環境變數 `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9222`
