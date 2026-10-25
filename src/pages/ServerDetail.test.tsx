import { describe, expect, it } from "vite-plus/test";
import { renderToStaticMarkup } from "react-dom/server";
import { hasChanges, type EditForm } from "@/lib/server-form";
import { ServerEditFields } from "./ServerDetail";
import { ToolCategoryBadge } from "@/components/shared/ToolCategoryBadge";
import { AuthorizationBadge, AuthorizationButton } from "@/components/servers/ServerAuthorization";

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
  });
});
