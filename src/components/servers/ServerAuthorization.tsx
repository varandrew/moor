import { useState } from "react";
import type { AuthorizationClientInput, AuthorizationState } from "@moor/types";
import { useServerAuthorization } from "@/hooks/useServerAuthorization";
import { useLockedMutation } from "@/hooks/useLockedMutation";
import { useTranslation } from "@/contexts/LocaleContext";
import { getErrorMessage } from "@/lib/utils";
import { messages } from "@/lib/messages";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { ErrorBanner } from "@/components/shared/ErrorBanner";

const labels: Record<AuthorizationState, string> = {
  not_required: "Not configured",
  required: "Authorization required",
  authorizing: "Waiting for browser",
  authorized: "Authorized",
  error: "Authorization failed",
};

export function AuthorizationBadge({ state }: { state: AuthorizationState }) {
  const { t } = useTranslation();
  return (
    <Badge variant={state === "authorized" ? "success" : state === "error" ? "error" : "subtle"}>
      {t(labels[state] as Parameters<typeof t>[0])}
    </Badge>
  );
}

export function AuthorizationButton({
  state,
  pending,
  onClick,
}: {
  state: AuthorizationState;
  pending: boolean;
  onClick: () => void;
}) {
  const { t } = useTranslation();
  return (
    <Button variant="outline" size="sm" disabled={pending} onClick={onClick}>
      {t(
        pending
          ? "Authorizing..."
          : state === "authorized"
            ? "Reauthorize"
            : state === "authorizing"
              ? "Restart authorization"
              : "Authorize",
      )}
    </Button>
  );
}

export function RevocationButton({ pending, onClick }: { pending: boolean; onClick: () => void }) {
  const { t } = useTranslation();
  return (
    <Button variant="outline" size="sm" disabled={pending} onClick={onClick}>
      {t(pending ? "Revoking..." : "Revoke")}
    </Button>
  );
}

export class AuthorizationClientError extends Error {
  cause?: unknown;

  constructor(message: string, cause?: unknown) {
    super(message);
    this.cause = cause;
  }
}

export async function authorizeServer({
  clientId,
  clientSecret,
  registerClient,
  start,
}: {
  clientId: string;
  clientSecret: string;
  registerClient: (input: AuthorizationClientInput) => Promise<unknown>;
  start: (clientId: string) => Promise<void>;
}) {
  const id = clientId.trim();
  if (id) {
    try {
      await registerClient({ clientId: id, ...(clientSecret ? { clientSecret } : {}) });
    } catch (error) {
      throw new AuthorizationClientError(getErrorMessage(error), error);
    }
  } else if (clientSecret) {
    throw new AuthorizationClientError("Client ID is required");
  }
  await start(id);
}

export function ServerAuthorization({ serverId }: { serverId: string }) {
  const { t } = useTranslation();
  const { authorization, registerClient, start, revoke } = useServerAuthorization(serverId);
  const [clientId, setClientId] = useState<string | null>(null);
  const [clientSecret, setClientSecret] = useState("");
  const [error, setError] = useState<{ message: string; preferSnapshot: boolean } | null>(null);
  const [warning, setWarning] = useState<string | null>(null);
  const state = authorization?.state ?? "not_required";
  const mutation = useLockedMutation(
    async () => {
      await authorizeServer({
        clientId: clientId ?? authorization?.clientId ?? "",
        clientSecret,
        registerClient,
        start,
      });
    },
    {
      onMutate: () => setError(null),
      onError: (error) =>
        setError({
          message: getErrorMessage(error),
          preferSnapshot: !(error instanceof AuthorizationClientError),
        }),
    },
  );
  const revocation = useLockedMutation(
    async () => {
      const result = await revoke();
      setWarning(result.warning ?? null);
    },
    {
      onMutate: () => {
        setError(null);
        setWarning(null);
      },
      onError: (error) => setError({ message: getErrorMessage(error), preferSnapshot: false }),
    },
  );

  return (
    <div className="rounded-xl border border-[var(--fg-08)] p-4 space-y-3">
      <div className="flex items-center justify-between gap-3">
        <span className="text-sm">{t("OAuth Authorization")}</span>
        <AuthorizationBadge state={state} />
      </div>
      <div className="grid gap-3 sm:grid-cols-2">
        <div className="space-y-1.5">
          <Label>{t("Client ID")}</Label>
          <Input
            value={clientId ?? authorization?.clientId ?? ""}
            onChange={(event) => setClientId(event.target.value)}
            autoComplete="off"
          />
        </div>
        <div className="space-y-1.5">
          <Label>{t("Client Secret (optional)")}</Label>
          <Input
            type="password"
            value={clientSecret}
            onChange={(event) => setClientSecret(event.target.value)}
            placeholder={t("Leave blank to keep the current secret")}
            autoComplete="new-password"
          />
        </div>
      </div>
      <div className="flex items-center justify-between gap-3">
        <p className="text-xs text-[var(--fg-45)]">
          {t("Tokens are stored in the system credential store and never shared with agents.")}
        </p>
        <div className="flex gap-2">
          {state === "authorized" && (
            <RevocationButton
              pending={revocation.pending}
              onClick={() => void revocation.mutate()}
            />
          )}
          <AuthorizationButton
            state={state}
            pending={mutation.pending}
            onClick={() => void mutation.mutate()}
          />
        </div>
      </div>
      {warning && <p className="text-xs text-[var(--fg-45)]">{warning}</p>}
      <AuthorizationErrorBanner
        errorCode={error?.preferSnapshot === false ? null : authorization?.errorCode}
        errorMessage={error?.message ?? authorization?.errorMessage}
      />
    </div>
  );
}

export function AuthorizationErrorBanner({
  errorCode,
  errorMessage,
}: {
  errorCode?: string | null;
  errorMessage?: string | null;
}) {
  const { text } = useTranslation();
  const hasTranslation = errorCode && Object.prototype.hasOwnProperty.call(messages, errorCode);
  const message = hasTranslation ? text(errorCode) : errorMessage;
  return message ? <ErrorBanner message={message} /> : null;
}
