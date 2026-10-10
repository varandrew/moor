import { useTranslation } from "@/contexts/LocaleContext";
import { cn } from "@/lib/utils";

interface EmptyStateProps {
  icon: React.ElementType;
  message: string;
  className?: string;
}

export function EmptyState({ icon: Icon, message, className }: EmptyStateProps) {
  const { text } = useTranslation();
  return (
    <div className={cn("py-10 text-center", className)}>
      <Icon className="h-8 w-8 mx-auto text-[var(--fg-15)] mb-3" />
      <p className="font-body text-sm text-[var(--fg-35)]">{text(message)}</p>
    </div>
  );
}
