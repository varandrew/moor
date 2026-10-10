import { useState } from "react";
import { open } from "@tauri-apps/plugin-shell";
import type { UpdateCheck as CheckResult } from "@moor/types";
import { api } from "@/lib/api/client";
import { isTauriRuntime } from "@/lib/tauri";
import { getErrorMessage } from "@/lib/utils";
import { useLockedMutation } from "@/hooks/useLockedMutation";
import { useTranslation } from "@/contexts/LocaleContext";
import { Button } from "@/components/ui/button";
import { ErrorBanner } from "./ErrorBanner";

export function UpdateCheck() {
  const { t } = useTranslation();
  const [result, setResult] = useState<CheckResult | null>(null);
  const [error, setError] = useState<string | null>(null);
  const check = useLockedMutation(() => api<CheckResult>("/api/updates/check"), {
    onMutate: () => {
      setResult(null);
      setError(null);
    },
    onSuccess: setResult,
    onError: (err) => setError(getErrorMessage(err)),
  });
  const openRelease = async () => {
    if (!result) return;
    try {
      if (isTauriRuntime()) await open(result.releaseUrl);
      else window.open(result.releaseUrl, "_blank", "noopener,noreferrer");
    } catch (err) {
      setError(getErrorMessage(err));
    }
  };
  return (
    <div className="mt-4 space-y-2">
      <Button
        variant="outline"
        size="sm"
        disabled={check.pending}
        onClick={() => void check.mutate()}
      >
        {t(check.pending ? "Checking..." : "Check for updates")}
      </Button>
      {result && (
        <div className="text-xs space-y-2" role="status">
          <p>
            {t(
              result.updateAvailable
                ? "An update is available."
                : "No newer stable release is available.",
            )}
          </p>
          <p>
            {t("Current: {current}; latest stable: {latest}", {
              current: result.currentVersion,
              latest: result.latestVersion,
            })}
          </p>
          <Button variant="secondary" size="sm" onClick={() => void openRelease()}>
            {t("Open release page")}
          </Button>
        </div>
      )}
      {error && <ErrorBanner message={error} />}
    </div>
  );
}
