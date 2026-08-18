import { useState, useEffect, useCallback } from "react";
import { cn, createErrorWithCause, getErrorMessage } from "@/lib/utils";
import { getApiRuntime, resetRuntime } from "@/lib/api/runtime";
import { syncRuntimeSettings, applyLoginAutostartSetting } from "@/lib/tauri";
import { PageHeader } from "@/components/shared/PageHeader";
import { ErrorBanner } from "@/components/shared/ErrorBanner";
import { useSettings } from "@/hooks/useSettings";
import { Switch } from "@/components/ui/switch";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Card, CardContent } from "@/components/ui/card";
import { Tabs } from "@/components/ui/tabs";
import { Cog, Palette, Wrench, AlertTriangle, Eye, EyeOff, ExternalLink } from "lucide-react";
import type { GeneralSettings, SettingsGroup, SidecarInfo } from "@moor/types";
import { CopyButton } from "@/components/shared/CopyButton";
import {
  getAdvancedPortStatus,
  getGeneralSettingRuntimeAction,
  getPortBannerState,
  getSettingsPageLoadState,
  parseIdleTtlSecondsInput,
  parseTimeoutSecondsInput,
} from "./settings-state";

declare const __APP_VERSION__: string;

// --- Reusable Setting Row ---

interface SettingRowProps {
  label: string;
  description?: string;
  children: React.ReactNode;
}

function SettingRow({ label, description, children }: SettingRowProps) {
  return (
    <div className="flex items-center justify-between py-3.5 px-4">
      <div className="flex-1 min-w-0 mr-4">
        <p className="font-headline text-sm text-cursor-dark">{label}</p>
        {description && (
          <p className="font-body text-xs text-[var(--fg-45)] mt-0.5">{description}</p>
        )}
      </div>
      <div className="shrink-0">{children}</div>
    </div>
  );
}

// --- Restart Banner ---

function RestartBanner({ title, message }: { title: string; message: string }) {
  return (
    <div className="flex items-start gap-3 bg-cursor-orange/10 border border-cursor-orange/20 rounded-xl px-4 py-3">
      <AlertTriangle className="h-4 w-4 text-cursor-orange shrink-0 mt-0.5" />
      <div className="min-w-0 space-y-0.5">
        <p className="font-headline text-sm text-cursor-dark">{title}</p>
        <p className="font-body text-xs text-[var(--fg-55)]">{message}</p>
      </div>
    </div>
  );
}

// --- Group Nav Item ---

interface GroupNavItemProps {
  icon: React.ElementType;
  label: string;
  active: boolean;
  onClick: () => void;
}

function GroupNavItem({ icon: Icon, label, active, onClick }: GroupNavItemProps) {
  return (
    <button
      onClick={onClick}
      className={cn(
        "flex items-center gap-2.5 w-full px-3 py-2 rounded-lg font-headline text-sm transition-all duration-200",
        active
          ? "bg-surface-400 text-cursor-dark font-medium"
          : "text-[var(--fg-55)] hover:bg-[var(--fg-06)] hover:text-cursor-dark",
      )}
    >
      <Icon className="h-4 w-4" />
      {label}
    </button>
  );
}

// --- Settings Sections ---

