import { useTranslation } from "@/contexts/LocaleContext";
import { Badge } from "@/components/ui/badge";

export function ToolCategoryBadge({ name }: { name: string }) {
  const { t } = useTranslation();
  const lower = name.toLowerCase();
  if (lower.includes("read") || lower.includes("get") || lower.includes("fetch")) {
    return (
      <Badge variant="subtle" className="text-[10px] bg-read/15 text-read border-read/20">
        {t("Read")}
      </Badge>
    );
  }
  if (
    lower.includes("write") ||
    lower.includes("edit") ||
    lower.includes("update") ||
    lower.includes("create")
  ) {
    return (
      <Badge variant="subtle" className="text-[10px] bg-edit/15 text-edit border-edit/20">
        {t("Edit")}
      </Badge>
    );
  }
  if (
    lower.includes("search") ||
    lower.includes("find") ||
    lower.includes("list") ||
    lower.includes("grep")
  ) {
    return (
      <Badge variant="subtle" className="text-[10px] bg-grep/15 text-grep border-grep/20">
        {t("Search")}
      </Badge>
    );
  }
  if (lower.includes("delete") || lower.includes("remove") || lower.includes("destroy")) {
    return (
      <Badge
        variant="subtle"
        className="text-[10px] bg-error-warm/10 text-error-warm border-error-warm/15"
      >
        {t("Destructive")}
      </Badge>
    );
  }
  return (
    <Badge variant="subtle" className="text-[10px]">
      {t("Tool")}
    </Badge>
  );
}
