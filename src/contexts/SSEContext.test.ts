import { describe, expect, it } from "vite-plus/test";
import { parseMoorSSEEvent } from "./SSEContext";

describe("parseMoorSSEEvent", () => {
  it("accepts valid typed event payloads", () => {
    const warnings: string[] = [];

    expect(
      parseMoorSSEEvent(
        "server:status",
        { serverId: "server-1", status: "running", errorMessage: null },
        (message) => warnings.push(message),
      ),
    ).toEqual({
      type: "server:status",
      data: { serverId: "server-1", status: "running", errorMessage: null },
    });
    expect(warnings).toEqual([]);
  });

  it("ignores the initial connected event without warning", () => {
    const warnings: string[] = [];

    expect(
      parseMoorSSEEvent("connected", { timestamp: "2026-05-25T00:00:00.000Z" }, (message) =>
        warnings.push(message),
      ),
    ).toBeNull();
    expect(warnings).toEqual([]);
  });

  it("drops envelope-shaped drifted payloads with a warning", () => {
    const warnings: string[] = [];

    expect(
      parseMoorSSEEvent(
        "server:status",
        {
          type: "server:status",
          data: { serverId: "server-1", status: "running" },
        },
        (message) => warnings.push(message),
      ),
    ).toBeNull();
    expect(warnings).toHaveLength(1);
  });
});

describe("server health events", () => {
  it("accepts complete snapshots and rejects malformed health payloads", () => {
    const snapshot = {
      serverId: "demo",
      status: "unhealthy",
      checkedAt: "2026-10-09T00:00:00Z",
      consecutiveFailures: 3,
      errorMessage: "timeout",
    };
    expect(parseMoorSSEEvent("server:health", snapshot)).toEqual({
      type: "server:health",
      data: snapshot,
    });
    for (const invalid of [
      { ...snapshot, status: "running" },
      { ...snapshot, consecutiveFailures: -1 },
      { ...snapshot, checkedAt: 1 },
    ]) {
      expect(parseMoorSSEEvent("server:health", invalid, () => undefined)).toBeNull();
    }
  });
});
