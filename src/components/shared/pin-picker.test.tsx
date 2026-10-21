import { describe, expect, it } from "vite-plus/test";
import { renderToStaticMarkup } from "react-dom/server";
import type { ProfileToolGroup } from "@moor/types";
import { PinPicker } from "./PinPicker";
import { RiskBadge } from "./RiskBadge";

const groups: ProfileToolGroup[] = [
  {
    serverId: "server-1",
    serverName: "Alpha",
    serverEnabled: true,
    tools: [
      {
        toolName: "search",
        exposedName: "alpha__search",
        description: null,
        inputSchema: null,
        disabled: false,
        pinned: false,
        riskLevel: "safe",
        riskSource: "heuristic",
      },
      {
        toolName: "hidden",
        exposedName: "alpha__hidden",
        description: null,
        inputSchema: null,
        disabled: true,
        pinned: false,
        riskLevel: "destructive",
        riskSource: "user",
      },
    ],
  },
];

describe("RiskBadge", () => {
  it("labels the level", () => {
    expect(renderToStaticMarkup(<RiskBadge level="destructive" source="heuristic" />)).toContain(
      ">Destructive<",
    );
    expect(renderToStaticMarkup(<RiskBadge level="unknown" source="heuristic" />)).toContain(
      ">Unknown risk<",
    );
  });

  it("marks a user-corrected level", () => {
    const markup = renderToStaticMarkup(<RiskBadge level="safe" source="user" />);

    expect(markup).toContain("Safe ·");
    expect(markup).toContain("Corrected by you");
  });
});

describe("PinPicker", () => {
  it("offers only enabled tools and reports the selection count", () => {
    const markup = renderToStaticMarkup(
      <PinPicker
        groups={groups}
        selection={[{ serverId: "server-1", toolName: "search" }]}
        busy={false}
        onToggle={() => undefined}
        onConfirm={() => undefined}
        onCancel={() => undefined}
      />,
    );

    expect(markup).toContain("Pick the tools to keep");
    expect(markup).toContain(">search<");
    expect(markup).not.toContain(">hidden<");
    expect(markup).toContain("1 selected");
    expect(markup).toContain("Switch to curated");
  });
});
