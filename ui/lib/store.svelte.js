// 前端狀態：所有設定都以後端為準，改動後立即送回後端存檔並採用回傳值。
import { invoke } from '@tauri-apps/api/core';

export const cfg = $state({
  ready: false,
  settings: {},
  info: { version: '', author: '', repository: '', rateOptions: [] },
  startup: false,
  monitoring: true,
});

export async function load() {
  const [settings, info, startup, monitoring] = await Promise.all([
    invoke('get_settings'),
    invoke('app_info'),
    invoke('get_startup'),
    invoke('get_monitoring'),
  ]);
  Object.assign(cfg, { settings, info, startup, monitoring, ready: true });
}

export async function update(patch) {
  cfg.settings = { ...cfg.settings, ...patch };
  cfg.settings = await invoke('set_settings', { settings: cfg.settings });
}

export async function setStartup(enable) {
  cfg.startup = await invoke('set_startup', { enable });
}

export async function setMonitoring(on) {
  cfg.monitoring = on;
  await invoke('set_monitoring', { on });
}

export const openRepository = () => invoke('open_repository');
