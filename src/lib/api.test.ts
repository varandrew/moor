import { afterEach, beforeEach, describe, expect, it, vi } from "vite-plus/test";
import type { SidecarInfo } from "@moor/types";
import { ApiRequestError } from "./api-error";
import { api } from "./api/client";
import { formatApiNetworkError } from "./api/errors";
import { resetRuntime } from "./api/runtime";

const { invokeMock } = vi.hoisted(() => ({
  invokeMock: vi.fn<() => Promise<SidecarInfo>>(),
}));

vi.mock("@tauri-apps/api/core", () => ({
  invoke: invokeMock,
}));

function runtime(port: number, apiToken: string): SidecarInfo {
  return {
    port,
    baseUrl: `http://127.0.0.1:${port}`,
    apiToken,
  };
}

function jsonResponse(body: unknown, init?: ResponseInit): Response {
  return new Response(JSON.stringify(body), {
    status: 200,
    headers: { "Content-Type": "application/json" },
    ...init,
  });
}

describe("api runtime recovery", () => {
  beforeEach(() => {
    resetRuntime();
    invokeMock.mockReset();
  });

  afterEach(() => {
    vi.restoreAllMocks();
    resetRuntime();
  });

  it("refreshes runtime and retries once after a network failure", async () => {
    invokeMock.mockResolvedValueOnce(runtime(9223, "old-token"));
    invokeMock.mockResolvedValueOnce(runtime(9225, "fresh-token"));
    const fetchMock = vi
      .spyOn(globalThis, "fetch")
      .mockRejectedValueOnce(new TypeError("Load failed"))
      .mockResolvedValueOnce(jsonResponse({ ok: true }));

    await expect(api<{ ok: boolean }>("/api/settings")).resolves.toEqual({ ok: true });

    expect(fetchMock).toHaveBeenCalledTimes(2);
    expect(fetchMock).toHaveBeenNthCalledWith(
      1,
      "http://127.0.0.1:9223/api/settings",
      expect.objectContaining({
        headers: expect.objectContaining({ "X-Moor-Token": "old-token" }),
      }),
    );
    expect(fetchMock).toHaveBeenNthCalledWith(
      2,
      "http://127.0.0.1:9225/api/settings",
      expect.objectContaining({
        headers: expect.objectContaining({ "X-Moor-Token": "fresh-token" }),
      }),
    );
  });

  it("does not refresh runtime or retry aborted requests", async () => {
    invokeMock.mockResolvedValueOnce(runtime(9223, "token"));
    invokeMock.mockResolvedValueOnce(runtime(9225, "fresh-token"));
    const controller = new AbortController();
    controller.abort();
    const fetchMock = vi
      .spyOn(globalThis, "fetch")
      .mockRejectedValueOnce(new DOMException("This operation was aborted", "AbortError"))
      .mockResolvedValueOnce(jsonResponse({ ok: true }));

    await expect(api("/api/settings", { signal: controller.signal })).rejects.toThrow(
      "This operation was aborted",
    );

    expect(fetchMock).toHaveBeenCalledTimes(1);
    expect(invokeMock).toHaveBeenCalledTimes(1);
  });

  it.each(["POST", "PUT", "PATCH", "DELETE"])(
    "does not retry %s requests after a network failure",
    async (method) => {
      invokeMock.mockResolvedValueOnce(runtime(9223, "token"));
      const fetchMock = vi
        .spyOn(globalThis, "fetch")
        .mockRejectedValueOnce(new TypeError("Load failed"));

      await expect(
        api("/api/settings", {
          method,
          body: method === "DELETE" ? undefined : JSON.stringify({ theme: "dark" }),
        }),
      ).rejects.toThrow("Unable to connect to the Moor sidecar while requesting /api/settings");

      expect(fetchMock).toHaveBeenCalledTimes(1);
      expect(invokeMock).toHaveBeenCalledTimes(1);
    },
  );

  it("refreshes runtime and retries once after an unauthorized response", async () => {
    invokeMock.mockResolvedValueOnce(runtime(9223, "old-token"));
    invokeMock.mockResolvedValueOnce(runtime(9225, "fresh-token"));
    const fetchMock = vi
      .spyOn(globalThis, "fetch")
      .mockResolvedValueOnce(jsonResponse({ error: "Unauthorized" }, { status: 401 }))
      .mockResolvedValueOnce(jsonResponse({ theme: "dark" }));

    await expect(api<{ theme: string }>("/api/settings")).resolves.toEqual({ theme: "dark" });

    expect(fetchMock).toHaveBeenCalledTimes(2);
  });

  it("refreshes runtime and retries write requests once after an unauthorized response", async () => {
    invokeMock.mockResolvedValueOnce(runtime(9223, "old-token"));
    invokeMock.mockResolvedValueOnce(runtime(9225, "fresh-token"));
    const fetchMock = vi
      .spyOn(globalThis, "fetch")
      .mockResolvedValueOnce(jsonResponse({ error: "Unauthorized" }, { status: 401 }))
      .mockResolvedValueOnce(jsonResponse({ theme: "dark" }));

    await expect(
      api("/api/settings", {
        method: "POST",
        body: JSON.stringify({ theme: "dark" }),
      }),
    ).resolves.toEqual({ theme: "dark" });

    expect(fetchMock).toHaveBeenCalledTimes(2);
  });

  it("does not retry ordinary API errors", async () => {
    invokeMock.mockResolvedValueOnce(runtime(9223, "token"));
    const fetchMock = vi
      .spyOn(globalThis, "fetch")
      .mockResolvedValueOnce(jsonResponse({ error: "Invalid settings" }, { status: 400 }));

    await expect(api("/api/settings")).rejects.toThrow("Invalid settings");

    expect(fetchMock).toHaveBeenCalledTimes(1);
  });

  it("retains structured error codes and status for localized UI explanations", async () => {
    invokeMock.mockResolvedValueOnce(runtime(9223, "token"));
    vi.spyOn(globalThis, "fetch").mockResolvedValueOnce(
      jsonResponse({ error: { code: "SERVER_BUSY", message: "Server is busy" } }, { status: 409 }),
    );
    const error = await api("/api/servers/demo/check-health").catch((error: unknown) => error);
    expect(error).toBeInstanceOf(ApiRequestError);
    expect(error).toMatchObject({ code: "SERVER_BUSY", status: 409, message: "Server is busy" });
  });

  it("uses structured API error messages when available", async () => {
    invokeMock.mockResolvedValueOnce(runtime(9223, "token"));
    vi.spyOn(globalThis, "fetch").mockResolvedValueOnce(
      jsonResponse(
        { error: { code: "VALIDATION_ERROR", message: "advanced.sidecarPort: Too small" } },
        { status: 400 },
      ),
    );

    await expect(api("/api/settings")).rejects.toThrow("advanced.sidecarPort: Too small");
  });

  it("falls back to structured API error code when message is missing", async () => {
    invokeMock.mockResolvedValueOnce(runtime(9223, "token"));
    vi.spyOn(globalThis, "fetch").mockResolvedValueOnce(
      jsonResponse({ error: { code: "NOT_FOUND" } }, { status: 404 }),
    );

    await expect(api("/api/profiles/missing")).rejects.toThrow("NOT_FOUND");
  });

  it("uses the same runtime for a request URL and token", async () => {
    invokeMock.mockResolvedValueOnce(runtime(9224, "same-token"));
    const fetchMock = vi
      .spyOn(globalThis, "fetch")
      .mockResolvedValueOnce(jsonResponse({ ok: true }));

    await api("/api/settings");

    expect(fetchMock).toHaveBeenCalledWith(
      "http://127.0.0.1:9224/api/settings",
      expect.objectContaining({
        headers: expect.objectContaining({ "X-Moor-Token": "same-token" }),
      }),
    );
  });
});

describe("api error formatting", () => {
  it("adds sidecar context to browser network failures", () => {
    expect(
      formatApiNetworkError("/api/settings", new TypeError("Load failed"), runtime(9225, "t")),
    ).toBe(
      "Unable to connect to the Moor sidecar while requesting /api/settings at http://127.0.0.1:9225. Check that Moor is running and the Sidecar API port/token are current. Original error: Load failed",
    );
  });
});
