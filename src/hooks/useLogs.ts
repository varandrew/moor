import { useCallback } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { api } from "@/lib/api/client";
import { routes } from "@/lib/api-routes";
import { logKeys } from "@/lib/query-keys";
import { getErrorMessage } from "@/lib/utils";
import type { AuditLogEntry, LogInsights, LogStats } from "@moor/types";

const DEFAULT_STATS: LogStats = {
  totalCalls: 0,
  errorCalls: 0,
  errorRate: 0,
  avgDurationMs: null,
  topTools: [],
  topServers: [],
};

export function useLogs(filters?: {
  server_id?: string;
  tool_name?: string;
  from?: string;
  to?: string;
}) {
  const queryClient = useQueryClient();

  const path = routes.logs.list(filters);

  const {
    data: logs = [],
    isLoading: loading,
    error,
  } = useQuery<AuditLogEntry[]>({
    queryKey: logKeys.list(filters),
    queryFn: ({ signal }) => api<AuditLogEntry[]>(path, { signal }),
  });

  const refresh = useCallback(async () => {
    await queryClient.invalidateQueries({ queryKey: logKeys.all() });
  }, [queryClient]);

  return { logs, loading, error: error ? getErrorMessage(error) : null, refresh };
}

export function useLogStats() {
  const queryClient = useQueryClient();

  const {
    data: stats = DEFAULT_STATS,
    isLoading: loading,
    error,
  } = useQuery<LogStats>({
    queryKey: logKeys.stats(),
    queryFn: ({ signal }) => api<LogStats>(routes.logs.stats(), { signal }),
  });

  const refresh = useCallback(async () => {
    await queryClient.invalidateQueries({ queryKey: logKeys.stats() });
  }, [queryClient]);

  return { stats, loading, error: error ? getErrorMessage(error) : null, refresh };
}

const DEFAULT_INSIGHTS: LogInsights = {
  totalCalls: 0,
  errorCalls: 0,
  errorRate: 0,
  avgDurationMs: null,
  tools: [],
  servers: [],
};

export function useLogInsights(window?: { from?: string }) {
  const queryClient = useQueryClient();

  const {
    data: insights = DEFAULT_INSIGHTS,
    isLoading: loading,
    error,
  } = useQuery<LogInsights>({
    queryKey: logKeys.insights(window),
    queryFn: ({ signal }) => api<LogInsights>(routes.logs.insights(window), { signal }),
  });

  const refresh = useCallback(async () => {
    // 窗口参数内嵌于 key，按前缀失效以覆盖所有窗口
    await queryClient.invalidateQueries({ queryKey: ["logs", "insights"] });
  }, [queryClient]);

  return { insights, loading, error: error ? getErrorMessage(error) : null, refresh };
}
