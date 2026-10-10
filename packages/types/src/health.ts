export type ServerHealthStatus = "unknown" | "healthy" | "unhealthy" | "unsupported";

export interface ServerHealth {
  serverId: string;
  status: ServerHealthStatus;
  checkedAt: string | null;
  consecutiveFailures: number;
  errorMessage: string | null;
}

export interface UpdateCheck {
  currentVersion: string;
  latestVersion: string;
  updateAvailable: boolean;
  releaseUrl: string;
}
