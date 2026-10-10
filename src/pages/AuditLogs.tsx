import { useTranslation } from "@/contexts/LocaleContext";
import { useState, Fragment, useMemo, useEffect } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Badge } from "@/components/ui/badge";
import { ScrollArea } from "@/components/ui/scroll-area";
import { Tabs } from "@/components/ui/tabs";
import { useLogs, useLogInsights } from "@/hooks/useLogs";
import { useProfiles } from "@/hooks/useProfiles";
import { logKeys } from "@/lib/query-keys";
import { ErrorBanner } from "@/components/shared/ErrorBanner";
import { StatCard } from "@/components/shared/StatCard";
import { EmptyState } from "@/components/shared/EmptyState";
import { PageHeader } from "@/components/shared/PageHeader";
import {
  ERROR_RATE_ALERT,
  SLOW_P95_MS,
  formatMs,
  formatRate,
  isToolErrorProne,
  isToolSlow,
  windowFromDate,
  type InsightsWindow,
} from "@/lib/insights";
import { useNavigate } from "react-router-dom";
import {
  Search,
  ChevronDown,
  ChevronRight,
  RefreshCw,
  BarChart3,
  Activity,
  AlertTriangle,
  Clock,
  Zap,
  SlidersHorizontal,
  Server,
  ArrowUpDown,
} from "lucide-react";
import { cn } from "@/lib/utils";
import type { LogInsights, ToolInsight } from "@moor/types";

type ToolSortKey = "calls" | "errors" | "p95";

const WINDOW_OPTIONS: Array<{ value: InsightsWindow; label: string }> = [
  { value: "24h", label: "24h" },
  { value: "7d", label: "7d" },
  { value: "30d", label: "30d" },
  { value: "all", label: "All" },
];

export function AuditLogs() {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const [tab, setTab] = useState("insights");
  const [insightsWindow, setInsightsWindow] = useState<InsightsWindow>("7d");
  // from 带毫秒精度，渲染期现算会使 queryKey 每次渲染漂移 → 无限 refetch
  const insightsParams = useMemo(
    () => ({ from: windowFromDate(insightsWindow) }),
    [insightsWindow],
  );
  const { insights, loading, error: insightsError } = useLogInsights(insightsParams);

  const handleRefresh = () => {
    void queryClient.invalidateQueries({ queryKey: logKeys.all() });
  };

  return (
    <div className="space-y-8 animate-fade-in-up">
      <PageHeader
        title={t("Insights")}
        subtitle={t("Tool call health and governance signals")}
        action={
          <Button variant="outline" onClick={handleRefresh}>
            <RefreshCw className="h-4 w-4 mr-2" />
            {t("Refresh")}
          </Button>
        }
      />

      <Tabs
        value={tab}
        onValueChange={setTab}
        tabs={[
          { value: "insights", label: t("Insights") },
          { value: "entries", label: t("Log Entries") },
        ]}
      />

      {tab === "insights" ? (
        <InsightsPanel
          insights={insights}
          loading={loading}
          error={insightsError}
          window={insightsWindow}
          onWindowChange={setInsightsWindow}
        />
      ) : (
        <LogEntriesPanel />
      )}
    </div>
  );
}

