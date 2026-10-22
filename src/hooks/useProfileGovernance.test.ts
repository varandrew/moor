import { beforeEach, describe, expect, it, vi } from "vite-plus/test";
import { applyChange } from "./useProfileGovernance";
import { profileKeys, serverKeys } from "@/lib/query-keys";
import { queryClient as appQueryClient } from "@/lib/query-client";

const mocks = vi.hoisted(() => ({
  get: vi.fn(),
  put: vi.fn(),
  success: vi.fn(
    (_message: string, _options?: { action: { label: string; onClick: () => void } }) => "toast-id",
  ),
  dismiss: vi.fn(),
  error: vi.fn(),
}));
vi.mock("@/lib/api/client", () => ({ api: mocks.get, apiPut: mocks.put }));
vi.mock("sonner", () => ({
  toast: { success: mocks.success, dismiss: mocks.dismiss, error: mocks.error },
}));

function deferred() {
  let resolve!: () => void;
  const promise = new Promise<void>((done) => {
    resolve = done;
  });
  return { promise, resolve };
}

beforeEach(() => {
  appQueryClient.clear();
  vi.clearAllMocks();
  mocks.put.mockReset().mockResolvedValue({});
  mocks.get.mockReset().mockResolvedValue({
    servers: [
      { id: "server", profileServer: { enabled: true, disabledTools: [], pinnedTools: ["old"] } },
    ],
  });
});

describe("Profile Tool Governance", () => {
  it("sends one atomic transition and invalidates every affected projection", async () => {
    const queryClient = appQueryClient;
    const keys = [
      profileKeys.list(),
      profileKeys.detail("a"),
      profileKeys.tools("a"),
      serverKeys.tools("server", "a"),
    ];
    for (const key of keys) queryClient.setQueryData(key, {});
    const updates = {
      exposureMode: "curated" as const,
      initialPins: [{ serverId: "server", toolName: "new" }],
    };
    await applyChange(queryClient, "a", { kind: "profile", updates });
    expect(mocks.put).toHaveBeenCalledExactlyOnceWith("/api/profiles/a", updates);
    expect(mocks.get).not.toHaveBeenCalled();
    for (const key of keys) expect(queryClient.getQueryState(key)?.isInvalidated).toBe(true);
    queryClient.clear();
  });

  it("serializes all governance writes for one Profile while another Profile proceeds", async () => {
    const queryClient = appQueryClient;
    const gate = deferred();
    mocks.put.mockImplementation((path: string) =>
      path === "/api/profiles/a" ? gate.promise : Promise.resolve({}),
    );
    const first = applyChange(queryClient, "a", {
      kind: "profile",
      updates: { exposureMode: "curated", initialPins: [] },
    });
    await vi.waitFor(() => expect(mocks.put).toHaveBeenCalledTimes(1));
    const second = applyChange(queryClient, "a", {
      kind: "pinned",
      selection: [{ serverId: "server", toolName: "next" }],
      pinned: true,
    });
    await applyChange(queryClient, "b", {
      kind: "profile",
      updates: { riskPolicy: "block_destructive" },
    });
    expect(mocks.get).not.toHaveBeenCalled();
    expect(mocks.put).toHaveBeenCalledTimes(2);
    gate.resolve();
    await Promise.all([first, second]);
    expect(mocks.put).toHaveBeenLastCalledWith("/api/profiles/a/servers-state", {
      updates: [{ serverId: "server", pinnedTools: ["old", "next"] }],
    });
    queryClient.clear();
  });

  it("keeps Undo after a failed transition and invalidates it only after success", async () => {
    const queryClient = appQueryClient;
    await applyChange(queryClient, "a", {
      kind: "pinned",
      selection: [{ serverId: "server", toolName: "new" }],
      pinned: true,
      message: "Pinned",
    });
    const undo = mocks.success.mock.calls[0][1]?.action.onClick;
    expect(undo).toBeDefined();
    mocks.put.mockRejectedValueOnce(new Error("Save failed"));
    await expect(
      applyChange(queryClient, "a", {
        kind: "profile",
        updates: { exposureMode: "curated", initialPins: [] },
      }),
    ).rejects.toThrow("Save failed");
    expect(mocks.error).toHaveBeenCalledWith("Request failed", { description: "Save failed" });
    expect(mocks.dismiss).not.toHaveBeenCalled();
    undo?.();
    await vi.waitFor(() =>
      expect(mocks.success).toHaveBeenCalledWith("Restored previous tool states"),
    );
    expect(mocks.put).toHaveBeenLastCalledWith("/api/profiles/a/servers-state", {
      updates: [{ serverId: "server", pinnedTools: ["old"] }],
    });
    await applyChange(queryClient, "a", {
      kind: "pinned",
      selection: [{ serverId: "server", toolName: "new" }],
      pinned: true,
      message: "Pinned again",
    });
    const laterUndo =
      mocks.success.mock.calls[mocks.success.mock.calls.length - 1][1]?.action.onClick;
    await applyChange(queryClient, "a", { kind: "profile", updates: { exposureMode: "direct" } });
    const writes = mocks.put.mock.calls.length;
    laterUndo?.();
    await vi.waitFor(() => expect(queryClient.isMutating()).toBe(0));
    expect(mocks.put).toHaveBeenCalledTimes(writes);
    expect(mocks.dismiss).toHaveBeenCalledWith("toast-id");
    queryClient.clear();
  });
});
