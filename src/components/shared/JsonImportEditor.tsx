import { useTranslation } from "@/contexts/LocaleContext";
import { translateText } from "@/lib/messages";
import type { Locale } from "@/lib/locale";
import { EditorState } from "@codemirror/state";
import { useMemo } from "react";
import CodeMirror from "@uiw/react-codemirror";
import { json } from "@codemirror/lang-json";
import { lintGutter, linter, type Diagnostic } from "@codemirror/lint";
import { EditorView } from "@codemirror/view";
import { getJsonImportDiagnostics, type ImportDiagnostic } from "@/lib/json-import-editor";

interface JsonImportEditorProps {
  value: string;
  placeholder: string;
  diagnostics: ImportDiagnostic[];
  onChange: (value: string) => void;
}

function toCodeMirrorDiagnostic(
  content: string,
  diagnostic: ImportDiagnostic,
  locale: Locale,
): Diagnostic {
  const from = Math.min(diagnostic.offset ?? 0, content.length);
  const length = Math.max(diagnostic.length ?? 1, 1);
  return {
    from,
    to: Math.min(from + length, content.length),
    severity: "error",
    source: diagnostic.source,
    message: translateText(locale, diagnostic.code ?? diagnostic.message),
  };
}

const moorEditorTheme = EditorView.theme(
  {
    "&": {
      backgroundColor: "transparent",
      color: "#26251e",
      fontSize: "12px",
      height: "100%",
    },
    ".cm-scroller": {
      fontFamily:
        "var(--font-mono), ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace",
      lineHeight: "1.65",
      overflow: "auto",
    },
    ".cm-content": {
      caretColor: "#f54e00",
      minHeight: "260px",
      padding: "12px 0",
    },
    ".cm-line": {
      padding: "0 12px",
    },
    ".cm-gutters": {
      backgroundColor: "rgba(38, 37, 30, 0.035)",
      borderRight: "1px solid var(--fg-08)",
      color: "rgba(38, 37, 30, 0.36)",
      fontFamily:
        "var(--font-mono), ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace",
    },
    ".cm-activeLine": {
      backgroundColor: "rgba(245, 78, 0, 0.045)",
    },
    ".cm-activeLineGutter": {
      backgroundColor: "rgba(245, 78, 0, 0.07)",
      color: "#26251e",
    },
    ".cm-foldPlaceholder": {
      backgroundColor: "var(--fg-06)",
      border: "1px solid var(--fg-10)",
      color: "var(--fg-55)",
    },
    ".cm-tooltip": {
      backgroundColor: "#e6e5e0",
      border: "1px solid var(--fg-12)",
      color: "#26251e",
      fontFamily: "var(--font-headline), system-ui, sans-serif",
    },
    ".cm-tooltip-lint": {
      borderRadius: "8px",
      padding: "6px 8px",
    },
    ".cm-diagnosticText": {
      color: "#cf2d56",
    },
    ".cm-placeholder": {
      color: "var(--fg-35)",
    },
    ".cm-selectionBackground": {
      backgroundColor: "rgba(245, 78, 0, 0.16) !important",
    },
    "&.cm-focused": {
      outline: "none",
    },
  },
  { dark: false },
);

export function JsonImportEditor({
  value,
  placeholder,
  diagnostics,
  onChange,
}: JsonImportEditorProps) {
  const { t, text, locale } = useTranslation();
  const extensions = useMemo(
    () => [
      json(),
      EditorState.phrases.of(
        locale === "zh-CN"
          ? {
              Find: "查找",
              Replace: "替换",
              next: "下一个",
              previous: "上一个",
              all: "全部",
              "match case": "区分大小写",
              "by word": "按单词",
              regexp: "正则表达式",
              replace: "替换",
              "replace all": "全部替换",
              close: "关闭",
              "No diagnostics": "没有诊断问题",
              "Fold line": "折叠行",
              "Unfold line": "展开行",
              "Folded lines": "已折叠的行",
              "$1 scan warnings": "$1 条检查警告",
            }
          : {},
      ),
      linter((view) => {
        const content = view.state.doc.toString();
        return getJsonImportDiagnostics(content).map((diagnostic) =>
          toCodeMirrorDiagnostic(content, diagnostic, locale),
        );
      }),
      lintGutter(),
      moorEditorTheme,
    ],
    [locale],
  );

  return (
    <div className="overflow-hidden rounded-xl border border-[var(--fg-10)] bg-surface-200/40 focus-within:border-[var(--fg-20)]">
      <CodeMirror
        value={value}
        height="300px"
        placeholder={placeholder}
        basicSetup={{
          foldGutter: true,
          highlightActiveLine: true,
          highlightActiveLineGutter: true,
          lineNumbers: true,
        }}
        extensions={extensions}
        theme="none"
        onChange={onChange}
      />
      {diagnostics.length > 0 && (
        <div className="space-y-1 border-t border-error-warm/15 bg-error-warm/8 px-3 py-2">
          {diagnostics.map((diagnostic) => (
            <p
              key={`${diagnostic.offset ?? "root"}:${diagnostic.message}`}
              className="font-mono text-[11px] leading-relaxed text-error-warm"
            >
              {diagnostic.line && diagnostic.column
                ? t("Line {line}, Column {column}: {message}", {
                    line: diagnostic.line,
                    column: diagnostic.column,
                    message: text(diagnostic.code ?? diagnostic.message),
                  })
                : text(diagnostic.code ?? diagnostic.message)}
            </p>
          ))}
        </div>
      )}
    </div>
  );
}