function InsightsPanel({
  insights,
  loading,
  error,
  window: insightsWindow,
  onWindowChange,
}: {
  insights: LogInsights;
  loading: boolean;
  error: string | null;
  window: InsightsWindow;
  onWindowChange: (window: InsightsWindow) => void;
}) {
  const { t } = useTranslation();
  const [sortKey, setSortKey] = useState<ToolSortKey>("calls");
  const navigate = useNavigate();
  const { profiles } = useProfiles();
  const activeProfile = profiles.find((profile) => profile.isActive);

  const sortedTools = sortTools(insights.tools, sortKey);
  const alertCount = insights.tools.filter((t) => isToolErrorProne(t) || isToolSlow(t)).length;

  const jumpToTool = (tool: ToolInsight) => {
    if (!activeProfile || !tool.serverId) return;
    navigate(
      `/profiles/${activeProfile.id}?serverId=${encodeURIComponent(tool.serverId)}&toolName=${encodeURIComponent(tool.toolName)}`,
    );
  };

  return (
    <div className="space-y-6">
      {/* Window selector + sort hint */}
      <div className="flex items-center justify-between gap-3 flex-wrap">
        <Tabs
          value={insightsWindow}
          onValueChange={(value) => onWindowChange(value as InsightsWindow)}
          tabs={WINDOW_OPTIONS}
          size="sm"
        />
        {alertCount > 0 && (
          <Badge variant="error" className="text-[11px]">
            <AlertTriangle className="h-3 w-3 mr-1" />
            {t("{count} tools need attention", { count: alertCount })}
          </Badge>
        )}
      </div>

      {/* Summary cards */}
      <div className="grid grid-cols-2 lg:grid-cols-4 gap-4">
        <StatCard
          icon={Activity}
          label={t("Calls")}
          value={insights.totalCalls.toString()}
          accent="bg-[var(--fg-06)] text-[var(--fg-50)] border-[var(--fg-10)]"
          delay={1}
          compact
        />
        <StatCard
          icon={Zap}
          label={t("Error Rate")}
          value={formatRate(insights.errorRate)}
          accent={
            insights.errorRate > 0.1
              ? "bg-error-warm/10 text-error-warm border-error-warm/20"
              : "bg-success-muted/10 text-success-muted border-success-muted/20"
          }
          delay={2}
          compact
        />
        <StatCard
          icon={Clock}
          label={t("Avg Duration")}
          value={formatMs(insights.avgDurationMs)}
          accent="bg-read/10 text-read border-read/15"
          delay={3}
          compact
        />
        <StatCard
          icon={BarChart3}
          label={t("Tools Seen")}
          value={insights.tools.length.toString()}
          accent="bg-grep/10 text-grep border-grep/20"
          delay={4}
          compact
        />
      </div>

      {/* Tools table */}
      <Card>
        <CardHeader className="pb-3">
          <CardTitle className="text-base flex items-center gap-2">
            <SlidersHorizontal className="h-4 w-4 text-[var(--fg-40)]" />
            {t("Tool Health")}
          </CardTitle>
          <p className="font-body text-xs text-[var(--fg-45)]">
            {t("Red = error rate ≥ {rate} (≥5 calls) · amber = p95 ≥ {duration}", {
              rate: formatRate(ERROR_RATE_ALERT),
              duration: formatMs(SLOW_P95_MS),
            })}
          </p>
        </CardHeader>
        <CardContent>
          {loading && insights.tools.length === 0 ? (
            <EmptyState icon={BarChart3} message={t("Loading insights...")} />
          ) : error != null ? (
            <ErrorBanner message={`Failed to load insights.\n${String(error)}`} />
          ) : insights.tools.length === 0 ? (
            <EmptyState
              icon={BarChart3}
              message={t(
                "No tool calls in this window — insights appear once agents start calling tools",
              )}
            />
          ) : (
            <ScrollArea className="max-h-[520px]">
              <table className="w-full">
                <thead>
                  <tr className="border-b border-[var(--fg-08)]">
                    <th className="text-left font-headline text-[11px] text-[var(--fg-45)] uppercase tracking-wider px-4 py-2.5 font-medium">
                      {t("Tool")}
                    </th>
                    <th className="text-left font-headline text-[11px] text-[var(--fg-45)] uppercase tracking-wider px-4 py-2.5 font-medium">
                      {t("Server")}
                    </th>
                    <SortHeader
                      label={t("Calls")}
                      active={sortKey === "calls"}
                      onClick={() => setSortKey("calls")}
                    />
                    <SortHeader
                      label={t("Errors")}
                      active={sortKey === "errors"}
                      onClick={() => setSortKey("errors")}
                    />
                    <SortHeader label="p50" active={false} onClick={() => {}} muted />
                    <SortHeader
                      label="p95"
                      active={sortKey === "p95"}
                      onClick={() => setSortKey("p95")}
                    />
                    <th className="text-left font-headline text-[11px] text-[var(--fg-45)] uppercase tracking-wider px-4 py-2.5 font-medium">
                      {t("Last Called")}
                    </th>
                    <th className="px-4" />
                  </tr>
                </thead>
                <tbody>
                  {sortedTools.map((tool) => {
                    const errorProne = isToolErrorProne(tool);
                    const slow = isToolSlow(tool);
                    return (
                      <tr
                        key={`${tool.serverId ?? "none"}:${tool.toolName}`}
                        className={cn(
                          "border-b border-[var(--fg-04)] hover:bg-surface-300/40 transition-colors",
                          errorProne && "bg-error-warm/[0.04]",
                          !errorProne && slow && "bg-read/[0.04]",
                        )}
                      >
                        <td className="px-4 py-2.5">
                          <span className="font-mono text-xs text-cursor-dark">
                            {tool.toolName}
                          </span>
                        </td>
                        <td className="px-4 py-2.5">
                          <span className="font-body text-xs text-[var(--fg-45)]">
                            {tool.serverName ?? tool.serverId ?? "—"}
                          </span>
                        </td>
                        <td className="px-4 py-2.5 font-mono text-xs text-[var(--fg-60)]">
                          {tool.callCount}
                        </td>
                        <td
                          className={cn(
                            "px-4 py-2.5 font-mono text-xs",
                            errorProne ? "text-error-warm" : "text-[var(--fg-45)]",
                          )}
                        >
                          {formatRate(tool.errorRate)}
                        </td>
                        <td className="px-4 py-2.5 font-mono text-xs text-[var(--fg-45)]">
                          {formatMs(tool.p50Ms)}
                        </td>
                        <td
                          className={cn(
                            "px-4 py-2.5 font-mono text-xs",
                            slow ? "text-read" : "text-[var(--fg-45)]",
                          )}
                        >
                          {formatMs(tool.p95Ms)}
                        </td>
                        <td className="px-4 py-2.5 font-mono text-[11px] text-[var(--fg-45)]">
                          {tool.lastCalledAt
                            ? new Date(tool.lastCalledAt).toLocaleDateString(undefined, {
                                month: "short",
                                day: "numeric",
                                hour: "2-digit",
                                minute: "2-digit",
                              })
                            : "—"}
                        </td>
                        <td className="px-4 py-2.5 text-right">
                          {activeProfile && tool.serverId && (
                            <Button
                              variant="ghost"
                              size="sm"
                              className="h-7 text-xs"
                              onClick={() => jumpToTool(tool)}
                            >
                              {t("Manage")}
                            </Button>
                          )}
                        </td>
                      </tr>
                    );
                  })}
                </tbody>
              </table>
            </ScrollArea>
          )}
        </CardContent>
      </Card>

      {/* Servers summary */}
      {insights.servers.length > 0 && (
        <Card>
          <CardHeader className="pb-3">
            <CardTitle className="text-base flex items-center gap-2">
              <Server className="h-4 w-4 text-[var(--fg-40)]" />
              {t("Servers")}
            </CardTitle>
          </CardHeader>
          <CardContent>
            <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-2">
              {insights.servers.map((server) => (
                <div
                  key={server.serverId}
                  className="flex items-center justify-between gap-3 px-3 py-2.5 rounded-xl border border-[var(--fg-06)] bg-surface-100"
                >
                  <div className="min-w-0">
                    <p className="font-headline text-xs text-cursor-dark truncate">
                      {server.serverName ?? server.serverId}
                    </p>
                    <p className="font-mono text-[10px] text-[var(--fg-40)]">
                      {t("{count} calls · {duration} avg", {
                        count: server.callCount,
                        duration: formatMs(server.avgDurationMs),
                      })}
                    </p>
                  </div>
                  <Badge
                    variant={server.errorRate >= ERROR_RATE_ALERT ? "error" : "subtle"}
                    className="text-[10px] shrink-0"
                  >
                    {formatRate(server.errorRate)}
                  </Badge>
                </div>
              ))}
            </div>
          </CardContent>
        </Card>
      )}
    </div>
  );
}

