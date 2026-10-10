import { translateCurrent } from "@/lib/messages";
import {
  useIsMutating,
  MutationObserver,
  useQueryClient,
  type QueryClient,
} from "@tanstack/react-query";
import { toast } from "sonner";
import { api, apiPut } from "@/lib/api/client";
import { routes } from "@/lib/api-routes";
import { profileKeys, serverKeys } from "@/lib/query-keys";
import type { ProfileDetail, ProfileServerUpsert } from "@moor/types";

export interface ToolSelection {
  serverId: string;
  toolName: string;
}

interface Undo {
  snapshot: ProfileServerUpsert[];
  toastId?: string | number;
}

const undoByClient = new WeakMap<QueryClient, Map<string, Undo>>();

export function invalidateProfileUndo(client: QueryClient, profileId: string) {
  const entries = undoByClient.get(client);
  const undo = entries?.get(profileId);
  if (undo?.toastId !== undefined) toast.dismiss(undo.toastId);
  entries?.delete(profileId);
}

type Change =
  | { kind: "tools"; selection: ToolSelection[]; enabled: boolean; message?: string }
  | { kind: "server"; serverId: string; enabled: boolean }
  | { kind: "undo"; undo: Undo };

// 每次操作固定 Profile，toast 跨页面存活时也不会继承新路由的写入目标。
async function applyChange(client: QueryClient, profileId: string | undefined, change: Change) {
  const mutation = new MutationObserver(client, {
    mutationKey: ["profile-governance", profileId],
    scope: { id: `profile-governance:${profileId}` },
    mutationFn: async (change: Change) => {
      if (!profileId) throw new Error("Profile not found");
      let updates: ProfileServerUpsert[];
      const before: ProfileServerUpsert[] = [];
      if (change.kind === "undo") {
        if (undoByClient.get(client)?.get(profileId) !== change.undo) {
          throw new Error("Undo is no longer available because this profile changed");
        }
        updates = change.undo.snapshot;
      } else if (change.kind === "server") {
        updates = [{ serverId: change.serverId, enabled: change.enabled }];
      } else {
        if (change.selection.length === 0) return { updates: [], before };
        // 串行执行时读取完整清单，保留暂未发现的工具并避免排队期间快照过期。
        const profile = await api<ProfileDetail>(routes.profiles.detail(profileId));
        const selected = new Map<string, Set<string>>();
        for (const tool of change.selection) {
          const names = selected.get(tool.serverId) ?? new Set<string>();
          names.add(tool.toolName);
          selected.set(tool.serverId, names);
        }
        updates = [];
        for (const [serverId, names] of selected) {
          const server = profile.servers.find((item) => item.id === serverId);
          if (!server) throw new Error("Server not found in profile");
          before.push({ serverId, disabledTools: [...server.profileServer.disabledTools] });
          const disabled = new Set(server.profileServer.disabledTools);
          for (const name of names) {
            if (change.enabled) disabled.delete(name);
            else disabled.add(name);
          }
          updates.push({ serverId, disabledTools: [...disabled] });
        }
      }
      await apiPut(routes.profiles.bulkServerState(profileId), { updates });
      invalidateProfileUndo(client, profileId);
      return { updates, before };
    },
    onSuccess: async ({ updates, before }, change) => {
      if (!profileId || updates.length === 0) return;
      if (change.kind === "tools" && change.message) {
        const undo: Undo = { snapshot: before };
        const entries = undoByClient.get(client) ?? new Map<string, Undo>();
        undoByClient.set(client, entries);
        entries.set(profileId, undo);
        undo.toastId = toast.success(change.message, {
          action: {
            label: translateCurrent("Undo"),
            onClick: () => {
              void applyChange(client, profileId, { kind: "undo", undo }).catch(() => {});
            },
          },
        });
      } else if (change.kind === "undo") {
        toast.success(translateCurrent("Restored previous tool states"));
      }
      await Promise.all([
        client.invalidateQueries({ queryKey: profileKeys.detail(profileId) }),
        ...updates.map(({ serverId }) =>
          client.invalidateQueries({ queryKey: serverKeys.toolsRoot(serverId) }),
        ),
      ]);
    },
  });

  try {
    await mutation.mutate(change);
  } finally {
    mutation.reset();
  }
}

export function useProfileGovernance(profileId: string | undefined) {
  const client = useQueryClient();
  const busy = useIsMutating({ mutationKey: ["profile-governance", profileId], exact: true }) > 0;

  return {
    busy,
    setTools: async (selection: ToolSelection[], enabled: boolean, message?: string) => {
      try {
        await applyChange(client, profileId, { kind: "tools", selection, enabled, message });
        return true;
      } catch {
        // 失败由全局 MutationCache 提示；页面保留选择以便重试。
        return false;
      }
    },
    setServerEnabled: async (serverId: string, enabled: boolean) => {
      try {
        await applyChange(client, profileId, { kind: "server", serverId, enabled });
      } catch {
        // 失败由全局 MutationCache 提示。
      }
    },
  };
}
