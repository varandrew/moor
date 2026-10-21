import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { useTranslation } from "@/contexts/LocaleContext";
import type { ToolSelection } from "@/lib/tool-exposure";
import type { ProfileToolGroup } from "@moor/types";

/**
 * 切到策展模式前的置顶选择器。
 *
 * 只做展示与选择，写入由调用方负责：确认后先置顶再切模式，两步都由页面调度。
 */
export function PinPicker({
  groups,
  selection,
  busy,
  onToggle,
  onConfirm,
  onCancel,
}: {
  groups: ProfileToolGroup[];
  selection: ToolSelection[];
  busy: boolean;
  onToggle: (item: ToolSelection) => void;
  onConfirm: () => void;
  onCancel: () => void;
}) {
  const { t } = useTranslation();
  const selected = new Set(selection.map((item) => `${item.serverId}\u0000${item.toolName}`));

  return (
    <div className="mt-2 rounded-xl border border-[var(--fg-08)] bg-surface-100 p-4 space-y-3">
      <div>
        <p className="font-headline text-sm text-cursor-dark">{t("Pick the tools to keep")}</p>
        <p className="font-body text-xs text-[var(--fg-45)]">
          {t(
            "Only these tools are listed to agents. Everything else stays reachable through search.",
          )}
        </p>
      </div>
      <div className="space-y-2 max-h-72 overflow-y-auto pr-1">
        {groups.map((group) => (
          <div key={group.serverId} className="space-y-1">
            <p className="font-body text-[11px] uppercase tracking-wide text-[var(--fg-40)]">
              {group.serverName}
            </p>
            {group.tools
              .filter((tool) => !tool.disabled)
              .map((tool) => {
                const item = { serverId: group.serverId, toolName: tool.toolName };
                const key = `${item.serverId}\u0000${item.toolName}`;
                return (
                  <label
                    key={tool.toolName}
                    className="flex items-center gap-2 rounded-lg px-2 py-1 hover:bg-surface-300/40 cursor-pointer"
                  >
                    <Checkbox checked={selected.has(key)} onCheckedChange={() => onToggle(item)} />
                    <span className="font-mono text-xs text-cursor-dark truncate">
                      {tool.toolName}
                    </span>
                  </label>
                );
              })}
          </div>
        ))}
      </div>
      <div className="flex items-center gap-2">
        <Button size="sm" disabled={busy} onClick={onConfirm}>
          {t("Switch to curated")}
        </Button>
        <Button size="sm" variant="ghost" disabled={busy} onClick={onCancel}>
          {t("Cancel")}
        </Button>
        <span className="font-body text-xs text-[var(--fg-45)]">
          {t("{count} selected", { count: selection.length })}
        </span>
      </div>
    </div>
  );
}
