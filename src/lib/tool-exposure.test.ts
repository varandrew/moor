import { describe, expect, it } from "vite-plus/test";
import type { ProfileToolGroup, ToolDetail } from "@moor/types";
import { isToolListed, needsPinWarning, summarizeExposure } from "./tool-exposure";

function tool(toolName: string, pinned: boolean, disabled = false): ToolDetail {
  return {
    toolName,
    exposedName: `fake__${toolName}`,
    description: null,
    inputSchema: null,
    disabled,
    pinned,
  };
}

function group(serverEnabled: boolean, tools: ToolDetail[]): ProfileToolGroup {
  return {
    serverId: "server-1",
    serverName: "Fake",
    serverEnabled,
    tools,
  };
}

describe("isToolListed", () => {
  it("lists every enabled tool in direct mode", () => {
    expect(isToolListed(tool("search", false), "direct", true)).toBe(true);
  });

  it("lists only pinned tools in curated mode", () => {
    expect(isToolListed(tool("search", false), "curated", true)).toBe(false);
    expect(isToolListed(tool("search", true), "curated", true)).toBe(true);
  });

  it("hides denied tools in both modes", () => {
    expect(isToolListed(tool("search", false, true), "direct", true)).toBe(false);
    expect(isToolListed(tool("search", true, true), "curated", true)).toBe(false);
  });

  it("hides everything from a disabled server", () => {
    expect(isToolListed(tool("search", true), "curated", false)).toBe(false);
  });
});

describe("summarizeExposure", () => {
  it("counts listed, total and pinned separately", () => {
    const groups = [
      group(true, [tool("search", true), tool("read", false), tool("write", false, true)]),
    ];

    expect(summarizeExposure(groups, "direct")).toEqual({
      listed: 2,
      total: 3,
      pinned: 1,
    });
    expect(summarizeExposure(groups, "curated")).toEqual({
      listed: 1,
      total: 3,
      pinned: 1,
    });
  });

  it("counts nothing from a disabled server", () => {
    const groups = [group(false, [tool("search", true)])];

    expect(summarizeExposure(groups, "curated").listed).toBe(0);
    expect(summarizeExposure(groups, "direct").listed).toBe(0);
  });

  it("keeps the pinned count independent of the mode", () => {
    const groups = [group(true, [tool("search", true), tool("read", true)])];

    expect(summarizeExposure(groups, "direct").pinned).toBe(2);
    expect(summarizeExposure(groups, "curated").pinned).toBe(2);
  });
});

describe("needsPinWarning", () => {
  it("warns only in curated mode with nothing pinned", () => {
    const empty = { listed: 0, total: 5, pinned: 0 };
    const pinned = { listed: 1, total: 5, pinned: 1 };

    expect(needsPinWarning("curated", empty)).toBe(true);
    expect(needsPinWarning("curated", pinned)).toBe(false);
    expect(needsPinWarning("direct", empty)).toBe(false);
  });
});
