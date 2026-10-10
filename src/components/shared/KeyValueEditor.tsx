import { useTranslation } from "@/contexts/LocaleContext";
import { useCallback, useEffect, useId, useRef, useState } from "react";
import { Input } from "@/components/ui/input";
import { Button } from "@/components/ui/button";
import { Plus, X } from "lucide-react";
import { findDuplicateKeys } from "@/lib/server-form";
import { cn } from "@/lib/utils";

interface KeyValueEditorProps {
  entries: Array<[string, string]>;
  onChange: (entries: Array<[string, string]>) => void;
  duplicateKeyFinder?: (entries: Array<[string, string]>) => Set<number>;
  keyPlaceholder?: string;
  valuePlaceholder?: string;
  keyLabel?: string;
  valueLabel?: string;
  disabled?: boolean;
}

export function KeyValueEditor({
  entries,
  onChange,
  duplicateKeyFinder = findDuplicateKeys,
  keyPlaceholder = "Key",
  valuePlaceholder = "Value",
  keyLabel = "Key",
  valueLabel = "Value",
  disabled = false,
}: KeyValueEditorProps) {
  const { t, text } = useTranslation();
  const reactId = useId();
  const duplicateErrorId = `${reactId}-duplicate-error`;
  const nextRowIdRef = useRef(entries.length);
  const [rowIds, setRowIds] = useState(() => entries.map((_, index) => `kv-row-${index + 1}`));
  const createRowId = useCallback(() => {
    nextRowIdRef.current += 1;
    return `kv-row-${nextRowIdRef.current}`;
  }, []);

  useEffect(() => {
    setRowIds((current) => {
      if (current.length === entries.length) return current;
      if (current.length > entries.length) return current.slice(0, entries.length);

      const next = [...current];
      while (next.length < entries.length) {
        next.push(createRowId());
      }
      return next;
    });
  }, [createRowId, entries.length]);

  const duplicateIndexes = duplicateKeyFinder(entries);

  const update = (index: number, field: 0 | 1, value: string) => {
    const next = entries.map(([k, v], i) =>
      i === index
        ? ((field === 0 ? [value, v] : [k, value]) as [string, string])
        : ([k, v] as [string, string]),
    );
    onChange(next);
  };

  const remove = (index: number) => {
    setRowIds((current) => current.filter((_, i) => i !== index));
    onChange(entries.filter((_, i) => i !== index));
  };

  const add = () => {
    setRowIds((current) => [...current, createRowId()]);
    onChange([...entries, ["", ""]]);
  };

  return (
    <div className="space-y-2">
      <div className="grid grid-cols-[1fr_1fr_36px] gap-2 items-center">
        <span className="font-headline text-[11px] text-[var(--fg-45)] uppercase tracking-wider px-1">
          {text(keyLabel)}
        </span>
        <span className="font-headline text-[11px] text-[var(--fg-45)] uppercase tracking-wider px-1">
          {text(valueLabel)}
        </span>
        <span />
      </div>
      {entries.map(([key, value], index) => {
        const duplicated = duplicateIndexes.has(index);
        return (
          <div
            key={rowIds[index] ?? `${reactId}-pending-${index}`}
            className="grid grid-cols-[1fr_1fr_36px] gap-2 items-center"
          >
            <Input
              placeholder={text(keyPlaceholder)}
              value={key}
              onChange={(e) => update(index, 0, e.target.value)}
              disabled={disabled}
              aria-invalid={duplicated || undefined}
              aria-describedby={duplicated ? duplicateErrorId : undefined}
              className={cn("h-9 text-xs font-mono", duplicated && "border-error-warm")}
            />
            <Input
              placeholder={text(valuePlaceholder)}
              value={value}
              onChange={(e) => update(index, 1, e.target.value)}
              disabled={disabled}
              className="h-9 text-xs font-mono"
            />
            <Button
              type="button"
              variant="ghost"
              size="icon"
              className="h-9 w-9 text-[var(--fg-40)] hover:text-error-warm shrink-0"
              onClick={() => remove(index)}
              disabled={disabled}
              aria-label={t("Remove {name} row", { name: text(keyLabel).toLowerCase() })}
            >
              <X className="h-4 w-4" />
            </Button>
          </div>
        );
      })}
      {duplicateIndexes.size > 0 && (
        <p id={duplicateErrorId} className="px-1 text-xs text-error-warm">
          {t("{label} keys must be unique.", { label: text(keyLabel) })}
        </p>
      )}
      <Button
        type="button"
        variant="outline"
        size="sm"
        className="text-[var(--fg-55)] hover:text-cursor-dark"
        onClick={add}
        disabled={disabled}
      >
        <Plus className="h-3.5 w-3.5 mr-1.5" />
        {t("Add")}
      </Button>
    </div>
  );
}
