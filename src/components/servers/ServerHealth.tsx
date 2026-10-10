import { useState } from "react";
import type { ServerHealth as HealthSnapshot } from "@moor/types";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { ErrorBanner } from "@/components/shared/ErrorBanner";
import { useTranslation } from "@/contexts/LocaleContext";
import { useServerHealth } from "@/hooks/useServerHealth";
import { useLockedMutation } from "@/hooks/useLockedMutation";
import { getErrorMessage } from "@/lib/utils";

export function HealthBadge({ snapshot }: { snapshot?: HealthSnapshot }) {
  const { t } = useTranslation();
  const status = snapshot?.status ?? "unknown";
  const labels = {
    unknown: "Not checked",
    healthy: "Healthy",
    unhealthy: "Health check failed",
    unsupported: "Ping unsupported",
  } as const;
  return (
    <Badge variant={status === "unhealthy" ? "error" : status === "healthy" ? "success" : "subtle"}>
      {t(labels[status])}
    </Badge>
  );
}

export function ServerHealth({ serverId, running }: { serverId: string; running: boolean }) {
  const { t, locale } = useTranslation();
  const { snapshots, check } = useServerHealth();
  const [error, setError] = useState<string | null>(null);
  const mutation = useLockedMutation(() => check(serverId), {
    onMutate: () => setError(null),
    onError: (err) => setError(getErrorMessage(err)),
  });
  const snapshot = snapshots.find((item) => item.serverId === serverId);
  return (
    <div className="rounded-xl border border-[var(--fg-08)] p-4 space-y-2">
      <div className="flex items-center justify-between gap-3">
        <div className="flex flex-wrap items-center gap-2">
          <span className="text-sm">{t("Server Health")}</span>
          <HealthBadge snapshot={running ? snapshot : undefined} />
        </div>
        <Button
          variant="outline"
          size="sm"
          disabled={!running || mutation.pending}
          onClick={() => void mutation.mutate()}
        >
          {t(mutation.pending ? "Checking..." : "Check health")}
        </Button>
      </div>
      <p className="text-xs text-[var(--fg-45)]">
        {t("Health checks only report problems. Stop and start the server manually to recover.")}
      </p>
      {running && snapshot?.checkedAt && (
        <p className="text-xs text-[var(--fg-45)]">
          {t("Last checked: {time}; consecutive failures: {count}", {
            time: new Date(snapshot.checkedAt).toLocaleString(locale),
            count: snapshot.consecutiveFailures,
          })}
        </p>
      )}
      {running && snapshot?.errorMessage && <ErrorBanner message={snapshot.errorMessage} />}
      {error && <ErrorBanner message={error} />}
    </div>
  );
}
