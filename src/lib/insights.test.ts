import { describe, expect, it, vi } from "vite-plus/test";
import { formatMs, formatRate, isToolErrorProne, isToolSlow, windowFromDate } from "./insights";
import type { ToolInsight } from "@moor/types";

function tool(overrides: Partial<ToolInsight>): ToolInsight {
  return {
    toolName: "search",
    serverId: "s1",
    serverName: "Alpha",
    callCount: 10,
    errorCount: 0,
    errorRate: 0,
    avgDurationMs: 100,
    p50Ms: 100,
    p95Ms: 200,
    lastCalledAt: null,
    ...overrides,
  };
}

describe("isToolErrorProne", () => {
  it("flags tools with sustained error rate over threshold", () => {
    expect(isToolErrorProne(tool({ callCount: 10, errorCount: 3, errorRate: 0.3 }))).toBe(true);
  });

  it("ignores error rate on low-volume tools to avoid noise from flukes", () => {
    // 意图：偶发一次失败的新工具不应进入治理候选，否则面板会被误报淹没
    expect(isToolErrorProne(tool({ callCount: 2, errorCount: 1, errorRate: 0.5 }))).toBe(false);
  });
});

describe("洞察边界", () => {
  it("达到阈值时提示，低调用量保持静默", () => {
    expect(isToolErrorProne(tool({ callCount: 5, errorCount: 1, errorRate: 0.2 }))).toBe(true);
    expect(isToolErrorProne(tool({ callCount: 4, errorCount: 1, errorRate: 0.25 }))).toBe(false);
    expect(isToolErrorProne(tool({ callCount: 5, errorRate: 0.199 }))).toBe(false);
    expect(isToolSlow(tool({ p95Ms: 5000 }))).toBe(true);
  });

  it("统一秒与缺失值的展示", () => {
    expect(formatMs(1000)).toBe("1.0s");
    expect(formatMs(undefined)).toBe("—");
    expect(formatRate(0)).toBe("0.0%");
    expect(formatRate(0.2)).toBe("20.0%");
    expect(formatRate(1 / 3)).toBe("33.3%");
  });

  it("七天和三十天窗口保留当前时间精度", () => {
    vi.useFakeTimers();
    try {
      vi.setSystemTime(new Date("2026-03-01T12:34:56.789Z"));
      expect(windowFromDate("7d")).toBe("2026-02-22T12:34:56.789Z");
      expect(windowFromDate("30d")).toBe("2026-01-30T12:34:56.789Z");
    } finally {
      vi.useRealTimers();
    }
  });
});

describe("isToolSlow", () => {
  it("flags only when p95 crosses the threshold", () => {
    expect(isToolSlow(tool({ p95Ms: 6000 }))).toBe(true);
    expect(isToolSlow(tool({ p95Ms: 4999 }))).toBe(false);
    expect(isToolSlow(tool({ p95Ms: null }))).toBe(false);
  });
});

describe("formatMs", () => {
  it("renders seconds above 1s and milliseconds below", () => {
    expect(formatMs(1500)).toBe("1.5s");
    expect(formatMs(120)).toBe("120ms");
    expect(formatMs(null)).toBe("—");
  });
});

describe("windowFromDate", () => {
  it("returns undefined for all-time window", () => {
    expect(windowFromDate("all")).toBeUndefined();
  });

  it("computes a past ISO timestamp for bounded windows", () => {
    const from = windowFromDate("24h")!;
    const diffHours = (Date.now() - new Date(from).getTime()) / 3600 / 1000;
    expect(diffHours).toBeGreaterThan(23.9);
    expect(diffHours).toBeLessThan(24.1);
  });
});
