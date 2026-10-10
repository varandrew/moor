import { useTranslation } from "@/contexts/LocaleContext";
interface KeyValueTableProps {
  entries: Array<[string, string]>;
  keyLabel?: string;
  valueLabel?: string;
}

export function KeyValueTable({
  entries,
  keyLabel = "Key",
  valueLabel = "Value",
}: KeyValueTableProps) {
  const { text } = useTranslation();
  return (
    <div className="rounded-xl border border-[var(--fg-08)] overflow-hidden">
      <table className="w-full text-sm">
        <thead>
          <tr className="border-b border-[var(--fg-08)] bg-surface-300/30">
            <th className="text-left font-headline text-[11px] text-[var(--fg-45)] uppercase tracking-wider px-4 py-2 font-medium">
              {text(keyLabel)}
            </th>
            <th className="text-left font-headline text-[11px] text-[var(--fg-45)] uppercase tracking-wider px-4 py-2 font-medium">
              {text(valueLabel)}
            </th>
          </tr>
        </thead>
        <tbody>
          {entries.map(([key, value]) => (
            <tr key={key} className="border-b border-[var(--fg-04)] last:border-b-0">
              <td className="px-4 py-2 font-mono text-xs text-[var(--fg-55)] whitespace-nowrap">
                {key}
              </td>
              <td className="px-4 py-2 font-mono text-xs text-cursor-dark truncate max-w-[300px]">
                {value || "—"}
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}
