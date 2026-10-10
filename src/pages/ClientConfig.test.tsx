import { describe, expect, it } from "vite-plus/test";
import { CLIENT_ICONS } from "./ClientConfig";

// 与 Rust 侧 ALL_CLIENTS 对齐的 id 快照——漏配图标会静默降级为 Terminal 兜底。
const ALL_CLIENT_IDS = [
  "claude-code",
  "claude-desktop",
  "codex",
  "cursor",
  "opencode",
  "kimi-code",
  "dsh",
  "grok-build",
  "pi",
  "zcode",
  "minimax-code",
] as const;

describe("client icons", () => {
  it("covers every registry client id", () => {
    for (const id of ALL_CLIENT_IDS) {
      expect(CLIENT_ICONS[id], id).toBeDefined();
    }
  });
});
