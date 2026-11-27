; 編譯:ISCC /DAppVersion=<版本> installer.iss(版本號取自 Cargo.toml,見 README)
; 前置：先 npm run tauri -- build --no-bundle(產出 src-tauri\target\release\Pathie.exe)
#ifndef AppVersion
  #error 請用 /DAppVersion=x.y.z 傳入版本號
#endif
#define AppMutexName "com.yucmochi.pathie-sim"

[Setup]
; AppId 固定，之後升級才會被視為同一個程式
AppId={{B3A7C0E1-5D2F-4A68-8E19-7F4C2D9A6B05}
AppName=Pathie
AppVersion={#AppVersion}
AppPublisher=YuCMochi
AppPublisherURL=https://github.com/YuCMochi/Pathie
DefaultDirName={autopf}\Pathie
DefaultGroupName=Pathie
; 每使用者安裝：不用系統管理員權限，程式本來也只寫 HKCU
PrivilegesRequired=lowest
; 偵測到程式執行中會提示關閉，exe 才不會被鎖住
; Tauri single-instance 外掛的 mutex 名稱是「identifier-sim」,identifier 見 src-tauri\tauri.conf.json
; 程式內一鍵更新(/UPDATE)時改由 InitializeSetup 等程式自己結束，不跳「程式執行中」提示
AppMutex={code:GetAppMutex}
SetupIconFile=icon\app\256x256.ico
UninstallDisplayIcon={app}\Pathie.exe
OutputDir=dist
OutputBaseFilename=Pathie-Setup
Compression=lzma2
SolidCompression=yes
ArchitecturesInstallIn64BitMode=x64compatible
DisableProgramGroupPage=yes

[Languages]
; 繁中語言檔隨 repo 附上(Inno Setup 預設安裝不一定有)
Name: "zhtw"; MessagesFile: "installer-lang\ChineseTraditional.isl"

[Tasks]
Name: "autostart"; Description: "開機自動啟動 (Start with Windows)"

[Files]
Source: "src-tauri\target\release\Pathie.exe"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\Pathie"; Filename: "{app}\Pathie.exe"

[Registry]
; 格式與程式內「開機自動啟動」開關相同，兩邊互通
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; ValueType: string; ValueName: "Pathie"; ValueData: """{app}\Pathie.exe"" --autostart"; Tasks: autostart

[Run]
Filename: "{app}\Pathie.exe"; Description: "立即啟動 Pathie"; Flags: nowait postinstall skipifsilent
; 一鍵更新裝完後自動重新啟動，並回到設定視窗
Filename: "{app}\Pathie.exe"; Parameters: "--settings"; Flags: nowait; Check: IsUpdate

[Code]
// 設定視窗需要 WebView2 Runtime(Win10/11 通常已內建);缺少時提醒,但不擋安裝
function WebView2Installed: Boolean;
var
  v: String;
begin
  Result :=
    RegQueryStringValue(HKLM32, 'SOFTWARE\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}', 'pv', v) or
    RegQueryStringValue(HKCU, 'Software\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}', 'pv', v);
  Result := Result and (v <> '') and (v <> '0.0.0.0');
end;

// 程式內「下載並更新」會帶 /UPDATE(搭配 /VERYSILENT),見 src-tauri\src\update.rs
function IsUpdate: Boolean;
var
  i: Integer;
begin
  Result := False;
  for i := 1 to ParamCount do
    if CompareText(ParamStr(i), '/UPDATE') = 0 then
      Result := True;
end;

function GetAppMutex(Param: String): String;
begin
  if IsUpdate then
    Result := ''
  else
    Result := '{#AppMutexName}';
end;

function InitializeSetup: Boolean;
var
  i: Integer;
begin
  Result := True;
  if IsUpdate then
  begin
    // Pathie 啟動安裝程式後馬上結束，最多等 10 秒讓它關掉，exe 才不會被鎖住
    for i := 1 to 40 do
    begin
      if not CheckForMutexes('{#AppMutexName}') then
        Break;
      Sleep(250);
    end;
    Exit;
  end;
  if not WebView2Installed then
    Result := MsgBox('找不到 Microsoft Edge WebView2 Runtime,Pathie 的設定視窗需要它才能顯示。' + #13#10 +
      '請先到 https://developer.microsoft.com/microsoft-edge/webview2/ 安裝後再使用。' + #13#10#13#10 +
      '仍要繼續安裝嗎?', mbConfirmation, MB_YESNO) = IDYES;
end;

// Run 值可能是程式內的開關寫的而非安裝程式，所以解除安裝時一律清掉
procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
begin
  if CurUninstallStep = usPostUninstall then
  begin
    RegDeleteValue(HKCU, 'Software\Microsoft\Windows\CurrentVersion\Run', 'Pathie');
    RegDeleteKeyIncludingSubkeys(HKCU, 'Software\Pathie');
    DelTree(ExpandConstant('{userappdata}\Pathie'), True, True, True);
  end;
end;
