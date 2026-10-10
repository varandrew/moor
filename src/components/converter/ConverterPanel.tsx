import { ErrorBanner } from "@/components/shared/ErrorBanner";
import { useTranslation } from "@/contexts/LocaleContext";
import { useState, useCallback, useEffect } from "react";
import { useQuery } from "@tanstack/react-query";
import { api, apiPost } from "@/lib/api/client";
import { Button } from "@/components/ui/button";
import { Card, CardContent } from "@/components/ui/card";
import { Tabs } from "@/components/ui/tabs";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Checkbox } from "@/components/ui/checkbox";
import { Textarea } from "@/components/ui/textarea";
import { Label } from "@/components/ui/label";
import { CodeBlock } from "@/components/shared/CodeBlock";
import { ArrowRight, AlertTriangle, Loader2 } from "lucide-react";
import { cn, getErrorMessage } from "@/lib/utils";
import { routes } from "@/lib/api-routes";
import type { ClientSnippet, ConvertResult, Server } from "@moor/types";
import { importKeys, serverKeys } from "@/lib/query-keys";

const SOURCE_LABELS = { moor: "Moor", scan: "Scan", paste: "Paste" } as const;
type InputSource = keyof typeof SOURCE_LABELS;

export function ConverterPanel() {
  const { t } = useTranslation();
  const [inputSource, setInputSource] = useState<InputSource>("moor");
  const [targetClient, setTargetClient] = useState("claude-code");
  const [result, setResult] = useState<ConvertResult | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const [selectedIds, setSelectedIds] = useState<Set<string>>(new Set());
  const { data: servers = [] } = useQuery<Server[]>({
    queryKey: serverKeys.list(),
    queryFn: () => api<Server[]>(routes.servers.list()),
  });

  // 客户端清单由网关注册表派生（存在的客户端的唯一事实源）；加载期清空并禁用下拉
  const { data: snippets = [], isPending: clientsPending } = useQuery<ClientSnippet[]>({
    queryKey: importKeys.snippets(),
    queryFn: () => api<ClientSnippet[]>(routes.import.snippets()),
  });
  const clients = snippets.map((s) => ({ id: s.clientId, name: s.client }));
  const clientsDisabled = clientsPending || clients.length === 0;
  const [scanClient, setScanClient] = useState("claude-code");
  const [pasteClient, setPasteClient] = useState("claude-code");
  const [pasteContent, setPasteContent] = useState("");

  useEffect(() => {
    setResult(null);
    setError(null);
  }, [inputSource, targetClient, selectedIds, scanClient, pasteClient, pasteContent]);

  const toggleServer = useCallback((id: string) => {
    setSelectedIds((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  }, []);

  const handleConvert = useCallback(async () => {
    setLoading(true);
    setError(null);
    setResult(null);

    try {
      const body: Record<string, unknown> = { source: inputSource, targetClient };

      switch (inputSource) {
        case "moor":
          body.serverIds = Array.from(selectedIds);
          break;
        case "scan":
          body.sourceClient = scanClient;
          break;
        case "paste":
          body.sourceClient = pasteClient;
          body.content = pasteContent;
          break;
      }

      const res = await apiPost<ConvertResult>(routes.import.convert(), body);
      setResult(res);
    } catch (err) {
      setError(getErrorMessage(err, "Conversion failed"));
    } finally {
      setLoading(false);
    }
  }, [inputSource, targetClient, selectedIds, scanClient, pasteClient, pasteContent]);

  const canConvert = (() => {
    if (loading) return false;
    switch (inputSource) {
      case "moor":
        return selectedIds.size > 0;
      case "scan":
        return true;
      case "paste":
        return pasteContent.trim().length > 0;
    }
  })();

  return (
    <div className="grid grid-cols-1 lg:grid-cols-2 gap-6 animate-fade-in-up">
      {/* Left: Input Panel */}
      <Card className="border-[var(--fg-08)]">
        <CardContent className="p-5 space-y-4">
          <h3 className="font-headline text-sm font-medium text-cursor-dark">{t("Source")}</h3>

          <Tabs
            value={inputSource}
            onValueChange={(v) => setInputSource(v as InputSource)}
            tabs={Object.entries(SOURCE_LABELS).map(([value, label]) => ({ value, label }))}
          />

          {inputSource === "moor" && (
            <div className="space-y-2 max-h-80 overflow-y-auto">
              {servers.length === 0 ? (
                <p className="text-xs text-[var(--fg-40)]">{t("No servers available")}</p>
              ) : (
                servers.map((s) => (
                  <label
                    key={s.id}
                    className={cn(
                      "flex items-center gap-2.5 px-3 py-2.5 rounded-xl border cursor-pointer transition-all duration-150 text-sm",
                      selectedIds.has(s.id)
                        ? "border-cursor-orange/30 bg-cursor-orange/[0.04] text-cursor-dark"
                        : "border-[var(--fg-08)] text-[var(--fg-60)] hover:border-[var(--fg-15)] hover:bg-surface-300/30",
                    )}
                  >
                    <Checkbox
                      checked={selectedIds.has(s.id)}
                      onCheckedChange={() => toggleServer(s.id)}
                    />
                    <span className="font-headline">{s.name}</span>
                    <span className="text-[10px] text-[var(--fg-35)] ml-auto uppercase">
                      {s.connectionType}
                    </span>
                  </label>
                ))
              )}
            </div>
          )}

          {inputSource === "scan" && (
            <div className="space-y-3">
              <p className="text-xs text-[var(--fg-45)]">
                {t("Automatically scan local config file for the selected client")}
              </p>
              <Select value={scanClient} onValueChange={setScanClient} disabled={clientsDisabled}>
                <SelectTrigger>
                  <SelectValue placeholder={t("Select client")} />
                </SelectTrigger>
                <SelectContent>
                  {clients.map((c) => (
                    <SelectItem key={c.id} value={c.id}>
                      {c.name}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </div>
          )}

          {inputSource === "paste" && (
            <div className="space-y-3">
              <div className="flex items-center gap-3">
                <Label className="mb-0">{t("Format:")}</Label>
                <Select
                  value={pasteClient}
                  onValueChange={setPasteClient}
                  disabled={clientsDisabled}
                >
                  <SelectTrigger className="w-auto min-w-[160px]">
                    <SelectValue placeholder={t("Select client")} />
                  </SelectTrigger>
                  <SelectContent>
                    {clients.map((c) => (
                      <SelectItem key={c.id} value={c.id}>
                        {c.name}
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
              </div>
              <Textarea
                value={pasteContent}
                onChange={(e) => setPasteContent(e.target.value)}
                placeholder={t("Paste MCP configuration here...")}
                className="h-48 bg-surface-inverted text-text-inverted font-mono text-[11px] placeholder:text-text-inverted-muted border-[var(--fg-15)] focus:border-cursor-orange/30 focus:shadow-none rounded-xl"
              />
            </div>
          )}
        </CardContent>
      </Card>

      {/* Right: Output Panel */}
      <Card className="border-[var(--fg-08)]">
        <CardContent className="p-5 space-y-4">
          <h3 className="font-headline text-sm font-medium text-cursor-dark">{t("Target")}</h3>

          <Select value={targetClient} onValueChange={setTargetClient} disabled={clientsDisabled}>
            <SelectTrigger>
              <SelectValue placeholder={t("Select target client")} />
            </SelectTrigger>
            <SelectContent>
              {clients.map((c) => (
                <SelectItem key={c.id} value={c.id}>
                  {c.name}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>

          <Button
            onClick={handleConvert}
            disabled={!canConvert}
            className="w-full flex items-center justify-center gap-2"
          >
            {loading ? (
              <Loader2 className="h-4 w-4 animate-spin" />
            ) : (
              <ArrowRight className="h-4 w-4" />
            )}
            {loading ? t("Converting...") : t("Convert")}
          </Button>

          {error && <ErrorBanner message={error} />}

          {result && (
            <div className="space-y-3">
              <CodeBlock code={result.content} />

              <div className="flex items-start gap-2 text-xs text-[var(--fg-45)]">
                <span>{t("Target file:")}</span>
                <code className="font-mono text-[11px] bg-surface-300 px-1.5 py-0.5 rounded text-[var(--fg-70)]">
                  {result.targetPath}
                </code>
              </div>

              {result.warnings.length > 0 && (
                <div className="space-y-1.5">
                  {result.warnings.map((w, i) => (
                    <div
                      key={i}
                      className="flex items-start gap-2 text-xs text-[var(--fg-55)] bg-cursor-orange/5 rounded-lg px-3 py-2"
                    >
                      <AlertTriangle className="h-3.5 w-3.5 text-cursor-orange shrink-0 mt-0.5" />
                      <ErrorBanner message={w} />
                    </div>
                  ))}
                </div>
              )}
            </div>
          )}
        </CardContent>
      </Card>
    </div>
  );
}
