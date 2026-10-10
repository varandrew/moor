import { useTranslation } from "@/contexts/LocaleContext";
import { getErrorMessage } from "@/lib/utils";
import { useMemo, useRef, useState } from "react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Label } from "@/components/ui/label";
import { X } from "lucide-react";
import { ErrorBanner } from "@/components/shared/ErrorBanner";
import { Switch } from "@/components/ui/switch";
import { KeyValueEditor } from "@/components/shared/KeyValueEditor";
import { UnsavedChangesDialog } from "@/components/shared/UnsavedChangesDialog";
import {
  argsToArrayOrUndefined,
  entriesToRecordOrUndefined,
  findDuplicateHeaderKeys,
  findDuplicateKeys,
  headerEntriesToRecordOrUndefined,
} from "@/lib/server-form";

const CONNECTION_TYPES = [
  { value: "stdio", label: "stdio" },
  { value: "http", label: "HTTP/SSE" },
] as const;

function createInitialForm() {
  return {
    name: "",
    connectionType: "stdio" as "stdio" | "http",
    command: "",
    args: "",
    url: "",
    env: [] as Array<[string, string]>,
    headers: [] as Array<[string, string]>,
    autoStart: false,
  };
}

interface AddServerFormProps {
  onAdd: (config: {
    name: string;
    connectionType: "stdio" | "http";
    command?: string;
    args?: string[];
    url?: string;
    env?: Record<string, string>;
    headers?: Record<string, string>;
    autoStart?: boolean;
  }) => Promise<void>;
  onClose: () => void;
}

export function AddServerForm({ onAdd, onClose }: AddServerFormProps) {
  const { t } = useTranslation();
  const [form, setForm] = useState(createInitialForm);
  const [submitting, setSubmitting] = useState(false);
  const [formError, setFormError] = useState<string | null>(null);
  const [discardOpen, setDiscardOpen] = useState(false);
  const baselineRef = useRef(createInitialForm());
  const dirty = useMemo(() => JSON.stringify(form) !== JSON.stringify(baselineRef.current), [form]);

  const requestClose = () => {
    if (dirty) {
      setDiscardOpen(true);
      return;
    }
    onClose();
  };

  const handleSubmit = async () => {
    if (!form.name.trim() || submitting) return;
    setFormError(null);

    if (findDuplicateKeys(form.env).size > 0) {
      setFormError("Environment variable keys must be unique.");
      return;
    }
    if (form.connectionType === "http" && findDuplicateHeaderKeys(form.headers).size > 0) {
      setFormError("HTTP header keys must be unique.");
      return;
    }

    setSubmitting(true);
    try {
      const env = entriesToRecordOrUndefined(form.env);

      const baseConfig = {
        name: form.name.trim(),
        connectionType: form.connectionType,
        autoStart: form.autoStart,
        env,
      };

      await onAdd(
        form.connectionType === "stdio"
          ? {
              ...baseConfig,
              connectionType: "stdio",
              command: form.command.trim(),
              args: argsToArrayOrUndefined(form.args),
            }
          : {
              ...baseConfig,
              connectionType: "http",
              url: form.url.trim(),
              headers: headerEntriesToRecordOrUndefined(form.headers),
            },
      );
      onClose();
    } catch (err) {
      setFormError(getErrorMessage(err, "Failed to add server"));
    } finally {
      setSubmitting(false);
    }
  };

  return (
    <Card className="animate-scale-in border-cursor-orange/20 shadow-[0_8px_30px_rgba(245,78,0,0.04)]">
      <UnsavedChangesDialog
        open={discardOpen}
        onCancel={() => setDiscardOpen(false)}
        onConfirm={onClose}
      />
      <CardHeader className="pb-3">
        <div className="flex items-center justify-between">
          <CardTitle className="text-base">{t("Add New Server")}</CardTitle>
          <Button
            variant="ghost"
            size="icon"
            className="text-[var(--fg-65)] hover:text-cursor-dark hover:bg-[var(--fg-08)]"
            onClick={requestClose}
          >
            <X className="h-5 w-5" />
          </Button>
        </div>
      </CardHeader>
      <CardContent className="space-y-4">
        <div className="grid grid-cols-2 gap-4">
          <div className="space-y-1.5">
            <Label>{t("Name")}</Label>
            <Input
              placeholder={t("e.g., github")}
              value={form.name}
              onChange={(e) => setForm((f) => ({ ...f, name: e.target.value }))}
            />
          </div>
          <div className="space-y-1.5">
            <Label>{t("Type")}</Label>
            <Select
              value={form.connectionType}
              onValueChange={(value) =>
                setForm((f) => ({ ...f, connectionType: value as "stdio" | "http" }))
              }
            >
              <SelectTrigger>
                <SelectValue placeholder={t("Select type")} />
              </SelectTrigger>
              <SelectContent>
                {CONNECTION_TYPES.map((t) => (
                  <SelectItem key={t.value} value={t.value}>
                    {t.label}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </div>
        </div>
        {form.connectionType === "stdio" ? (
          <>
            <div className="space-y-1.5">
              <Label>{t("Command")}</Label>
              <Input
                placeholder={t("e.g., npx -y @modelcontextprotocol/server-github")}
                value={form.command}
                onChange={(e) => setForm((f) => ({ ...f, command: e.target.value }))}
              />
            </div>
            <div className="space-y-1.5">
              <Label>{t("Arguments (one per line)")}</Label>
              <Textarea
                placeholder={"-y\n@modelcontextprotocol/server-github"}
                value={form.args}
                onChange={(e) => setForm((f) => ({ ...f, args: e.target.value }))}
                className="min-h-[80px] font-mono text-xs"
              />
            </div>
          </>
        ) : (
          <>
            <div className="space-y-1.5">
              <Label>{t("URL")}</Label>
              <Input
                placeholder={t("e.g., http://localhost:3000/mcp")}
                value={form.url}
                onChange={(e) => setForm((f) => ({ ...f, url: e.target.value }))}
              />
            </div>
            <div className="space-y-1.5">
              <Label>{t("HTTP Headers")}</Label>
              <KeyValueEditor
                entries={form.headers}
                onChange={(headers) => setForm((f) => ({ ...f, headers }))}
                duplicateKeyFinder={findDuplicateHeaderKeys}
                keyLabel={t("Header")}
                keyPlaceholder="Authorization"
                valuePlaceholder="Bearer {env:MCP_TOKEN}"
              />
            </div>
          </>
        )}
        <div className="flex items-center justify-between py-2">
          <div className="space-y-0.5">
            <Label>{t("Auto Start")}</Label>
            <p className="text-[11px] text-[var(--fg-40)]">
              {t("Automatically start this server when Moor launches")}
            </p>
          </div>
          <Switch
            checked={form.autoStart}
            onCheckedChange={(v) => setForm((f) => ({ ...f, autoStart: v }))}
          />
        </div>
        <div className="space-y-1.5">
          <Label>{t("Environment Variables")}</Label>
          <KeyValueEditor
            entries={form.env}
            onChange={(env) => setForm((f) => ({ ...f, env }))}
            keyLabel={t("Variable")}
            keyPlaceholder="API_KEY"
            valuePlaceholder={t("your-api-key")}
          />
        </div>
        {formError && <ErrorBanner message={formError} />}
        <div className="flex justify-end gap-2 pt-1">
          <Button variant="outline" onClick={requestClose}>
            {t("Cancel")}
          </Button>
          <Button onClick={handleSubmit} disabled={!form.name.trim() || submitting}>
            {t("Add Server")}
          </Button>
        </div>
      </CardContent>
    </Card>
  );
}
