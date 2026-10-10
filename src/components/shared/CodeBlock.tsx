import { useTranslation } from "@/contexts/LocaleContext";
import { CopyButton } from "./CopyButton";

interface CodeBlockProps {
  code: string;
  label?: string;
}

export function CodeBlock({ code, label }: CodeBlockProps) {
  const { text } = useTranslation();
  return (
    <div className="relative group">
      {label && (
        <p className="font-headline text-[11px] text-[var(--fg-50)] mb-1.5 uppercase tracking-wider">
          {text(label)}
        </p>
      )}
      <div className="bg-surface-inverted rounded-xl border border-[var(--fg-15)] p-4 relative overflow-hidden">
        <pre className="font-mono text-[12px] leading-relaxed text-text-inverted overflow-x-auto whitespace-pre-wrap pr-10">
          {code}
        </pre>
        <CopyButton
          text={code}
          className="absolute top-2 right-2 text-text-inverted-muted hover:text-text-inverted hover:bg-white/10 opacity-0 group-hover:opacity-100 transition-opacity"
        />
      </div>
    </div>
  );
}
