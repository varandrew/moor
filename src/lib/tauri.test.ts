import { describe, expect, it, vi } from "vite-plus/test";
import { syncRuntimeSettings } from "./tauri";

const { invoke } = vi.hoisted(() => ({ invoke: vi.fn().mockResolvedValue(undefined) }));
vi.mock("@tauri-apps/api/core", () => ({ invoke, isTauri: () => true }));

describe("tray language sync", () => {
  it("sends the resolved language through the existing runtime settings command", async () => {
    await syncRuntimeSettings("zh-CN");
    expect(invoke).toHaveBeenLastCalledWith("sync_runtime_settings", { locale: "zh-CN" });
    await syncRuntimeSettings("en");
    expect(invoke).toHaveBeenLastCalledWith("sync_runtime_settings", { locale: "en" });
  });
});
