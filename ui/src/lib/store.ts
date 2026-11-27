// 前端狀態：所有設定都以後端為準，改動後立即送回後端存檔並採用回傳值。
import { useSyncExternalStore } from 'react';
import { invoke } from '@tauri-apps/api/core';

export type Settings = {
  fileAction: 'ignore' | 'reveal' | 'open';
  allowUnc: boolean;
  openUrl: boolean;
  skipUrlFromBrowser: boolean;
  rateLimitPerMinute: number;
  showToast: boolean;
  firstRunDone: boolean;
};

export type Info = { version: string; author: string; repository: string; rateOptions: number[] };

type State = { ready: boolean; settings: Settings; info: Info; startup: boolean; monitoring: boolean };

let state: State = {
  ready: false,
  settings: {} as Settings,
  info: { version: '', author: '', repository: '', rateOptions: [] },
  startup: false,
  monitoring: true,
};
const listeners = new Set<() => void>();
const set = (patch: Partial<State>) => {
  state = { ...state, ...patch };
  listeners.forEach((l) => l());
};

export const useStore = () =>
  useSyncExternalStore(
    (l) => (listeners.add(l), () => listeners.delete(l)),
    () => state,
  );

export async function load() {
  const [settings, info, startup, monitoring] = await Promise.all([
    invoke<Settings>('get_settings'),
    invoke<Info>('app_info'),
    invoke<boolean>('get_startup'),
    invoke<boolean>('get_monitoring'),
  ]);
  set({ settings, info, startup, monitoring, ready: true });
}

export async function update(patch: Partial<Settings>) {
  set({ settings: { ...state.settings, ...patch } });
  set({ settings: await invoke<Settings>('set_settings', { settings: state.settings }) });
}

export const setStartup = async (enable: boolean) => set({ startup: await invoke<boolean>('set_startup', { enable }) });

export async function setMonitoring(on: boolean) {
  set({ monitoring: on });
  await invoke('set_monitoring', { on });
}

// 左鍵點系統匣、頻率上限自動暫停時由後端推送
export const syncMonitoring = (on: boolean) => set({ monitoring: on });

export const openRepository = () => invoke('open_repository');

// 載入完成後通知後端（它可能此刻才顯示視窗），回傳要切到的分頁
export const uiReady = () => invoke<string | null>('ui_ready');

// 檢查更新：只在使用者按下按鈕時才連網
export type UpdateStatus = { version: string; installable: boolean };
export const checkUpdate = () => invoke<UpdateStatus | null>('check_update');
// 安裝版：下載驗證後靜默安裝並重新啟動（成功時程式會直接結束）；免安裝版：開啟下載頁
export const installUpdate = () => invoke<void>('install_update');
