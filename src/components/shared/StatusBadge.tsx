import { useTranslation } from "@/contexts/LocaleContext";
import { Badge } from "@/components/ui/badge";
import { cn } from "@/lib/utils";

const statusConfig = {
  stopped: { label: "Stopped", variant: "subtle" as const, dot: "bg-[var(--fg-25)]" },
  starting: { label: "Starting", variant: "warning" as const, dot: "bg-gold animate-pulse-dot" },
  stopping: { label: "Stopping", variant: "warning" as const, dot: "bg-gold animate-pulse-dot" },
  running: { label: "Running", variant: "success" as const, dot: "bg-success-muted" },
  error: { label: "Error", variant: "error" as const, dot: "bg-error-warm" },
};

export function StatusBadge({ status }: { status: string }) {
  const { text } = useTranslation();
  const config = statusConfig[status as keyof typeof statusConfig] || statusConfig.stopped;
  return (
    <Badge variant={config.variant}>
      <span className={cn("h-1.5 w-1.5 rounded-full mr-1.5", config.dot)} />
      {text(config.label)}
    </Badge>
  );
}
