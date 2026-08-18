import {
  MCP_TIMEOUT_MS_MAX,
  MCP_TIMEOUT_MS_MIN,
  MCP_SESSION_IDLE_TTL_MS_MAX,
  MCP_SESSION_IDLE_TTL_MS_MIN,
  type GeneralSettings,
  type SidecarInfo,
} from "@moor/types";

export type SettingsPageLoadState =
  | { kind: "loading"; canRenderControls: false }
  | { kind: "error"; canRenderControls: false; message: string }
  | { kind: "ready"; canRenderControls: true };

export type PortBannerState = { kind: "restart" };

export type AdvancedPortStatus = { kind: "mismatch"; currentPort: number; configuredPort: number };

export type GeneralSettingRuntimeAction = "loginAutostart" | "settingsOnly" | "windowRuntime";

export type TimeoutSecondsInputState =
  | { valid: true; milliseconds: number }
  | { valid: false; message: string };

const timeoutSecondsMin = MCP_TIMEOUT_MS_MIN / 1000;
const timeoutSecondsMax = MCP_TIMEOUT_MS_MAX / 1000;
const idleTtlSecondsMin = MCP_SESSION_IDLE_TTL_MS_MIN / 1000;
const idleTtlSecondsMax = MCP_SESSION_IDLE_TTL_MS_MAX / 1000;

function parseBoundedSecondsInput(
  value: string,
  secondsMin: number,
  secondsMax: number,
): TimeoutSecondsInputState {
  const error = `Enter a whole number between ${secondsMin} and ${secondsMax}.`;
  if (!/^\d+$/.test(value.trim())) {
    return { valid: false, message: error };
  }
  const seconds = Number(value);
  if (seconds < secondsMin || seconds > secondsMax) {
    return { valid: false, message: error };
  }
  return { valid: true, milliseconds: seconds * 1000 };
}

export function parseIdleTtlSecondsInput(value: string): TimeoutSecondsInputState {
  return parseBoundedSecondsInput(value, idleTtlSecondsMin, idleTtlSecondsMax);
}

export function getSettingsPageLoadState({
  isLoading,
  isError,
  error,
}: {
  isLoading: boolean;
  isError: boolean;
  error: unknown;
}): SettingsPageLoadState {
  if (isLoading) {
    return { kind: "loading", canRenderControls: false };
  }
  if (isError) {
    return {
      kind: "error",
      canRenderControls: false,
      message: error instanceof Error ? error.message : "Failed to load settings",
    };
  }
  return { kind: "ready", canRenderControls: true };
}

export function getPortBannerState({
  runtimeInfo,
  configuredPort,
  portChangeApplied,
}: {
  runtimeInfo: SidecarInfo | null;
  configuredPort: number;
  portChangeApplied: boolean;
}): PortBannerState | null {
  if (!runtimeInfo || runtimeInfo.port === configuredPort) {
    return null;
  }
  return portChangeApplied ? { kind: "restart" } : null;
}

export function getAdvancedPortStatus({
  runtimeInfo,
  configuredPort,
}: {
  runtimeInfo: SidecarInfo | null;
  configuredPort: number;
}): AdvancedPortStatus | null {
  if (!runtimeInfo || runtimeInfo.port === configuredPort) {
    return null;
  }
  return { kind: "mismatch", currentPort: runtimeInfo.port, configuredPort };
}

export function getGeneralSettingRuntimeAction(
  key: keyof GeneralSettings,
): GeneralSettingRuntimeAction {
  if (key === "autoStartOnLogin") {
    return "loginAutostart";
  }
  if (key === "autoStartServersOnLaunch") {
    return "settingsOnly";
  }
  return "windowRuntime";
}

export function parseTimeoutSecondsInput(value: string): TimeoutSecondsInputState {
  return parseBoundedSecondsInput(value, timeoutSecondsMin, timeoutSecondsMax);
}