function GeneralSection({ onError }: { onError: (message: string | null) => void }) {
  const { settings, updateSettings } = useSettings();

  const handleSwitch = useCallback(
    async (key: keyof GeneralSettings, value: boolean) => {
      const general: Partial<GeneralSettings> = { [key]: value };
      try {
        onError(null);
        const action = getGeneralSettingRuntimeAction(key);
        if (action === "loginAutostart") {
          await applyLoginAutostartSetting(value);
          try {
            await updateSettings({ general });
          } catch (err) {
            try {
              await applyLoginAutostartSetting(!value);
            } catch (rollbackErr) {
              throw createErrorWithCause(
                `${getErrorMessage(err, "Failed to save login auto-start setting")}. Rollback failed: ${getErrorMessage(rollbackErr, "unknown error")}`,
                rollbackErr,
              );
            }
            throw err;
          }
          return;
        }

        await updateSettings({ general });
        if (action === "windowRuntime") {
          await syncRuntimeSettings();
        }
      } catch (err) {
        onError(getErrorMessage(err, "Failed to update runtime settings"));
      }
    },
    [onError, updateSettings],
  );

  return (
    <Card>
      <CardContent className="p-2 divide-y divide-[var(--fg-06)]">
        <SettingRow
          label="Auto-start on Login"
          description="Launch Moor automatically when you log in"
        >
          <Switch
            checked={settings.general.autoStartOnLogin}
            onCheckedChange={(v) => void handleSwitch("autoStartOnLogin", v)}
          />
        </SettingRow>
        <SettingRow
          label="Auto-start Servers on Launch"
          description="Automatically start servers marked as auto-start when Moor opens"
        >
          <Switch
            checked={settings.general.autoStartServersOnLaunch}
            onCheckedChange={(v) => void handleSwitch("autoStartServersOnLaunch", v)}
          />
        </SettingRow>
        <SettingRow
          label="Minimize to Tray on Close"
          description="Keep Moor running in the system tray when the window is closed"
        >
          <Switch
            checked={settings.general.minimizeToTrayOnClose}
            onCheckedChange={(v) => void handleSwitch("minimizeToTrayOnClose", v)}
          />
        </SettingRow>
        <SettingRow
          label="Hide Dock Icon on Close"
          description="Hide the macOS Dock icon after the window is closed"
        >
          <Switch
            checked={settings.general.hideDockIconOnClose}
            disabled={!settings.general.minimizeToTrayOnClose}
            onCheckedChange={(v) => void handleSwitch("hideDockIconOnClose", v)}
          />
        </SettingRow>
        <SettingRow
          label="Show Window on Launch"
          description="Display the main window when Moor starts"
        >
          <Switch
            checked={settings.general.showWindowOnLaunch}
            disabled={!settings.general.minimizeToTrayOnClose}
            onCheckedChange={(v) => void handleSwitch("showWindowOnLaunch", v)}
          />
        </SettingRow>
        {!settings.general.minimizeToTrayOnClose && (
          <p className="px-4 py-2 font-body text-xs text-[var(--fg-35)]">
            Enable "Minimize to Tray on Close" to configure window visibility on launch
          </p>
        )}
      </CardContent>
    </Card>
  );
}

function AppearanceSection({ onError }: { onError: (message: string | null) => void }) {
  const { settings, updateSettings } = useSettings();

  const handleThemeChange = useCallback(
    async (value: string) => {
      try {
        onError(null);
        await updateSettings({ appearance: { theme: value as "light" | "dark" | "system" } });
      } catch (err) {
        onError(getErrorMessage(err, "Failed to update theme"));
      }
    },
    [onError, updateSettings],
  );

  return (
    <Card>
      <CardContent className="p-2">
        <div className="flex items-center justify-between py-3.5 px-4">
          <div className="flex-1 min-w-0 mr-4">
            <p className="font-headline text-sm text-cursor-dark">Theme</p>
            <p className="font-body text-xs text-[var(--fg-45)] mt-0.5">
              Choose the application appearance
            </p>
          </div>
          <Tabs
            value={settings.appearance.theme}
            onValueChange={(v) => void handleThemeChange(v)}
            tabs={[
              { value: "light", label: "Light" },
              { value: "dark", label: "Dark" },
              { value: "system", label: "System" },
            ]}
          />
        </div>
      </CardContent>
    </Card>
  );
}

