import { describe, expect, it } from "vite-plus/test";
import { renderToStaticMarkup } from "react-dom/server";
import { createDefaultSettings } from "@moor/types";
import { LocaleContext } from "@/contexts/LocaleContext";
import { StatusBadge } from "@/components/shared/StatusBadge";
import { ErrorBanner } from "@/components/shared/ErrorBanner";
import { resolveLocale, readCachedLocale } from "./locale";
import { messages, translate } from "./messages";

describe("locale", () => {
  it("uses the primary system language and honors explicit overrides", () => {
    expect(resolveLocale("system", ["zh-TW", "en"])).toBe("zh-CN");
    expect(resolveLocale("system", ["fr", "zh-CN"])).toBe("en");
    expect(resolveLocale("system", [])).toBe("en");
    expect(resolveLocale("en", ["zh-CN"])).toBe("en");
    expect(resolveLocale("zh-CN", ["en"])).toBe("zh-CN");
    expect(createDefaultSettings().appearance.locale).toBe("system");
  });

  it("restores an override but tolerates old or unavailable local storage", () => {
    expect(readCachedLocale({ getItem: () => "zh-CN" })).toBe("zh-CN");
    expect(readCachedLocale({ getItem: () => null })).toBe("system");
    expect(readCachedLocale({ getItem: () => "invalid" })).toBe("system");
    expect(
      readCachedLocale({
        getItem: () => {
          throw new Error("Storage unavailable");
        },
      }),
    ).toBe("system");
  });

  it("keeps both dictionaries complete and preserves interpolation parameters", () => {
    for (const [key, value] of Object.entries(messages)) {
      expect(value.trim()).not.toBe("");
      const parameters = (text: string) =>
        [...text.matchAll(/\{(\w+)\}/g)].map((match) => match[1]).sort();
      expect(parameters(value), key).toEqual(parameters(key));
    }
    expect(translate("zh-CN", "{count} tools", { count: 3 })).toBe("3 个工具");
    expect(translate("en", "{count} tools", { count: 3 })).toBe("3 tools");
  });

  it("translates lifecycle and notices while keeping original diagnostics", () => {
    const render = (locale: "zh-CN" | "en") =>
      renderToStaticMarkup(
        <LocaleContext.Provider value={locale}>
          <StatusBadge status="running" />
          <ErrorBanner
            message={"Check the configuration values.\nadvanced.sidecarPort is invalid"}
          />
        </LocaleContext.Provider>,
      );
    expect(render("zh-CN")).toContain("运行中");
    expect(render("zh-CN")).toContain("请检查配置值。");
    expect(render("zh-CN")).toContain("advanced.sidecarPort is invalid");
    expect(render("en")).toContain("Running");
  });

  it("keeps every line of downstream diagnostics in both languages", () => {
    for (const locale of ["en", "zh-CN"] as const) {
      const markup = renderToStaticMarkup(
        <LocaleContext.Provider value={locale}>
          <ErrorBanner message={"Custom MCP failure\nUnderlying transport detail"} />
        </LocaleContext.Provider>,
      );
      expect(markup).toContain("Custom MCP failure");
      expect(markup).toContain("Underlying transport detail");
    }
  });
});
