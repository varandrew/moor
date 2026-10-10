import { ErrorBanner } from "@/components/shared/ErrorBanner";
import { useTranslation } from "@/contexts/LocaleContext";
import { lazy, Suspense } from "react";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Checkbox } from "@/components/ui/checkbox";
import { AlertTriangle, Check, FileJson, WandSparkles, X } from "lucide-react";
import { cn } from "@/lib/utils";
import type { useConfigImport } from "@/hooks/useConfigImport";

const JsonImportEditor = lazy(() =>
  import("@/components/shared/JsonImportEditor").then((module) => ({
    default: module.JsonImportEditor,
  })),
);

const JSON_IMPORT_PLACEHOLDER = `{
  "mcpServers": {
    "my-server": {
      "command": "npx",
      "args": ["-y", "my-mcp-server"]
    }
  }
}`;

type ImportState = ReturnType<typeof useConfigImport>;

interface ConfigImportPanelProps {
  state: ImportState;
  showJsonImport: boolean;
  onCloseJsonImport: () => void;
}

export function ConfigImportPanel({
  state,
  showJsonImport,
  onCloseJsonImport,
}: ConfigImportPanelProps) {
  const { t, text } = useTranslation();
  const {
    scanCandidates,
    selectedImports,
    scanStatus,
    importPreview,
    hasStaticAuthorizationHeader,
    jsonImport,
    jsonImportErrors,
    jsonImportStatus,
    jsonImportDiagnostics,
    jsonImportStatusIsError,
    updateJsonImport,
    formatJson,
    parseJson,
    executeImport,
    importPending,
    toggleImport,
    clearScan,
  } = state;

  return (
    <>
      {/* JSON Import Form */}
      {showJsonImport && (
        <Card className="animate-scale-in border-[var(--fg-08)]">
          <CardHeader className="pb-3">
            <div className="flex items-center justify-between">
              <CardTitle className="text-base">{t("Import MCP JSON")}</CardTitle>
              <Button
                variant="ghost"
                size="icon"
                className="text-[var(--fg-65)] hover:text-cursor-dark hover:bg-[var(--fg-08)]"
                onClick={onCloseJsonImport}
              >
                <X className="h-5 w-5" />
              </Button>
            </div>
          </CardHeader>
          <CardContent className="space-y-4">
            <div className="flex items-center justify-between gap-3">
              <div className="flex items-center gap-2">
                <FileJson className="h-4 w-4 text-[var(--fg-50)]" />
                <span className="font-mono text-[11px] text-[var(--fg-48)]">JSONC</span>
              </div>
              <Button
                variant="outline"
                size="sm"
                onClick={formatJson}
                disabled={!jsonImport.trim()}
              >
                <WandSparkles className="h-3.5 w-3.5 mr-2" />
                {t("Format JSON")}
              </Button>
            </div>
            <Suspense
              fallback={
                <div className="h-[300px] rounded-xl border border-[var(--fg-10)] bg-surface-200/40" />
              }
            >
              <JsonImportEditor
                value={jsonImport}
                placeholder={JSON_IMPORT_PLACEHOLDER}
                diagnostics={jsonImportDiagnostics}
                onChange={updateJsonImport}
              />
            </Suspense>
            {jsonImportStatus && (
              <div
                className={cn(
                  "flex items-center gap-2 rounded-lg border px-3 py-2",
                  jsonImportStatusIsError
                    ? "border-error-warm/20 bg-error-warm/8"
                    : "border-[var(--fg-08)] bg-surface-300/40",
                )}
              >
                {jsonImportStatusIsError ? (
                  <AlertTriangle className="h-4 w-4 text-error-warm" />
                ) : (
                  <Check className="h-4 w-4 text-success-muted" />
                )}
                <p className="font-body text-xs text-[var(--fg-55)]">{text(jsonImportStatus)}</p>
              </div>
            )}
            {jsonImportErrors.length > 0 && (
              <div className="rounded-lg border border-error-warm/20 bg-error-warm/8 px-3 py-2">
                {jsonImportErrors.map((error) => (
                  <ErrorBanner key={error} message={error} />
                ))}
              </div>
            )}
            <div className="flex justify-end gap-2">
              <Button variant="outline" onClick={onCloseJsonImport}>
                {t("Cancel")}
              </Button>
              <Button
                onClick={() => void parseJson()}
                disabled={!jsonImport.trim() || jsonImportDiagnostics.length > 0}
              >
                {t("Preview Import")}
              </Button>
            </div>
          </CardContent>
        </Card>
      )}

      {/* Config Import */}
      {(scanCandidates.length > 0 || scanStatus) && (
        <Card className="animate-scale-in">
          <CardHeader className="pb-3">
            <div className="flex items-center justify-between">
              <CardTitle className="text-base">{t("Config Import")}</CardTitle>
              <Button
                variant="ghost"
                size="icon"
                className="text-[var(--fg-65)] hover:text-cursor-dark hover:bg-[var(--fg-08)]"
                onClick={clearScan}
              >
                <X className="h-5 w-5" />
              </Button>
            </div>
          </CardHeader>
          <CardContent className="space-y-3">
            {scanStatus && (
              <div className="flex items-center gap-2 py-2">
                <Check className="h-4 w-4 text-success-muted" />
                <div className="font-body text-sm text-[var(--fg-55)]">
                  {typeof scanStatus === "string" ? (
                    <ErrorBanner message={scanStatus} />
                  ) : (
                    t(scanStatus.key, scanStatus.params)
                  )}
                </div>
              </div>
            )}
            {hasStaticAuthorizationHeader && (
              <div className="flex items-start gap-2 rounded-lg border border-gold/20 bg-gold/8 px-3 py-2">
                <AlertTriangle className="h-4 w-4 text-gold shrink-0 mt-0.5" />
                <p className="font-body text-xs leading-relaxed text-[var(--fg-55)]">
                  {t(
                    "Static Authorization headers are stored in Moor's local SQLite database. Prefer",
                  )}{" "}
                  <code className="font-mono">{"{env:VAR_NAME}"}</code>
                  {t("when possible.")}
                </p>
              </div>
            )}
            {importPreview && importPreview.errors.length > 0 && (
              <div className="rounded-lg border border-error-warm/20 bg-error-warm/8 px-3 py-2">
                {importPreview.errors.map((error) => (
                  <ErrorBanner key={error} message={error} />
                ))}
              </div>
            )}
            {importPreview && importPreview.unsupported.length > 0 && (
              <div className="rounded-lg border border-[var(--fg-08)] bg-surface-300/40 px-3 py-2">
                <p className="font-headline text-xs text-[var(--fg-55)] mb-1.5">
                  {t("Unsupported ({count})", { count: importPreview.unsupported.length })}
                </p>
                <div className="space-y-1">
                  {importPreview.unsupported.map((server) => (
                    <p
                      key={`${server.source}:${server.name}:${server.reason}`}
                      className="font-body text-xs text-[var(--fg-48)]"
                    >
                      <span className="font-mono text-[var(--fg-62)]">{server.name}</span>:{" "}
                      {server.reason}
                    </p>
                  ))}
                </div>
              </div>
            )}
            {importPreview && importPreview.duplicates.length > 0 && (
              <div className="rounded-lg border border-[var(--fg-08)] bg-surface-300/30 px-3 py-2">
                <p className="font-body text-xs text-[var(--fg-48)]">
                  {t("Skipping {count} duplicate servers by name.", {
                    count: importPreview.duplicates.length,
                  })}
                </p>
              </div>
            )}
            {scanCandidates.map((server) => (
              <label
                key={`${server.source}:${server.name}`}
                className="flex items-center justify-between py-2.5 px-3 rounded-xl hover:bg-surface-300/50 transition-colors cursor-pointer border border-transparent hover:border-[var(--fg-06)]"
              >
                <div className="min-w-0">
                  <p className="font-headline text-sm text-cursor-dark">{server.name}</p>
                  <p className="font-mono text-[11px] text-[var(--fg-45)] truncate">
                    {server.connectionType === "stdio"
                      ? [server.command, ...(server.args ?? [])].filter(Boolean).join(" ")
                      : server.url}
                  </p>
                </div>
                <Checkbox
                  checked={selectedImports.has(server.name)}
                  onCheckedChange={(checked) => toggleImport(server.name, checked === true)}
                />
              </label>
            ))}
            {scanCandidates.length > 0 && (
              <div className="flex justify-end pt-1">
                <Button
                  onClick={() => void executeImport()}
                  disabled={selectedImports.size === 0 || importPending}
                >
                  {t("Import Selected ({count})", { count: selectedImports.size })}
                </Button>
              </div>
            )}
          </CardContent>
        </Card>
      )}
    </>
  );
}