function AdvancedSection({
  runtimeInfo,
  onError,
  onPortApplied,
  onLanAccessApplied,
}: {
  runtimeInfo: SidecarInfo | null;
  onError: (message: string | null) => void;
  onPortApplied: (port: number) => void;
  onLanAccessApplied: () => void;
}) {
  const { settings, updateSettings } = useSettings();
  const [localRetention, setLocalRetention] = useState(String(settings.advanced.logRetentionDays));
  const [localPort, setLocalPort] = useState(String(settings.advanced.sidecarPort));
  const [localRequestTimeout, setLocalRequestTimeout] = useState(
    String(settings.advanced.mcpRequestTimeoutMs / 1000),
  );
  const [localStartTimeout, setLocalStartTimeout] = useState(
    String(settings.advanced.mcpServerStartTimeoutMs / 1000),
  );
  const [localSessionIdleTtl, setLocalSessionIdleTtl] = useState(
    String(settings.advanced.mcpSessionIdleTtlMs / 1000),
  );
  const [tokenVisible, setTokenVisible] = useState(false);
  const requestTimeoutState = parseTimeoutSecondsInput(localRequestTimeout);
  const startTimeoutState = parseTimeoutSecondsInput(localStartTimeout);
  const sessionIdleTtlState = parseIdleTtlSecondsInput(localSessionIdleTtl);
  const requestTimeoutErrorId = "request-timeout-error";
  const startTimeoutErrorId = "server-start-timeout-error";
  const sessionIdleTtlErrorId = "session-idle-ttl-error";
  const portStatus = getAdvancedPortStatus({
    runtimeInfo,
    configuredPort: settings.advanced.sidecarPort,
  });

  useEffect(() => {
    setLocalRetention(String(settings.advanced.logRetentionDays));
    setLocalPort(String(settings.advanced.sidecarPort));
    setLocalRequestTimeout(String(settings.advanced.mcpRequestTimeoutMs / 1000));
    setLocalStartTimeout(String(settings.advanced.mcpServerStartTimeoutMs / 1000));
    setLocalSessionIdleTtl(String(settings.advanced.mcpSessionIdleTtlMs / 1000));
  }, [
    settings.advanced.logRetentionDays,
    settings.advanced.sidecarPort,
    settings.advanced.mcpRequestTimeoutMs,
    settings.advanced.mcpServerStartTimeoutMs,
    settings.advanced.mcpSessionIdleTtlMs,
  ]);

  const applyRetention = async () => {
    try {
      onError(null);
      await updateSettings({ advanced: { logRetentionDays: Number(localRetention) } });
    } catch (err) {
      onError(getErrorMessage(err, "Failed to update log retention"));
    }
  };

  const applyPort = async () => {
    try {
      onError(null);
      const nextPort = Number(localPort);
      await updateSettings({ advanced: { sidecarPort: nextPort } });
      onPortApplied(nextPort);
    } catch (err) {
      onError(getErrorMessage(err, "Failed to update sidecar port"));
    }
  };

  const toggleLanAccess = async (value: boolean) => {
    try {
      onError(null);
      await updateSettings({ advanced: { allowLanMcpAccess: value } });
      onLanAccessApplied();
    } catch (err) {
      onError(getErrorMessage(err, "Failed to update LAN MCP access"));
    }
  };

  type TimeoutKey = "mcpRequestTimeoutMs" | "mcpServerStartTimeoutMs" | "mcpSessionIdleTtlMs";

  const applyTimeout = async (
    key: TimeoutKey,
    parsed: typeof requestTimeoutState,
    label: string,
  ) => {
    try {
      onError(null);
      if (!parsed.valid) {
        onError(parsed.message);
        return;
      }
      await updateSettings({ advanced: { [key]: parsed.milliseconds } });
    } catch (err) {
      onError(getErrorMessage(err, `Failed to update ${label}`));
    }
  };

  return (
    <div className="space-y-4">
      <Card>
        <CardContent className="p-2 divide-y divide-[var(--fg-06)]">
          <SettingRow
            label="Log Retention"
            description="Number of days to keep audit logs (0 for unlimited)"
          >
            <div className="flex items-center gap-2">
              <Input
                type="number"
                min={0}
                max={365}
                value={localRetention}
                onChange={(e) => setLocalRetention(e.target.value)}
                className="w-20 h-8 text-center text-xs"
              />
              <Button variant="secondary" size="sm" onClick={() => void applyRetention()}>
                Apply
              </Button>
            </div>
          </SettingRow>
          <SettingRow label="Audit Logging" description="Record tool calls in the audit log">
            <Switch
              checked={settings.advanced.enableAuditLogging}
              onCheckedChange={(v) => void updateSettings({ advanced: { enableAuditLogging: v } })}
            />
          </SettingRow>
          <SettingRow
            label="Allow LAN MCP Access"
            description="Expose /mcp on all interfaces for WSL2/LAN clients; /api stays loopback-only (requires restart)"
          >
            <Switch
              checked={settings.advanced.allowLanMcpAccess}
              onCheckedChange={(v) => void toggleLanAccess(v)}
            />
          </SettingRow>
          <SettingRow
            label="Request Timeout"
            description="Timeout for MCP JSON-RPC requests in seconds (5-300). Applies to the next MCP request."
          >
            <div className="flex flex-col items-end gap-1">
              <div className="flex items-center gap-2">
                <Input
                  type="number"
                  min={5}
                  max={300}
                  step={1}
                  value={localRequestTimeout}
                  aria-invalid={!requestTimeoutState.valid}
                  aria-describedby={requestTimeoutState.valid ? undefined : requestTimeoutErrorId}
                  onChange={(e) => setLocalRequestTimeout(e.target.value)}
                  className="w-20 h-8 text-center text-xs"
                />
                <Button
                  variant="secondary"
                  size="sm"
                  disabled={!requestTimeoutState.valid}
                  onClick={() =>
                    void applyTimeout("mcpRequestTimeoutMs", requestTimeoutState, "request timeout")
                  }
                >
                  Apply
                </Button>
              </div>
              {!requestTimeoutState.valid && (
                <p id={requestTimeoutErrorId} className="font-body text-[11px] text-error-warm">
                  {requestTimeoutState.message}
                </p>
              )}
            </div>
          </SettingRow>
          <SettingRow
            label="Server Start Timeout"
            description="Total startup wait for MCP servers in seconds (5-300). Applies to the next server start."
          >
            <div className="flex flex-col items-end gap-1">
              <div className="flex items-center gap-2">
                <Input
                  type="number"
                  min={5}
                  max={300}
                  step={1}
                  value={localStartTimeout}
                  aria-invalid={!startTimeoutState.valid}
                  aria-describedby={startTimeoutState.valid ? undefined : startTimeoutErrorId}
                  onChange={(e) => setLocalStartTimeout(e.target.value)}
                  className="w-20 h-8 text-center text-xs"
                />
                <Button
                  variant="secondary"
                  size="sm"
                  disabled={!startTimeoutState.valid}
                  onClick={() =>
                    void applyTimeout(
                      "mcpServerStartTimeoutMs",
                      startTimeoutState,
                      "server start timeout",
                    )
                  }
                >
                  Apply
                </Button>
              </div>
              {!startTimeoutState.valid && (
                <p id={startTimeoutErrorId} className="font-body text-[11px] text-error-warm">
                  {startTimeoutState.message}
                </p>
              )}
            </div>
          </SettingRow>
          <SettingRow
            label="Session Idle TTL"
            description="Idle expiry for MCP sessions in seconds (300-86400). Idle clients re-initialize on their next request."
          >
            <div className="flex flex-col items-end gap-1">
              <div className="flex items-center gap-2">
                <Input
                  type="number"
                  min={300}
                  max={86400}
                  step={1}
                  value={localSessionIdleTtl}
                  aria-invalid={!sessionIdleTtlState.valid}
                  aria-describedby={sessionIdleTtlState.valid ? undefined : sessionIdleTtlErrorId}
                  onChange={(e) => setLocalSessionIdleTtl(e.target.value)}
                  className="w-20 h-8 text-center text-xs"
                />
                <Button
                  variant="secondary"
                  size="sm"
                  disabled={!sessionIdleTtlState.valid}
                  onClick={() =>
                    void applyTimeout(
                      "mcpSessionIdleTtlMs",
                      sessionIdleTtlState,
                      "session idle TTL",
                    )
                  }
                >
                  Apply
                </Button>
              </div>
              {!sessionIdleTtlState.valid && (
                <p id={sessionIdleTtlErrorId} className="font-body text-[11px] text-error-warm">
                  {sessionIdleTtlState.message}
                </p>
              )}
            </div>
          </SettingRow>
        </CardContent>
      </Card>

      <Card>
        <CardContent className="p-2 divide-y divide-[var(--fg-06)]">
          <div>
            <SettingRow
              label="Sidecar Port"
              description="Port for the Moor API server (requires restart)"
            >
              <div className="flex items-center gap-2">
                <Input
                  type="number"
                  min={1024}
                  max={65535}
                  value={localPort}
                  onChange={(e) => setLocalPort(e.target.value)}
                  className="w-24 h-8 text-center text-xs"
                />
                <Button variant="secondary" size="sm" onClick={() => void applyPort()}>
                  Apply
                </Button>
              </div>
            </SettingRow>
            {portStatus?.kind === "mismatch" && (
              <p className="px-4 pb-3 -mt-1 font-body text-xs text-[var(--fg-45)]">
                Currently running on port {portStatus.currentPort}; configured for port{" "}
                {portStatus.configuredPort}. The configured port may already be used by another Moor
                instance.
              </p>
            )}
          </div>
          <div className="py-3.5 px-4">
            <div className="flex items-center justify-between mb-2">
              <div className="flex-1 min-w-0 mr-4">
                <p className="font-headline text-sm text-cursor-dark">API Token</p>
                <p className="font-body text-xs text-[var(--fg-45)] mt-0.5">
                  Authentication token for the Moor API
                </p>
              </div>
            </div>
            <div className="flex items-center gap-2">
              <code className="flex-1 font-mono text-xs bg-surface-100 px-3 py-2 rounded-lg truncate">
                {runtimeInfo
                  ? tokenVisible
                    ? runtimeInfo.apiToken
                    : "•".repeat(20)
                  : "Loading..."}
              </code>
              <Button variant="ghost" size="icon" onClick={() => setTokenVisible(!tokenVisible)}>
                {tokenVisible ? <EyeOff className="h-4 w-4" /> : <Eye className="h-4 w-4" />}
              </Button>
              {runtimeInfo && <CopyButton text={runtimeInfo.apiToken} />}
            </div>
          </div>
        </CardContent>
      </Card>

      <Card>
        <CardContent className="p-4">
          <div className="flex items-center justify-between">
            <div>
              <p className="font-headline text-sm text-cursor-dark">Moor v{__APP_VERSION__}</p>
              <p className="font-body text-xs text-[var(--fg-40)]">MCP Gateway Manager</p>
            </div>
            <a
              href="https://github.com/varandrew/moor"
              target="_blank"
              rel="noopener noreferrer"
              className="text-[var(--fg-40)] hover:text-cursor-dark transition-colors"
            >
              <ExternalLink className="h-4 w-4" />
            </a>
          </div>
        </CardContent>
      </Card>
    </div>
  );
}

