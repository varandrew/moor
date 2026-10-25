import { useQuery, useQueryClient } from "@tanstack/react-query";
import type {
  AuthorizationClientInput,
  AuthorizationStart,
  RevocationResult,
  ServerAuthorization,
} from "@moor/types";
import { api, apiDelete, apiPost, apiPut } from "@/lib/api/client";
import { useSSEEvent } from "@/contexts/SSEContext";
import { open } from "@tauri-apps/plugin-shell";
import { isTauriRuntime } from "@/lib/tauri";

export function useServerAuthorization(serverId?: string) {
  const client = useQueryClient();
  const queryKey = ["server-authorization", serverId] as const;
  const query = useQuery({
    queryKey,
    enabled: Boolean(serverId),
    queryFn: ({ signal }) =>
      api<ServerAuthorization>(`/api/servers/${encodeURIComponent(serverId!)}/authorization`, {
        signal,
      }),
  });

  useSSEEvent("server:authorization", (snapshot) => {
    if (snapshot.serverId === serverId) client.setQueryData(queryKey, snapshot);
  });

  const registerClient = async (input: AuthorizationClientInput) => {
    const snapshot = await apiPut<ServerAuthorization>(
      `/api/servers/${encodeURIComponent(serverId!)}/authorization/client`,
      input,
    );
    client.setQueryData(queryKey, snapshot);
  };

  const start = async () => {
    const started = await apiPost<AuthorizationStart>(
      `/api/servers/${encodeURIComponent(serverId!)}/authorization/start`,
      {},
    );
    if (isTauriRuntime()) await open(started.authorizationUrl);
    else window.open(started.authorizationUrl, "_blank", "noopener,noreferrer");
  };

  const revoke = async () => {
    const result = await apiDelete<RevocationResult>(
      `/api/servers/${encodeURIComponent(serverId!)}/authorization`,
    );
    client.setQueryData(queryKey, result.snapshot);
    return result;
  };

  return { authorization: query.data, ...query, registerClient, start, revoke };
}
