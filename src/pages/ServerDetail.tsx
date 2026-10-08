import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { Switch } from "@/components/ui/switch";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import { Label } from "@/components/ui/label";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { StatusBadge } from "@/components/shared/StatusBadge";
import { PageLoading } from "@/components/shared/PageLoading";
import { EmptyState } from "@/components/shared/EmptyState";
import { KeyValueTable } from "@/components/shared/KeyValueTable";
import { KeyValueEditor } from "@/components/shared/KeyValueEditor";
import { CopyButton } from "@/components/shared/CopyButton";
import { OpenLogButton } from "@/components/servers/OpenLogButton";
import { UnsavedChangesDialog } from "@/components/shared/UnsavedChangesDialog";
import { ToolCategoryBadge } from "@/components/shared/ToolCategoryBadge";
import { useParams, useNavigate } from "react-router-dom";
import { ArrowLeft, Play, Square, RefreshCw, Terminal, Pencil, X, Check } from "lucide-react";
import { useProfileGovernance } from "@/hooks/useProfileGovernance";
import { useProfiles } from "@/hooks/useProfiles";
import {
  useServerActions,
  useServerActionsState,
  useServer,
  useServerTools,
} from "@/hooks/useServers";
import { useEditSession } from "@/hooks/useEditSession";
import { findDuplicateHeaderKeys, type EditForm } from "@/lib/server-form";
import { resolveDisplayStatus } from "@/lib/server-status";
import { cn } from "@/lib/utils";
import type { ConnectionType } from "@moor/types";

interface ServerEditFieldsProps {
  form: EditForm;
  connectionType: ConnectionType;
  onChange: (form: EditForm) => void;
}

export function ServerEditFields({ form, connectionType, onChange }: ServerEditFieldsProps) {
  return (
    <>
      <div className="grid grid-cols-2 gap-4">
        <div className="space-y-1.5">
          <Label>Name</Label>
          <Input value={form.name} onChange={(e) => onChange({ ...form, name: e.target.value })} />
        </div>
        <div className="space-y-1.5">
          <Label>Connection Type</Label>
          <Input
            value={connectionType}
            readOnly
            aria-disabled="true"
            className="capitalize bg-surface-300/50 cursor-not-allowed"
          />
          <p className="text-[11px] text-[var(--fg-40)]">Type cannot be changed after creation.</p>
        </div>
      </div>
      {connectionType === "stdio" ? (
        <>
          <div className="space-y-1.5">
            <Label>Command</Label>
            <Input
              placeholder="e.g., npx -y @modelcontextprotocol/server-github"
              value={form.command}
              onChange={(e) => onChange({ ...form, command: e.target.value })}
            />
          </div>
          <div className="space-y-1.5">
            <Label>Arguments (one per line)</Label>
            <Textarea
              placeholder={"-y\n@modelcontextprotocol/server-github"}
              value={form.args}
              onChange={(e) => onChange({ ...form, args: e.target.value })}
              className="min-h-[80px] font-mono text-xs"
            />
          </div>
          <div className="space-y-1.5">
            <Label>Working Directory</Label>
            <Input
              placeholder="e.g., /path/to/project"
              value={form.workingDir}
              onChange={(e) => onChange({ ...form, workingDir: e.target.value })}
            />
          </div>
        </>
      ) : (
        <>
          <div className="space-y-1.5">
            <Label>URL</Label>
            <Input
              placeholder="e.g., http://localhost:3000/mcp"
              value={form.url}
              onChange={(e) => onChange({ ...form, url: e.target.value })}
            />
          </div>
          <div className="space-y-1.5">
            <Label>HTTP Headers</Label>
            <KeyValueEditor
              entries={form.headers}
              onChange={(headers) => onChange({ ...form, headers })}
              duplicateKeyFinder={findDuplicateHeaderKeys}
              keyLabel="Header"
              keyPlaceholder="Authorization"
              valuePlaceholder="Bearer {env:MCP_TOKEN}"
            />
          </div>
        </>
      )}
      <div className="space-y-1.5">
        <Label>Environment Variables</Label>
        <KeyValueEditor
          entries={form.env}
          onChange={(env) => onChange({ ...form, env })}
          keyLabel="Variable"
          keyPlaceholder="API_KEY"
          valuePlaceholder="your-api-key"
        />
      </div>
    </>
  );
}

