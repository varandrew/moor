import type { MessageKey } from "@/lib/messages";
import { getErrorMessage } from "@/lib/utils";
import { useCallback, useMemo, useState } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { apiPost } from "@/lib/api/client";
import { routes } from "@/lib/api-routes";
import { serverKeys } from "@/lib/query-keys";
import { useLockedMutation } from "@/hooks/useLockedMutation";
import { formatJsonImport, getJsonImportDiagnostics } from "@/lib/json-import-editor";
import type { ScannedServer, ImportPreview as ImportPreviewType } from "@moor/types";

type ImportPreview = ImportPreviewType;

export function useConfigImport() {
  const queryClient = useQueryClient();

  const [scanCandidates, setScanCandidates] = useState<ScannedServer[]>([]);
  const [selectedImports, setSelectedImports] = useState<Set<string>>(new Set());
  const [scanStatus, setScanStatus] = useState<
    string | { key: MessageKey; params: Record<string, number> } | null
  >(null);
  const [importPreview, setImportPreview] = useState<ImportPreview | null>(null);

  const [jsonImport, setJsonImport] = useState("");
  const [jsonImportErrors, setJsonImportErrors] = useState<string[]>([]);
  const [jsonImportStatus, setJsonImportStatus] = useState<string | null>(null);
  const jsonImportDiagnostics = useMemo(() => getJsonImportDiagnostics(jsonImport), [jsonImport]);

  const applyImportPreview = useCallback((result: ImportPreview) => {
    setImportPreview(result);
    setScanCandidates(result.servers);
    setSelectedImports(new Set(result.servers.map((server) => server.name)));
    setScanStatus(
      result.newServers === 0
        ? {
            key: "Scanned {count} configs. No new servers found.",
            params: { count: result.scanned },
          }
        : null,
    );
  }, []);

  const scan = useCallback(async () => {
    try {
      const result = await apiPost<ImportPreview>(routes.import.scan(), {});
      applyImportPreview(result);
    } catch (err) {
      setScanStatus(getErrorMessage(err));
    }
  }, [applyImportPreview]);

  const updateJsonImport = useCallback((value: string) => {
    setJsonImport(value);
    setJsonImportErrors([]);
    setJsonImportStatus(null);
  }, []);

  const formatJson = useCallback(() => {
    const result = formatJsonImport(jsonImport);
    setJsonImportErrors([]);

    if (result.diagnostics.length > 0) {
      setJsonImportStatus("Fix JSON syntax errors before formatting.");
      return;
    }

    setJsonImport(result.value);
    setJsonImportStatus(result.formatted ? "JSON formatted." : "JSON is already formatted.");
  }, [jsonImport]);

  const parseJson = useCallback(async () => {
    if (jsonImportDiagnostics.length > 0) {
      setJsonImportErrors([]);
      setJsonImportStatus("Fix JSON syntax errors before previewing.");
      return;
    }

    try {
      const result = await apiPost<ImportPreview>(routes.import.parse(), { content: jsonImport });
      if (result.errors.length > 0 || (result.diagnostics?.length ?? 0) > 0) {
        setImportPreview(result);
        setScanCandidates([]);
        setSelectedImports(new Set());
        setJsonImportErrors(result.errors);
        setJsonImportStatus(null);
        return;
      }

      applyImportPreview(result);
      setJsonImportErrors([]);
      setJsonImportStatus(null);
      return true;
    } catch (err) {
      setJsonImportErrors([getErrorMessage(err)]);
      setJsonImportStatus(null);
      return false;
    }
  }, [jsonImport, jsonImportDiagnostics, applyImportPreview]);

  const executeImportMutation = useLockedMutation(
    async () => {
      const serversToImport = scanCandidates.filter((server) => selectedImports.has(server.name));
      const result = await apiPost<{ imported: string[]; skipped: string[] }>(
        routes.import.execute(),
        { servers: serversToImport },
      );
      return result;
    },
    {
      onSuccess: (result) => {
        setScanStatus({
          key: "Imported {imported} servers. Skipped {skipped}.",
          params: { imported: result.imported.length, skipped: result.skipped.length },
        });
        setScanCandidates([]);
        setSelectedImports(new Set());
        setImportPreview(null);
        void queryClient.invalidateQueries({ queryKey: serverKeys.list() });
      },
    },
  );

  const toggleImport = useCallback((name: string, checked: boolean) => {
    setSelectedImports((prev) => {
      const next = new Set(prev);
      if (checked) next.add(name);
      else next.delete(name);
      return next;
    });
  }, []);

  const clearScan = useCallback(() => {
    setScanCandidates([]);
    setScanStatus(null);
    setImportPreview(null);
  }, []);

  const clearJsonImport = useCallback(() => {
    setJsonImport("");
    setJsonImportErrors([]);
    setJsonImportStatus(null);
  }, []);

  const hasStaticAuthorizationHeader = scanCandidates.some((server) => {
    const authorization = Object.entries(server.headers ?? {}).find(
      ([key]) => key.toLowerCase() === "authorization",
    )?.[1];
    return Boolean(authorization && !authorization.includes("{env:"));
  });

  return {
    scanCandidates,
    selectedImports,
    scanStatus,
    importPreview,
    hasStaticAuthorizationHeader,
    jsonImport,
    jsonImportErrors,
    jsonImportStatus,
    jsonImportDiagnostics,
    jsonImportStatusIsError: jsonImportStatus?.startsWith("Fix ") ?? false,
    scan,
    updateJsonImport,
    formatJson,
    parseJson,
    executeImport: executeImportMutation.mutate,
    importPending: executeImportMutation.pending,
    toggleImport,
    clearScan,
    clearJsonImport,
  };
}
