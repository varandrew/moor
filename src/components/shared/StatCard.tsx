import { useTranslation } from "@/contexts/LocaleContext";
import { Card, CardContent } from "@/components/ui/card";
import { cn } from "@/lib/utils";

interface StatCardProps {
  icon: React.ElementType;
  label: string;
  value: string | number;
  accent: string;
  delay: number;
  compact?: boolean;
}

export function StatCard({ icon: Icon, label, value, accent, delay, compact }: StatCardProps) {
  const { text } = useTranslation();
  return (
    <Card
      className={cn(
        "animate-fade-in-up",
        compact
          ? undefined
          : "hover:shadow-[0_12px_40px_-12px_rgba(0,0,0,0.06)] transition-shadow-smooth",
        `stagger-${delay}`,
      )}
    >
      <CardContent className="p-5">
        <div className={cn("flex justify-between items-start", compact ? "mb-3" : "mb-4")}>
          <div
            className={cn(
              "rounded-xl flex items-center justify-center border",
              compact ? "h-8 w-8" : "h-9 w-9",
              accent,
            )}
          >
            <Icon className={compact ? "h-4 w-4" : "h-[18px] w-[18px]"} />
          </div>
        </div>
        <p
          className={cn(
            "font-headline font-medium tracking-tight text-cursor-dark leading-none",
            compact ? "text-[28px]" : "text-[32px]",
          )}
        >
          {value}
        </p>
        <p className={cn("font-body text-[var(--fg-45)] mt-1.5", compact ? "text-xs" : "text-sm")}>
          {text(label)}
        </p>
      </CardContent>
    </Card>
  );
}
