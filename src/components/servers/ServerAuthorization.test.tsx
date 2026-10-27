import { describe, expect, it, vi } from "vite-plus/test";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { renderToStaticMarkup } from "react-dom/server";
import { LocaleContext } from "@/contexts/LocaleContext";
import { translateText } from "@/lib/messages";
import { ServerAuthorization } from "./ServerAuthorization";

const data = vi.hoisted(() => ({
  snapshot: { serverId: "demo", state: "required", canRevoke: true, scopes: ["mcp.read"] },
}));

vi.mock("@/hooks/useServerAuthorization", () => ({
  useServerAuthorization: () => ({
    authorization: data.snapshot,
    registerClient: async () => undefined,
    start: async () => undefined,
    revoke: async () => undefined,
  }),
}));

describe("ServerAuthorization", () => {
  it("allows revocation of a retained grant after authorization becomes required", () => {
    const markup = renderToStaticMarkup(
      <QueryClientProvider client={new QueryClient()}>
        <LocaleContext.Provider value="en">
          <ServerAuthorization serverId="demo" />
        </LocaleContext.Provider>
      </QueryClientProvider>,
    );
    expect(markup).toContain(">Revoke<");
  });

  it("localizes the remote revocation warning and preserves unknown warnings", () => {
    const warning = "Provider revocation failed; local credentials were removed.";
    expect(translateText("zh-CN", warning)).toBe("服务提供商撤销授权失败；本地凭据已删除。");
    expect(translateText("en", warning)).toBe(warning);
    expect(translateText("zh-CN", "Unknown provider warning")).toBe("Unknown provider warning");
  });
});