// --- Main Settings Page ---

const groups: { key: SettingsGroup; label: string; icon: React.ElementType }[] = [
  { key: "general", label: "General", icon: Cog },
  { key: "appearance", label: "Appearance", icon: Palette },
  { key: "advanced", label: "Advanced", icon: Wrench },
];

export function SettingsPage() {
  const [activeGroup, setActiveGroup] = useState<SettingsGroup>("general");
  const [runtimeInfo, setRuntimeInfo] = useState<SidecarInfo | null>(null);
  const [errorMessage, setErrorMessage] = useState<string | null>(null);
  const [portChangeApplied, setPortChangeApplied] = useState(false);
  const [lanAccessChangeApplied, setLanAccessChangeApplied] = useState(false);
  const { settings, isLoading, isError, error, resetSettings } = useSettings();
  const loadState = getSettingsPageLoadState({ isLoading, isError, error });
  const portBannerState = loadState.canRenderControls
    ? getPortBannerState({
        runtimeInfo,
        configuredPort: settings.advanced.sidecarPort,
        portChangeApplied,
      })
    : null;

  const refreshRuntimeInfo = useCallback(async () => {
    setRuntimeInfo(await getApiRuntime());
  }, []);

  useEffect(() => {
    void refreshRuntimeInfo().catch(() => {});
  }, [refreshRuntimeInfo]);

  const handleReset = async () => {
    if (!window.confirm("Reset all settings to their default values?")) return;
    try {
      setErrorMessage(null);
      const previousAutoStartOnLogin = settings.general.autoStartOnLogin;
      await applyLoginAutostartSetting(false);
      try {
        await resetSettings();
      } catch (err) {
        await applyLoginAutostartSetting(previousAutoStartOnLogin);
        throw err;
      }
      await syncRuntimeSettings();
      resetRuntime();
      await refreshRuntimeInfo();
      setPortChangeApplied(false);
      setLanAccessChangeApplied(false);
    } catch (err) {
      setErrorMessage(getErrorMessage(err, "Failed to reset settings"));
    }
  };

  if (loadState.kind === "loading") {
    return (
      <div className="flex items-center justify-center h-64">
        <div className="h-8 w-8 rounded-full border-2 border-surface-300 border-t-cursor-orange animate-spin" />
      </div>
    );
  }

  return (
    <div className="space-y-6 animate-fade-in-up">
      <PageHeader
        title="Settings"
        subtitle="Configure Moor to your preferences"
        action={
          loadState.canRenderControls ? (
            <Button variant="outline" size="sm" onClick={handleReset}>
              Reset to Defaults
            </Button>
          ) : undefined
        }
      />

      {loadState.kind === "error" && <ErrorBanner message={loadState.message} />}
      {portBannerState?.kind === "restart" && (
        <RestartBanner
          title="Port changes require reopening Moor"
          message="Quit and reopen Moor to apply the configured port."
        />
      )}
      {lanAccessChangeApplied && (
        <RestartBanner
          title="LAN access changes require reopening Moor"
          message="Quit and reopen Moor to apply the new network binding."
        />
      )}
      {errorMessage && <ErrorBanner message={errorMessage} />}

      {loadState.canRenderControls && (
        <div className="flex gap-6">
          <nav className="w-44 shrink-0 space-y-0.5">
            {groups.map(({ key, label, icon }) => (
              <GroupNavItem
                key={key}
                icon={icon}
                label={label}
                active={activeGroup === key}
                onClick={() => setActiveGroup(key)}
              />
            ))}
          </nav>

          <div className="flex-1 min-w-0">
            {activeGroup === "general" && <GeneralSection onError={setErrorMessage} />}
            {activeGroup === "appearance" && <AppearanceSection onError={setErrorMessage} />}
            {activeGroup === "advanced" && (
              <AdvancedSection
                runtimeInfo={runtimeInfo}
                onError={setErrorMessage}
                onLanAccessApplied={() => setLanAccessChangeApplied(true)}
                onPortApplied={(nextPort) =>
                  setPortChangeApplied(runtimeInfo !== null && runtimeInfo.port !== nextPort)
                }
              />
            )}
          </div>
        </div>
      )}
    </div>
  );
}
