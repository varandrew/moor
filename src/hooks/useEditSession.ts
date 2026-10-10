import { getNoticeErrorMessage } from "@/lib/utils";
import { useTranslation } from "@/contexts/LocaleContext";
import { useState, useCallback, useEffect, useMemo } from "react";
import { useBlocker } from "react-router-dom";
import { api } from "@/lib/api/client";
import { useLockedMutation } from "@/hooks/useLockedMutation";
import { routes } from "@/lib/api-routes";
import {
  serverToForm,
  validateEditForm,
  formToUpdates,
  hasChanges,
  type EditForm,
} from "@/lib/server-form";
import type { ServerDetail, ServerUpdateInput } from "@moor/types";
import { toast } from "sonner";

interface EditSessionOptions {
  server: ServerDetail | null | undefined;
  serverId: string | undefined;
  updateServer: (args: { id: string; updates: ServerUpdateInput }) => Promise<unknown>;
}

export function useEditSession({ server, serverId, updateServer }: EditSessionOptions) {
  const { t, text } = useTranslation();
  const [isEditing, setIsEditing] = useState(false);
  const [editForm, setEditForm] = useState<EditForm | null>(null);
  const [baselineForm, setBaselineForm] = useState<EditForm | null>(null);
  const [baselineServer, setBaselineServer] = useState<ServerDetail | null>(null);
  const [discardSource, setDiscardSource] = useState<"manual" | null>(null);
  const [overwriteOpen, setOverwriteOpen] = useState(false);

  const dirty = useMemo(
    () => (isEditing && editForm && baselineForm ? hasChanges(editForm, baselineForm) : false),
    [baselineForm, editForm, isEditing],
  );

  const blocker = useBlocker(dirty);

  const editConnectionType = baselineServer?.connectionType ?? server?.connectionType ?? "stdio";

  const exitEdit = useCallback(() => {
    setEditForm(null);
    setBaselineForm(null);
    setBaselineServer(null);
    setIsEditing(false);
  }, []);

  const enterEdit = useCallback(() => {
    if (!server) return;
    const nextForm = serverToForm(server);
    setEditForm(nextForm);
    setBaselineForm(nextForm);
    setBaselineServer(server);
    setIsEditing(true);
  }, [server]);

  const requestCancelEdit = useCallback(() => {
    if (dirty) {
      setDiscardSource("manual");
      return;
    }
    exitEdit();
  }, [dirty, exitEdit]);

  const saveMutation = useLockedMutation(
    async (overwrite: boolean) => {
      if (!serverId || !editForm || !server || !baselineForm) return;
      const connectionType = baselineServer?.connectionType ?? server.connectionType;
      const validationError = validateEditForm(editForm, connectionType);
      if (validationError) {
        toast.error(text(validationError));
        return;
      }

      if (!overwrite) {
        const latest = await api<ServerDetail>(routes.servers.detail(serverId));
        if (latest.connectionType !== connectionType) {
          toast.error(t("Save failed"), {
            description: t("Connection type changed. Reopen this server before saving."),
          });
          return;
        }
        if (hasChanges(serverToForm(latest), baselineForm)) {
          setOverwriteOpen(true);
          return;
        }
      }

      const updates = formToUpdates(editForm, connectionType);
      await updateServer({ id: serverId, updates });
      if (server?.status === "running") {
        toast.success(t("Configuration saved"), {
          description: t("Restart the server to apply changes."),
        });
      } else {
        toast.success(t("Configuration saved"));
      }
      exitEdit();
    },
    {
      onError: (err) =>
        toast.error(t("Save failed"), {
          description: getNoticeErrorMessage(err),
        }),
    },
  );
  const { mutate: runSave, pending: saving } = saveMutation;
  const saveEdit = useCallback((overwrite = false) => void runSave(overwrite), [runSave]);
  const confirmDiscard = useCallback(() => {
    setDiscardSource(null);
    if (blocker.state === "blocked") {
      blocker.proceed?.();
      return;
    }
    exitEdit();
  }, [blocker, exitEdit]);

  const cancelDiscard = useCallback(() => {
    setDiscardSource(null);
    if (blocker.state === "blocked") {
      blocker.reset?.();
    }
  }, [blocker]);

  const confirmOverwrite = useCallback(() => {
    setOverwriteOpen(false);
    void saveEdit(true);
  }, [saveEdit]);

  useEffect(() => {
    if (!dirty) return;
    const handler = (e: BeforeUnloadEvent) => {
      e.preventDefault();
      e.returnValue = "";
    };
    window.addEventListener("beforeunload", handler);
    return () => window.removeEventListener("beforeunload", handler);
  }, [dirty]);

  const discardOpen = discardSource !== null;

  return {
    isEditing,
    editForm,
    editConnectionType,
    dirty,
    saving,
    blocker,
    discardOpen,
    overwriteOpen,
    setOverwriteOpen,
    setEditForm,
    enterEdit,
    requestCancelEdit,
    saveEdit,
    confirmDiscard,
    cancelDiscard,
    confirmOverwrite,
  };
}
