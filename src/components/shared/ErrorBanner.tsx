import { useTranslation } from "@/contexts/LocaleContext";
import { messages } from "@/lib/messages";
import { AlertTriangle } from "lucide-react";
import { cn } from "@/lib/utils";
import type { ReactNode } from "react";

interface ErrorBannerProps {
  message: string;
  variant?: "default" | "mono";
  className?: string;
  action?: ReactNode;
}

export function ErrorBanner({ message, variant = "default", className, action }: ErrorBannerProps) {
  const { t, text, locale } = useTranslation();
  const [summary, ...details] = message.split("\n");
  const known = Object.prototype.hasOwnProperty.call(messages, summary);
  const display = known
    ? text(summary)
    : locale === "en"
      ? summary
      : t("Moor could not complete this operation.");
  const diagnostic = known ? details.join("\n") : locale === "en" ? details.join("\n") : message;
  return (
    <div
      role="alert"
      className={cn(
        "flex items-center gap-2 rounded-lg border border-error-warm/20 bg-error-warm/8 px-3 py-2",
        className,
      )}
    >
      <AlertTriangle className="h-4 w-4 shrink-0 text-error-warm" />
      <div
        className={cn(
          "min-w-0 text-error-warm",
          variant === "mono" ? "truncate font-mono text-[11px]" : "break-words font-body text-xs",
        )}
        title={message}
      >
        {display}
        {diagnostic && (
          <details className="mt-1 whitespace-pre-wrap font-mono text-[11px]">
            <summary className="cursor-pointer font-body">{t("Diagnostic details")}</summary>
            {diagnostic}
          </details>
        )}
      </div>
      {action && <div className="ml-auto shrink-0">{action}</div>}
    </div>
  );
}
