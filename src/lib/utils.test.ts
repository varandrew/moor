import { afterEach, describe, expect, it } from "vite-plus/test";
import { ApiRequestError } from "./api-error";
import { setMessageLocale } from "./messages";
import { getNoticeErrorMessage } from "./utils";

afterEach(() => setMessageLocale("en"));

describe("getNoticeErrorMessage", () => {
  it.each([
    {
      code: "SERVER_CHANGED",
      message: "Server changed while refreshing tools. Refresh again.",
      english: "The server changed during the check. Try again.",
      chinese: "检查期间服务器发生变更，请重试。\n刷新期间服务器已变化，请重新刷新。",
    },
    {
      code: "TOOL_DISCOVERY_FAILED",
      message: "Failed to refresh tools from the server.",
      english: "Unable to refresh the server tools.",
      chinese: "无法刷新服务器工具清单。\n无法从服务器刷新工具清单。",
    },
    {
      code: "INTERNAL_ERROR",
      message: "Failed to save discovered tools.",
      english: "Moor could not complete this operation.",
      chinese: "Moor 无法完成此操作。\n无法保存发现的工具。",
    },
  ])("localizes the Moor refresh explanation for $code", ({ code, message, english, chinese }) => {
    const error = new ApiRequestError(message, code);
    setMessageLocale("zh-CN");
    expect(getNoticeErrorMessage(error)).toBe(chinese);
    setMessageLocale("en");
    expect(getNoticeErrorMessage(error)).toBe(`${english}\n${message}`);
    expect(error.message).toBe(message);
  });

  it.each([
    ["INTERNAL_ERROR", "Save failed"],
    ["REMOTE_ERROR", "Failed to save discovered tools."],
    ["TOOL_DISCOVERY_FAILED", "Remote diagnostics\nFailed to refresh tools from the server."],
  ])("preserves downstream details for %s", (code, message) => {
    setMessageLocale("zh-CN");
    const notice = getNoticeErrorMessage(new ApiRequestError(message, code));
    expect(notice.slice(notice.indexOf("\n") + 1)).toBe(message);
  });
});
