import { useEffect, useState } from "react";
import { useSettings } from "@/hooks/useSettings";
import { useLockedMutation } from "@/hooks/useLockedMutation";
import { useTranslation } from "@/contexts/LocaleContext";
import { getErrorMessage } from "@/lib/utils";
import { Switch } from "@/components/ui/switch";
import { Input } from "@/components/ui/input";
import { Button } from "@/components/ui/button";

export function HealthSettings({ onError }: { onError: (message: string | null) => void }) {
  const { t } = useTranslation();
  const { settings, updateSettings } = useSettings();
  const [interval, setInterval] = useState(String(settings.advanced.mcpHealthCheckIntervalSeconds));
  useEffect(
    () => setInterval(String(settings.advanced.mcpHealthCheckIntervalSeconds)),
    [settings.advanced.mcpHealthCheckIntervalSeconds],
  );
  const save = useLockedMutation(
    (advanced: { mcpHealthChecksEnabled?: boolean; mcpHealthCheckIntervalSeconds?: number }) =>
      updateSettings({ advanced }),
    {
      onMutate: () => onError(null),
      onError: (err) => onError(getErrorMessage(err)),
    },
  );
  const seconds = Number(interval);
  const valid =
    /^\d+$/.test(interval) && Number.isInteger(seconds) && seconds >= 30 && seconds <= 600;
  return (
    <div className="py-3.5 px-4 space-y-3">
      <div className="flex items-center justify-between gap-4">
        <div>
          <p className="text-sm">{t("Automatic Health Checks")}</p>
          <p className="text-xs text-[var(--fg-45)] mt-0.5">
            {t("Ping running MCP servers. Three failures trigger a warning; recovery is manual.")}
          </p>
        </div>
        <Switch
          checked={settings.advanced.mcpHealthChecksEnabled}
          disabled={save.pending}
          onCheckedChange={(value) => void save.mutate({ mcpHealthChecksEnabled: value })}
        />
      </div>
      <div className="flex items-center justify-between gap-4">
        <label htmlFor="health-interval" className="text-sm">
          {t("Health Check Interval (seconds)")}
        </label>
        <div className="flex gap-2">
          <Input
            id="health-interval"
            type="number"
            min={30}
            max={600}
            step={1}
            value={interval}
            aria-invalid={!valid}
            aria-describedby={!valid ? "health-interval-error" : undefined}
            className="w-20 h-8 text-center text-xs"
            onChange={(event) => setInterval(event.target.value)}
          />
          <Button
            variant="secondary"
            size="sm"
            disabled={!valid || save.pending}
            onClick={() => void save.mutate({ mcpHealthCheckIntervalSeconds: seconds })}
          >
            {t("Apply")}
          </Button>
        </div>
      </div>
      {!valid && (
        <p id="health-interval-error" className="text-xs text-error-warm">
          {t("Enter a whole number between {min} and {max}.", { min: 30, max: 600 })}
        </p>
      )}
      <p className="text-xs text-[var(--fg-45)]">
        {t("Busy sessions are skipped. A health check may delay a tool call by up to 5 seconds.")}
      </p>
    </div>
  );
}