function SortHeader({
  label,
  active,
  onClick,
  muted,
}: {
  label: string;
  active: boolean;
  onClick: () => void;
  muted?: boolean;
}) {
  return (
    <th className="text-left px-4 py-2.5">
      <button
        onClick={onClick}
        className={cn(
          "flex items-center gap-1 font-headline text-[11px] uppercase tracking-wider font-medium transition-colors",
          muted ? "text-[var(--fg-45)] cursor-default" : "hover:text-cursor-dark",
          active ? "text-cursor-dark" : "text-[var(--fg-45)]",
        )}
      >
        {label}
        {!muted && <ArrowUpDown className="h-3 w-3" />}
      </button>
    </th>
  );
}

function sortTools(tools: ToolInsight[], key: ToolSortKey): ToolInsight[] {
  const sorted = [...tools];
  sorted.sort((a, b) => {
    switch (key) {
      case "errors":
        return b.errorCount - a.errorCount || b.callCount - a.callCount;
      case "p95":
        return (b.p95Ms ?? -1) - (a.p95Ms ?? -1) || b.callCount - a.callCount;
      default:
        return b.callCount - a.callCount;
    }
  });
  return sorted;
}

function LogEntriesPanel() {
  const { t } = useTranslation();
  const [toolFilter, setToolFilter] = useState("");
  const [debouncedToolFilter, setDebouncedToolFilter] = useState("");
  const [expandedLog, setExpandedLog] = useState<string | null>(null);
  const { logs } = useLogs(debouncedToolFilter ? { tool_name: debouncedToolFilter } : undefined);

  useEffect(() => {
    const timer = setTimeout(() => setDebouncedToolFilter(toolFilter), 300);
    return () => clearTimeout(timer);
  }, [toolFilter]);

  return (
    <div>
      <div className="flex items-center gap-3 mb-3">
        <div className="relative flex-1 max-w-xs">
          <Search className="absolute left-3 top-1/2 -translate-y-1/2 h-4 w-4 text-[var(--fg-30)]" />
          <Input
            placeholder={t("Filter by tool name...")}
            value={toolFilter}
            onChange={(e) => setToolFilter(e.target.value)}
            className="pl-9"
          />
        </div>
        {toolFilter && (
          <Button variant="ghost" size="sm" onClick={() => setToolFilter("")}>
            {t("Clear")}
          </Button>
        )}
      </div>

      <Card>
        <ScrollArea className="max-h-[520px]">
          <table className="w-full">
            <thead>
              <tr className="border-b border-[var(--fg-08)]">
                <th className="text-left font-headline text-[11px] text-[var(--fg-45)] uppercase tracking-wider px-4 py-2.5 font-medium">
                  {t("Time")}
                </th>
                <th className="text-left font-headline text-[11px] text-[var(--fg-45)] uppercase tracking-wider px-4 py-2.5 font-medium">
                  {t("Tool")}
                </th>
                <th className="text-left font-headline text-[11px] text-[var(--fg-45)] uppercase tracking-wider px-4 py-2.5 font-medium">
                  {t("Status")}
                </th>
                <th className="text-left font-headline text-[11px] text-[var(--fg-45)] uppercase tracking-wider px-4 py-2.5 font-medium">
                  {t("Duration")}
                </th>
                <th className="text-left font-headline text-[11px] text-[var(--fg-45)] uppercase tracking-wider px-4 py-2.5 font-medium">
                  {t("Agent")}
                </th>
                <th className="w-10 px-2" />
              </tr>
            </thead>
            <tbody>
              {logs.length === 0 ? (
                <tr>
                  <td colSpan={6} className="p-0">
                    <EmptyState icon={Clock} message={t("No log entries")} />
                  </td>
                </tr>
              ) : (
                logs.map((log) => (
                  <Fragment key={log.id}>
                    <tr
                      className="border-b border-[var(--fg-04)] hover:bg-surface-300/40 cursor-pointer transition-colors"
                      onClick={() => setExpandedLog(expandedLog === log.id ? null : log.id)}
                    >
                      <td className="px-4 py-2.5 font-mono text-[11px] text-[var(--fg-45)]">
                        {new Date(log.timestamp).toLocaleTimeString()}
                      </td>
                      <td className="px-4 py-2.5 font-headline text-sm text-cursor-dark">
                        {log.toolName}
                      </td>
                      <td className="px-4 py-2.5">
                        <Badge variant={log.error ? "error" : "success"} className="text-[10px]">
                          {log.error ? t("Error") : t("Success")}
                        </Badge>
                      </td>
                      <td className="px-4 py-2.5 font-mono text-[11px] text-[var(--fg-45)]">
                        {log.durationMs ? `${log.durationMs}ms` : "—"}
                      </td>
                      <td className="px-4 py-2.5 font-body text-[11px] text-[var(--fg-35)] truncate max-w-[120px]">
                        {log.agentInfo || "—"}
                      </td>
                      <td className="px-2">
                        {expandedLog === log.id ? (
                          <ChevronDown className="h-3.5 w-3.5 text-[var(--fg-35)]" />
                        ) : (
                          <ChevronRight className="h-3.5 w-3.5 text-[var(--fg-35)]" />
                        )}
                      </td>
                    </tr>
                    {expandedLog === log.id && (
                      <tr>
                        <td colSpan={6} className="px-4 py-4 bg-surface-300/30 animate-fade-in">
                          <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
                            {log.arguments != null && (
                              <div>
                                <p className="font-headline text-[11px] text-[var(--fg-50)] mb-1.5 uppercase tracking-wider">
                                  {t("Arguments")}
                                </p>
                                <pre className="font-mono text-[11px] bg-surface-100 rounded-xl p-3 text-[var(--fg-70)] overflow-auto max-h-40 border border-[var(--fg-06)]">
                                  {JSON.stringify(log.arguments, null, 2)}
                                </pre>
                              </div>
                            )}
                            {(log.result || log.error) && (
                              <div>
                                <p className="font-headline text-[11px] text-[var(--fg-50)] mb-1.5 uppercase tracking-wider">
                                  {log.error ? t("Error") : t("Result")}
                                </p>
                                <pre
                                  className={cn(
                                    "font-mono text-[11px] rounded-xl p-3 overflow-auto max-h-40 border",
                                    log.error
                                      ? "bg-error-warm/5 text-error-warm border-error-warm/10"
                                      : "bg-surface-100 text-[var(--fg-70)] border-[var(--fg-06)]",
                                  )}
                                >
                                  {JSON.stringify(log.error || log.result, null, 2)}
                                </pre>
                              </div>
                            )}
                          </div>
                        </td>
                      </tr>
                    )}
                  </Fragment>
                ))
              )}
            </tbody>
          </table>
        </ScrollArea>
      </Card>
    </div>
  );
}
