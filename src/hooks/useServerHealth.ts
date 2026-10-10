import { useQuery, useQueryClient } from "@tanstack/react-query";
import type { ServerHealth } from "@moor/types";
import { api, apiPost } from "@/lib/api/client";
import { useSSEEvent } from "@/contexts/SSEContext";

const key = ["server-health"] as const;

export function useServerHealth() {
  const client = useQueryClient();
  const query = useQuery({
    queryKey: key,
    queryFn: ({ signal }) => api<ServerHealth[]>("/api/servers/health", { signal }),
  });
  useSSEEvent("server:health", (snapshot) => {
    client.setQueryData<ServerHealth[]>(key, (previous) => [
      ...(previous ?? []).filter((item) => item.serverId !== snapshot.serverId),
      snapshot,
    ]);
  });
  useSSEEvent("server:status", () => {
    void client.invalidateQueries({ queryKey: key });
  });
  const check = async (id: string) => {
    const snapshot = await apiPost<ServerHealth>(
      `/api/servers/${encodeURIComponent(id)}/check-health`,
      {},
    );
    client.setQueryData<ServerHealth[]>(key, (previous) => [
      ...(previous ?? []).filter((item) => item.serverId !== id),
      snapshot,
    ]);
  };
  return { snapshots: query.data ?? [], check };
}
