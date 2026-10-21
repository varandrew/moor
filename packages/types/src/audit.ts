export interface AuditLogEntry {
  id: string;
  timestamp: string;
  profileId: string | null;
  serverId: string | null;
  toolName: string;
  arguments: unknown;
  result: unknown | null;
  error: string | null;
  durationMs: number | null;
  agentInfo: string | null;
}

export interface LogStats {
  totalCalls: number;
  errorCalls: number;
  errorRate: number;
  avgDurationMs: number | null;
  topTools: Array<{ toolName: string; count: number; avgDuration: number }>;
  topServers: Array<{ serverId: string; count: number }>;
}

export interface ToolInsight {
  toolName: string;
  serverId: string | null;
  serverName: string | null;
  callCount: number;
  errorCount: number;
  errorRate: number;
  avgDurationMs: number | null;
  p50Ms: number | null;
  p95Ms: number | null;
  lastCalledAt: string | null;
}

export interface ServerInsight {
  serverId: string;
  serverName: string | null;
  callCount: number;
  errorCount: number;
  errorRate: number;
  avgDurationMs: number | null;
  lastCalledAt: string | null;
}

export interface LogInsights {
  totalCalls: number;
  errorCalls: number;
  errorRate: number;
  avgDurationMs: number | null;
  /** 被风险策略拒绝的调用数。 */
  blockedCalls: number;
  tools: ToolInsight[];
  servers: ServerInsight[];
}
