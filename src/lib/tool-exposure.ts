import type { ExposureMode, ProfileToolGroup, ToolDetail } from "@moor/types";

export type { ExposureMode };

/**
 * 治理面板的曝光汇总。
 *
 * 注意：这里只回答「如果 Server 正在运行，哪些工具会出现在 tools/list」。
 * 运行状态不在面板数据里，所以 listed 是上限而不是实时值。
 */
export interface ExposureSummary {
  listed: number;
  total: number;
  pinned: number;
}

/** 单个工具在给定模式下是否会出现在 tools/list。 */
export function isToolListed(
  tool: ToolDetail,
  mode: ExposureMode,
  serverEnabled: boolean,
): boolean {
  if (!serverEnabled || tool.disabled) return false;
  return mode === "direct" || tool.pinned;
}

/** 按 Profile 的全部工具组汇总直出数、总数与置顶数。 */
export function summarizeExposure(groups: ProfileToolGroup[], mode: ExposureMode): ExposureSummary {
  return groups.reduce<ExposureSummary>(
    (summary, group) => {
      summary.total += group.tools.length;
      summary.pinned += group.tools.filter((tool) => tool.pinned).length;
      summary.listed += group.tools.filter((tool) =>
        isToolListed(tool, mode, group.serverEnabled),
      ).length;
      return summary;
    },
    { listed: 0, total: 0, pinned: 0 },
  );
}

/** 策展模式下一个都没置顶时，Agent 会看到空目录——值得显式警告。 */
export function needsPinWarning(mode: ExposureMode, summary: ExposureSummary): boolean {
  return mode === "curated" && summary.pinned === 0;
}
