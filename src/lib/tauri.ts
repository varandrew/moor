import { invoke, isTauri } from "@tauri-apps/api/core";
import type { SidecarInfo } from "@moor/types";

export function isTauriRuntime(): boolean {
  return isTauri();
}

export async function getSidecarInfo(): Promise<SidecarInfo> {
  return invoke<SidecarInfo>("get_sidecar_info");
}

export async function getServerLogPath(serverId: string): Promise<string> {
  return invoke<string>("get_server_log_path", { serverId });
}

export async function syncRuntimeSettings(locale?: "zh-CN" | "en"): Promise<void> {
  if (!isTauriRuntime()) return;
  await invoke("sync_runtime_settings", { locale });
}

export async function applyLoginAutostartSetting(enabled: boolean): Promise<void> {
  if (!isTauriRuntime()) return;
  await invoke("apply_login_autostart_setting", { enabled });
}