export function ServerDetail() {
  const { id } = useParams<{ id: string }>();
  const navigate = useNavigate();
  const { serverActions, setServerAction, clearServerAction } = useServerActionsState();
  const { startServer, stopServer, updateServer } = useServerActions({
    setServerAction,
    clearServerAction,
  });
  const { profiles } = useProfiles();
  const activeProfile = profiles.find((profile) => profile.isActive);

  const { setTools, busy: governanceBusy } = useProfileGovernance(activeProfile?.id);

  const { server, isLoading: loading } = useServer(id);
  const { tools } = useServerTools(id, activeProfile?.id);

  const {
    isEditing,
    editForm,
    editConnectionType,
    dirty,
    saving,
    blocker,
    discardOpen,
    overwriteOpen,
    setOverwriteOpen,
    setEditForm,
    enterEdit,
    requestCancelEdit,
    saveEdit,
    confirmDiscard,
    cancelDiscard,
    confirmOverwrite,
  } = useEditSession({ server, serverId: id, updateServer });

  if (loading || !server?.id) {
    return <PageLoading message="Loading server details..." />;
  }

  const displayStatus = resolveDisplayStatus(server, serverActions[id ?? ""]);

  const handleDiscoverTools = async () => {
    if (server.status === "running") {
      await stopServer(id!);
      await startServer(id!);
    } else {
      await startServer(id!);
    }
  };

  const toggleTool = async (toolName: string, enabled: boolean) => {
    if (!activeProfile || !id) return;
    await setTools([{ serverId: id, toolName }], enabled);
  };

  const toggleAutoStart = async (value: boolean) => {
    if (!id) return;
    try {
      await updateServer({ id, updates: { autoStart: value } });
    } catch {
      // 请求失败时 SSE 会还原 Switch 状态
    }
  };

  const commandText =
    server.connectionType === "stdio"
      ? `${server.command || ""} ${server.args?.join(" ") || ""}`.trim()
      : server.url || "";

  const envEntries = server.env ? Object.entries(server.env) : [];
  const headerEntries = server.headers ? Object.entries(server.headers) : [];

  return (
    <div className="space-y-6 animate-fade-in-up">
      <UnsavedChangesDialog
        open={blocker.state === "blocked" || discardOpen}
        onCancel={cancelDiscard}
        onConfirm={confirmDiscard}
      />
      <UnsavedChangesDialog
        open={overwriteOpen}
        title="Overwrite external changes?"
        description="This server configuration changed while you were editing. Saving now will overwrite the latest saved configuration."
        actionLabel="Overwrite changes"
        onCancel={() => setOverwriteOpen(false)}
        onConfirm={confirmOverwrite}
      />

      {/* Header */}
      <div className="flex items-start gap-3">
        <Button variant="ghost" size="icon" className="mt-0.5" onClick={() => navigate("/servers")}>
          <ArrowLeft className="h-4 w-4" />
        </Button>
        <div className="flex-1 min-w-0">
          <div className="flex items-center gap-3 flex-wrap">
            <div
              className={cn(
                "h-10 w-10 rounded-xl flex items-center justify-center border shrink-0",
                displayStatus.status === "running"
                  ? "bg-success-muted/10 text-success-muted border-success-muted/20"
                  : displayStatus.status === "error"
                    ? "bg-error-warm/10 text-error-warm border-error-warm/20"
                    : "bg-surface-300 text-[var(--fg-35)] border-[var(--fg-08)]",
              )}
            >
              <Terminal className="h-[18px] w-[18px]" />
            </div>
            <div className="min-w-0">
              <div className="flex items-center gap-2.5">
                <h1 className="font-headline text-[22px] tracking-tight text-cursor-dark">
                  {server.name}
                </h1>
                <StatusBadge status={displayStatus.status} />
              </div>
              <p className="font-mono text-[11px] text-[var(--fg-40)] mt-0.5 truncate">
                {commandText || "No command configured"}
              </p>
            </div>
          </div>
        </div>
        <div className="flex items-center gap-2 shrink-0">
          {isEditing ? (
            <>
              <Button variant="outline" onClick={requestCancelEdit}>
                <X className="h-4 w-4 mr-2" /> Cancel
              </Button>
              <Button onClick={() => void saveEdit()} disabled={!dirty || saving}>
                <Check className="h-4 w-4 mr-2" /> {saving ? "Saving..." : "Save"}
              </Button>
            </>
          ) : (
            <>
              <Button variant="outline" onClick={enterEdit}>
                <Pencil className="h-4 w-4 mr-2" /> Edit
              </Button>
              {displayStatus.status === "running" ? (
                <Button variant="outline" onClick={() => stopServer(id!)}>
                  <Square className="h-4 w-4 mr-2" /> Stop
                </Button>
              ) : (
                <Button onClick={() => startServer(id!)}>
                  <Play className="h-4 w-4 mr-2" /> Start
                </Button>
              )}
            </>
          )}
        </div>
      </div>

      {/* Configuration */}
      <Card>
        <CardHeader className="pb-3">
          <CardTitle className="text-base">Configuration</CardTitle>
        </CardHeader>
        <CardContent className="space-y-4">
          {isEditing && editForm ? (
            <ServerEditFields
              form={editForm}
              connectionType={editConnectionType}
              onChange={(nextForm) => setEditForm(nextForm)}
            />
          ) : (
            <>
              {commandText && (
                <div>
                  <label className="font-headline text-xs text-[var(--fg-50)] mb-2 block uppercase tracking-wider">
                    {server.connectionType === "stdio" ? "Command" : "URL"}
                  </label>
                  <div className="bg-surface-inverted rounded-xl border border-[var(--fg-15)] p-4 relative group">
                    <pre className="font-mono text-xs text-text-inverted overflow-x-auto whitespace-pre-wrap pr-10">
                      {commandText}
                    </pre>
                    <CopyButton
                      text={commandText}
                      className="absolute top-2.5 right-2.5 text-[rgba(242,241,237,0.4)] hover:text-[rgba(242,241,237,0.8)] hover:bg-white/10"
                    />
                  </div>
                </div>
              )}

              <div className="grid grid-cols-2 gap-4">
                <div>
                  <label className="font-headline text-xs text-[var(--fg-50)] mb-1.5 block">
                    Connection Type
                  </label>
                  <Badge variant="outline" className="capitalize">
                    {server.connectionType}
                  </Badge>
                </div>
                <div className="flex items-center justify-between">
                  <div>
                    <label className="font-headline text-xs text-[var(--fg-50)] mb-1.5 block">
                      Auto Start
                    </label>
                    <p className="text-[11px] text-[var(--fg-40)]">
                      Start automatically when Moor launches
                    </p>
                  </div>
                  <Switch
                    checked={server.autoStart}
                    onCheckedChange={(v) => void toggleAutoStart(v)}
                  />
                </div>
                {server.workingDir && (
                  <div className="col-span-2">
                    <label className="font-headline text-xs text-[var(--fg-50)] mb-1.5 block">
                      Working Directory
                    </label>
                    <p className="font-mono text-xs text-[var(--fg-55)] bg-surface-300 rounded-lg px-3 py-2">
                      {server.workingDir}
                    </p>
                  </div>
                )}
              </div>

              {envEntries.length > 0 && (
                <div>
                  <label className="font-headline text-xs text-[var(--fg-50)] mb-2 block">
                    Environment Variables
                  </label>
                  <KeyValueTable entries={envEntries} />
                </div>
              )}

              {headerEntries.length > 0 && (
                <div>
                  <label className="font-headline text-xs text-[var(--fg-50)] mb-2 block">
                    HTTP Headers
                  </label>
                  <KeyValueTable entries={headerEntries} keyLabel="Header" />
                </div>
              )}
            </>
          )}

          {server.errorMessage && (
            <div className="rounded-xl bg-error-warm/8 border border-error-warm/20 p-4">
              <div className="mb-1.5 flex items-center justify-between">
                <label className="font-headline text-xs text-error-warm block">Error</label>
                <OpenLogButton serverId={server.id} />
              </div>
              <p className="font-mono text-xs text-error-warm">{server.errorMessage}</p>
            </div>
          )}
        </CardContent>
      </Card>

      {/* Tools */}
      <Card>
        <CardHeader className="pb-3">
          <div className="flex items-center justify-between">
            <div className="flex items-center gap-2">
              <CardTitle className="text-base">Discovered Tools</CardTitle>
              <Badge variant="subtle">{tools.length}</Badge>
            </div>
            <Button variant="ghost" size="sm" onClick={handleDiscoverTools}>
              <RefreshCw className="h-3.5 w-3.5 mr-1.5" /> Refresh
            </Button>
          </div>
        </CardHeader>
        <CardContent>
          {tools.length === 0 ? (
            <EmptyState
              icon={Terminal}
              message="No tools discovered. Start the server to discover tools."
            />
          ) : (
            <div className="space-y-2">
              {tools.map((tool) => (
                <div
                  key={tool.toolName}
                  className={cn(
                    "flex items-center justify-between p-4 rounded-xl border transition-all duration-200",
                    tool.disabled
                      ? "bg-surface-300/40 border-[var(--fg-06)] opacity-60"
                      : "bg-surface-100 border-[var(--fg-08)] hover:border-[var(--fg-15)]",
                  )}
                >
                  <div className="min-w-0 flex-1 mr-4">
                    <div className="flex items-center gap-2 mb-1">
                      <span className="font-mono text-[13px] font-medium text-cursor-dark">
                        {tool.exposedName}
                      </span>
                      <ToolCategoryBadge name={tool.toolName} />
                    </div>
                    {tool.description && (
                      <p className="font-body text-xs text-[var(--fg-50)] truncate">
                        {tool.description}
                      </p>
                    )}
                  </div>
                  <Switch
                    checked={!tool.disabled}
                    disabled={!activeProfile || governanceBusy}
                    onCheckedChange={(v) => toggleTool(tool.toolName, v)}
                  />
                </div>
              ))}
            </div>
          )}
        </CardContent>
      </Card>
    </div>
  );
}
