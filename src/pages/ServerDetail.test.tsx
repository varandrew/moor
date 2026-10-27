import { describe, expect, it } from "vite-plus/test";
import { renderToStaticMarkup } from "react-dom/server";
import { hasChanges, type EditForm } from "@/lib/server-form";
import { ServerEditFields } from "./ServerDetail";
import { ToolCategoryBadge } from "@/components/shared/ToolCategoryBadge";
import { LocaleContext } from "@/contexts/LocaleContext";
import {
  AuthorizationBadge,
  AuthorizationButton,
  AuthorizationClientError,
  AuthorizationErrorBanner,
  RevocationButton,
  authorizeServer,
} from "@/components/servers/ServerAuthorization";

const baseForm: EditForm = {
  name: "Create Tools",
  command: "node",
  url: "",
  args: "server.js",
  env: [],
  headers: [],
  workingDir: "/tmp/project",
};

describe("ServerDetail", () => {
  it("classifies tools from the original tool name", () => {
    const markup = renderToStaticMarkup(<ToolCategoryBadge name="search" />);

    expect(markup).toContain(">Search<");
    expect(markup).toContain("bg-grep/15");
    expect(markup).not.toContain("bg-edit/15");
  });

  it("keeps working directory editable for stdio servers", () => {
    const markup = renderToStaticMarkup(
      <ServerEditFields form={baseForm} connectionType="stdio" onChange={() => undefined} />,
    );

    expect(markup).toContain(">Working Directory<");
    expect(markup).toContain("Type cannot be changed after creation.");
  });

  it("hides working directory editing for HTTP servers", () => {
    const markup = renderToStaticMarkup(
      <ServerEditFields
        form={{ ...baseForm, command: "", url: "http://localhost:3000/mcp" }}
        connectionType="http"
        onChange={() => undefined}
      />,
    );

    expect(markup).toContain(">URL<");
    expect(markup).not.toContain(">Working Directory<");
  });

  it("does not mark equivalent key-value rows dirty because of row order", () => {
    const baseline: EditForm = {
      ...baseForm,
      name: " Create Tools ",
      args: "one\ntwo",
      env: [
        ["TOKEN", "secret"],
        ["REGION", "us"],
      ],
      headers: [
        ["Authorization", "Bearer token"],
        ["X-Trace", "abc"],
      ],
    };
    const form: EditForm = {
      ...baseForm,
      name: "Create Tools",
      args: "one\r\ntwo",
      env: [
        ["REGION", "us"],
        ["TOKEN", "secret"],
      ],
      headers: [
        ["x-trace", "abc"],
        [" authorization ", "Bearer token"],
      ],
    };

    expect(hasChanges(form, baseline)).toBe(false);
  });

  it("renders OAuth states and reauthorization actions", () => {
    for (const [state, label] of [
      ["not_required", "Not configured"],
      ["required", "Authorization required"],
      ["authorizing", "Waiting for browser"],
      ["authorized", "Authorized"],
      ["error", "Authorization failed"],
    ] as const) {
      expect(renderToStaticMarkup(<AuthorizationBadge state={state} />)).toContain(`>${label}<`);
    }
    expect(
      renderToStaticMarkup(
        <AuthorizationButton state="authorized" pending={false} onClick={() => undefined} />,
      ),
    ).toContain(">Reauthorize<");
    expect(
      renderToStaticMarkup(
        <AuthorizationButton state="required" pending={false} onClick={() => undefined} />,
      ),
    ).toContain(">Authorize<");
    const restart = renderToStaticMarkup(
      <AuthorizationButton state="authorizing" pending={false} onClick={() => undefined} />,
    );
    expect(restart).toContain(">Restart authorization<");
    expect(restart).not.toContain(" disabled=");
    expect(
      renderToStaticMarkup(<RevocationButton pending={false} onClick={() => undefined} />),
    ).toContain(">Revoke<");
  });

  it("starts DCR without a client ID and preserves pre-registration", async () => {
    const calls: string[] = [];
    const registerClient = async (input: { clientId: string; clientSecret?: string }) => {
      calls.push(`register:${input.clientId}:${input.clientSecret ?? ""}`);
    };
    const start = async (clientId: string) => {
      calls.push(`start:${clientId}`);
    };

    await authorizeServer({
      clientId: "",
      clientSecret: "",
      registerClient,
      start,
    });
    expect(calls).toEqual(["start:"]);

    await authorizeServer({
      clientId: "manual-client",
      clientSecret: "secret",
      registerClient,
      start,
    });
    expect(calls).toEqual(["start:", "register:manual-client:secret", "start:manual-client"]);

    await expect(
      authorizeServer({ clientId: "", clientSecret: "secret", registerClient, start }),
    ).rejects.toBeInstanceOf(AuthorizationClientError);

    await expect(
      authorizeServer({
        clientId: "manual-client",
        clientSecret: "",
        registerClient: async () => {
          throw new Error("Registration failed");
        },
        start,
      }),
    ).rejects.toBeInstanceOf(AuthorizationClientError);
  });

  it("localizes known authorization errors and safely handles unknown codes", () => {
    const localized = renderToStaticMarkup(
      <LocaleContext.Provider value="zh-CN">
        <AuthorizationErrorBanner
          errorCode="pkce_unsupported"
          errorMessage="Authorization server does not support PKCE S256"
        />
      </LocaleContext.Provider>,
    );
    expect(localized).toContain("授权服务器不支持所需的 PKCE S256");
    expect(localized).not.toContain("Authorization server does not support");

    const fallback = renderToStaticMarkup(
      <AuthorizationErrorBanner errorCode="future_error" errorMessage="Authorization was denied" />,
    );
    expect(fallback).toContain("Authorization was denied");
    expect(fallback).not.toContain("future_error");
  });
});
