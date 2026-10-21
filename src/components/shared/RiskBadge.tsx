import { Badge } from "@/components/ui/badge";
import { useTranslation } from "@/contexts/LocaleContext";
import type { MessageKey } from "@/lib/messages";
import type { RiskLevel } from "@moor/types";

const RISK_VARIANT: Record<RiskLevel, "error" | "warning" | "success" | "subtle"> = {
  destructive: "error",
  write: "warning",
  safe: "success",
  unknown: "subtle",
};

const RISK_LABEL: Record<RiskLevel, MessageKey> = {
  destructive: "Destructive",
  write: "Write",
  safe: "Safe",
  unknown: "Unknown risk",
};

/**
 * 工具风险标记。`riskSource === "user"` 表示这是用户纠正过的结果，
 * 用省略号区分，避免把纠正值误当成又一次推断。
 */
export function RiskBadge({ level, source }: { level: RiskLevel; source: "heuristic" | "user" }) {
  const { t } = useTranslation();
  const label = `${t(RISK_LABEL[level])}${source === "user" ? " ·" : ""}`;
  return (
    <Badge
      variant={RISK_VARIANT[level]}
      className="text-[10px] shrink-0"
      title={source === "user" ? t("Corrected by you") : t("Inferred from the tool name")}
    >
      {label}
    </Badge>
  );
}
