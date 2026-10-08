import { invalidateProfileUndo } from "./useProfileGovernance";
import { useCallback } from "react";
import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import { api, apiPost, apiPut, apiDelete } from "@/lib/api/client";
import { routes } from "@/lib/api-routes";
import { serverKeys, profileKeys, logKeys } from "@/lib/query-keys";
import { useSSEEvent } from "@/contexts/SSEContext";
import type { Profile, ProfileDetail, ProfileToolGroup } from "@moor/types";

export function useProfiles() {
  const queryClient = useQueryClient();

  useSSEEvent("profile:activated", () => {
    void queryClient.invalidateQueries({ queryKey: profileKeys.list() });
    void queryClient.invalidateQueries({ queryKey: serverKeys.list() });
    void queryClient.invalidateQueries({ queryKey: logKeys.all() });
  });

  const {
    data: profiles = [],
    isLoading: loading,
    error,
  } = useQuery<Profile[]>({
    queryKey: profileKeys.list(),
    queryFn: ({ signal }) => api<Profile[]>(routes.profiles.list(), { signal }),
  });

  const refresh = useCallback(async () => {
    await queryClient.invalidateQueries({ queryKey: profileKeys.list() });
  }, [queryClient]);

  const createProfile = useMutation({
    mutationFn: (name: string) => apiPost<Profile>(routes.profiles.create(), { name }),
    onSuccess: (profile) => {
      queryClient.setQueryData<Profile[]>(profileKeys.list(), (prev) => [...(prev ?? []), profile]);
    },
  });

  const activateProfile = useMutation({
    mutationFn: async (id: string) => {
      await apiPut(routes.profiles.activate(id), {});
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: profileKeys.list() });
      void queryClient.invalidateQueries({ queryKey: serverKeys.list() });
    },
  });

  const deleteProfile = useMutation({
    mutationFn: (id: string) => apiDelete(routes.profiles.delete(id)).then(() => id),
    onSuccess: (id) => {
      invalidateProfileUndo(queryClient, id);
      queryClient.setQueryData<Profile[]>(profileKeys.list(), (prev) =>
        prev?.filter((p) => p.id !== id),
      );
    },
  });

  const updateProfile = useMutation({
    mutationFn: async ({ id, updates }: { id: string; updates: { name?: string } }) => {
      await apiPut(routes.profiles.update(id), updates);
      invalidateProfileUndo(queryClient, id);
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: profileKeys.list() });
    },
  });

  const cloneProfile = useMutation({
    mutationFn: async ({ sourceId, name }: { sourceId: string; name: string }) =>
      apiPost<Profile>(routes.profiles.clone(sourceId), { name }),
    onSuccess: (profile) => {
      queryClient.setQueryData<Profile[]>(profileKeys.list(), (prev) => [...(prev ?? []), profile]);
    },
  });

  return {
    profiles,
    loading,
    error: error?.message ?? null,
    refresh,
    createProfile: createProfile.mutateAsync,
    activateProfile: activateProfile.mutateAsync,
    deleteProfile: deleteProfile.mutateAsync,
    updateProfile: updateProfile.mutateAsync,
    cloneProfile: cloneProfile.mutateAsync,
  };
}

export function useProfile(id: string | undefined) {
  const queryClient = useQueryClient();

  const {
    data: profile,
    isLoading,
    error,
  } = useQuery<ProfileDetail | null>({
    queryKey: profileKeys.detail(id!),
    queryFn: ({ signal }) => api<ProfileDetail | null>(routes.profiles.detail(id!), { signal }),
    enabled: !!id,
  });

  const refresh = useCallback(() => {
    void queryClient.invalidateQueries({ queryKey: profileKeys.detail(id!) });
  }, [queryClient, id]);

  return { profile, isLoading, error, refresh };
}

export function useProfileTools(id: string | undefined) {
  const queryClient = useQueryClient();

  const {
    data: groups = [],
    isLoading,
    error,
  } = useQuery<ProfileToolGroup[]>({
    queryKey: profileKeys.tools(id!),
    queryFn: ({ signal }) => api<ProfileToolGroup[]>(routes.profiles.tools(id!), { signal }),
    enabled: !!id,
  });

  const refresh = useCallback(() => {
    void queryClient.invalidateQueries({ queryKey: profileKeys.tools(id!) });
  }, [queryClient, id]);

  return { groups, isLoading, error, refresh };
}
