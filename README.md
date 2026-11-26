# Pathie

Pathie 是一款 Windows 桌面工具，會在系統工具列常駐，監控你的剪貼簿內容，當你複製有效的檔案或資料夾路徑時，會自動幫你開啟該路徑。可在系統工具列左鍵點擊圖示快速暫停/恢復監控；右鍵選單可開啟設定視窗或退出程式。

以 Rust + [Tauri 2](https://tauri.app/) 實作（設定視窗為 Svelte），不需要 .NET runtime，產出單一 exe（約 3–4 MB）。設定視窗使用系統的 Microsoft Edge WebView2 Runtime（Windows 10/11 通常已內建）。

## 安裝與使用

1. 點擊 release ，下載並解壓縮 Pathie.zip
2. 執行 `Pathie.exe`，程式會自動縮到系統工具列。
3. 複製任何有效的檔案或資料夾路徑，程式會自動開啟。

也會開啟網址：只認 `http://`、`https://` 開頭，或 `www.` 開頭（自動補上 `https://`）。其他協定（如 `ms-msdt:`）與沒有前綴的裸網域（如 `example.com`）不處理。可在設定視窗「檔案與網址」關閉「自動開啟網址」，或讓「從瀏覽器複製的網址不開啟」（預設皆開），複製網址要傳給別人時就不會再開新分頁。

支援的路徑形式：一般絕對路徑、加引號的路徑、正斜線路徑、`file:///` URL、含環境變數的路徑（如 `%TEMP%`）。

## 建置 (Build)

需要 [Rust](https://rustup.rs/)、MSVC build tools 與 [Node.js](https://nodejs.org/)：

```
npm install
npm run tauri -- build --no-bundle
```

產物在 `src-tauri/target/release/Pathie.exe`，單一檔案即可執行。開發時可用 `npm run tauri -- dev`；啟動時加 `--settings` 會直接開啟設定視窗。

打包安裝程式（需要 [Inno Setup 6](https://jrsoftware.org/isinfo.php)，先照上面建置出 exe）：

```
ISCC /DAppVersion=<src-tauri/Cargo.toml 的版本號> installer.iss
```

產物在 `dist/Pathie-Setup.exe`。

跑測試：

```
cd src-tauri
cargo test
```

## 注意事項 (Notes)

- 請勿重複執行多個實例，程式已自動防呆。
- 程式是免安裝的單一 exe。
- 基於安全考量，只有**絕對路徑**才會觸發開啟；裸檔名（如 `gpedit.msc`）與相對路徑會被忽略，避免靠工作目錄湊巧命中系統指令。
- **資料夾一律直接開啟；檔案預設只在檔案總管中選取**，不會執行，避免繞過 Windows 對執行檔的保護。判斷依據是路徑實際上是不是資料夾，不是看副檔名。設定視窗「檔案與網址」的「複製檔案路徑時」可改成「不處理」或「直接開啟」，設定會記住。
- 選「直接開啟」時，`.exe`、`.bat`、`.cmd`、`.ps1`、`.vbs`、`.js`、`.lnk`、`.msi` 等可執行／腳本類副檔名仍只會選取、不會執行（完整清單見原始碼 `DANGEROUS_EXTS`）。
- **網路路徑（`\\host\share`）預設不處理**：光是檢查路徑是否存在，Windows 就會主動連線到對方的 SMB 伺服器，可能洩漏登入雜湊（NTLM hash）。需要時可在設定視窗「安全」開啟「處理網路路徑」。
- **從瀏覽器複製的網址不開啟**：複製當下若前景視窗是下列瀏覽器就略過網址（程序名稱不分大小寫）：Chrome、Edge、Firefox、Brave、Opera、Vivaldi、Zen、Arc、Dia。判斷依據是前景視窗而非剪貼簿來源，所以從瀏覽器頁面內複製連結也會被略過；要開啟可在設定視窗關掉此選項。清單見原始碼 `BROWSERS`。
- 0.1.x 的「僅開啟資料夾」設定不會沿用，升級後一律回到「在資料夾中選取」。
- **頻率上限（防濫用）**：每分鐘自動開啟超過上限（預設 20 次，可在設定視窗「安全」改成 10／20／50／100）時，會自動暫停監控並跳通知，避免有程式瘋狂複製路徑造成視窗洪水。左鍵點圖示即可恢復，設定會記住。
- **開啟成功提示**：Pathie 開啟路徑或網址後，螢幕右下角會短暫顯示「由 Pathie 開啟」，可在設定視窗「提示」關閉。複製了無關文字或無效路徑時不會有任何提示，避免干擾。
- 設定改存在 `%APPDATA%\Pathie\settings.json`；從舊版（0.2.x 以前存在登錄檔 `HKCU\Software\Pathie`）升級時，首次啟動會自動搬入。
