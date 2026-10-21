import type { ExposureMode, ProfileToolGroup, RiskPolicy, ToolDetail } from "@moor/types";

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

/**
 * 单个工具在给定模式下是否会出现在 tools/list。
 *
 * 与网关的判定顺序一致：禁用 → 风险策略 → 置顶/直出。
 */
export function isToolListed(
  tool: ToolDetail,
  mode: ExposureMode,
  serverEnabled: boolean,
  riskPolicy: RiskPolicy,
): boolean {
  if (!serverEnabled || tool.disabled) return false;
  if (riskPolicy === "block_destructive" && tool.riskLevel === "destructive") return false;
  return mode === "direct" || tool.pinned;
}

/** 按 Profile 的全部工具组汇总直出数、总数与置顶数。 */
export function summarizeExposure(
  groups: ProfileToolGroup[],
  mode: ExposureMode,
  riskPolicy: RiskPolicy,
): ExposureSummary {
  return groups.reduce<ExposureSummary>(
    (summary, group) => {
      summary.total += group.tools.length;
      summary.pinned += group.tools.filter((tool) => tool.pinned).length;
      summary.listed += group.tools.filter((tool) =>
        isToolListed(tool, mode, group.serverEnabled, riskPolicy),
      ).length;
      return summary;
    },
    { listed: 0, total: 0, pinned: 0 },
  );
}

/**
 * 策展模式下 Agent 一个工具都看不到时给出警告。
 *
 * 只看 listed 而不是 pinned：置顶工具也可能全被禁用或被风险策略拦下。
 */
export function needsPinWarning(mode: ExposureMode, summary: ExposureSummary): boolean {
  return mode === "curated" && summary.listed === 0;
}
