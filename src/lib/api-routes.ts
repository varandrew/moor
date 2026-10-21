function pathSegment(value: string): string {
  return encodeURIComponent(value);
}

type QueryValue = string | number | boolean | null | undefined;

function withQuery(path: string, params: Record<string, QueryValue>): string {
  const query = new URLSearchParams();
  for (const [key, value] of Object.entries(params)) {
    if (value !== undefined && value !== null && String(value).length > 0) {
      query.set(key, String(value));
    }
  }
  const qs = query.toString();
  return qs ? `${path}?${qs}` : path;
}

type LogListParams = {
  server_id?: string;
  tool_name?: string;
  from?: string;
  to?: string;
  limit?: string | number;
  offset?: string | number;
};

export const routes = {
  servers: {
    list: () => "/api/servers",
    detail: (id: string) => `/api/servers/${pathSegment(id)}`,
    create: () => "/api/servers",
    update: (id: string) => `/api/servers/${pathSegment(id)}`,
    delete: (id: string) => `/api/servers/${pathSegment(id)}`,
    start: (id: string) => `/api/servers/${pathSegment(id)}/start`,
    stop: (id: string) => `/api/servers/${pathSegment(id)}/stop`,
    order: () => "/api/servers/order",
    tools: (id: string, profileId?: string) =>
      withQuery(`/api/servers/${pathSegment(id)}/tools`, { profile_id: profileId }),
    toolRisk: (id: string, toolName: string) =>
      `/api/servers/${pathSegment(id)}/tools/${pathSegment(toolName)}/risk`,
  },
  profiles: {
    list: () => "/api/profiles",
    detail: (id: string) => `/api/profiles/${pathSegment(id)}`,
    create: () => "/api/profiles",
    update: (id: string) => `/api/profiles/${pathSegment(id)}`,
    delete: (id: string) => `/api/profiles/${pathSegment(id)}`,
    activate: (id: string) => `/api/profiles/${pathSegment(id)}/activate`,
    updateServer: (profileId: string, serverId: string) =>
      `/api/profiles/${pathSegment(profileId)}/servers/${pathSegment(serverId)}`,
    tools: (id: string) => `/api/profiles/${pathSegment(id)}/tools`,
    clone: (id: string) => `/api/profiles/${pathSegment(id)}/clone`,
    bulkServerState: (id: string) => `/api/profiles/${pathSegment(id)}/servers-state`,
  },
  settings: {
    get: () => "/api/settings",
    update: () => "/api/settings",
    reset: () => "/api/settings/reset",
  },
  logs: {
    list: (params?: LogListParams) => withQuery("/api/logs", params ?? {}),
    stats: () => "/api/logs/stats",
    insights: (params?: { from?: string; to?: string }) =>
      withQuery("/api/logs/insights", params ?? {}),
  },
  import: {
    scan: () => "/api/import/scan",
    parse: () => "/api/import/parse",
    execute: () => "/api/import/execute",
    snippets: () => "/api/import/snippets",
    convert: () => "/api/import/convert",
  },
  events: () => "/api/events",
  health: () => "/api/health",
} as const;
