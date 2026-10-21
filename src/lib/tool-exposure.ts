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

/** 直出模式下提示「考虑策展」的工具数阈值。 */
export const CURATION_HINT_THRESHOLD = 24;

/** 记录用户已关闭策展提示。 */
export const CURATION_HINT_STORAGE_KEY = "moor-curation-hint-dismissed";

export interface ToolSelection {
  serverId: string;
  toolName: string;
}

/**
 * 切换到策展模式时的默认置顶建议：每个启用的 Server 挑前 `perServer` 个可用工具。
 *
 * 不默认全选：全选等于没策展；也不默认空选：空目录会让 Agent 看不到任何工具。
 */
export function suggestDefaultPins(groups: ProfileToolGroup[], perServer = 1): ToolSelection[] {
  return groups.flatMap((group) => {
    if (!group.serverEnabled) return [];
    return group.tools
      .filter((tool) => !tool.disabled)
      .slice(0, perServer)
      .map((tool) => ({ serverId: group.serverId, toolName: tool.toolName }));
  });
}

/** 直出模式下工具已经很多时，提示用户考虑切到策展模式。 */
export function shouldSuggestCuration(
  totalTools: number,
  mode: ExposureMode,
  dismissed: boolean,
): boolean {
  return mode === "direct" && !dismissed && totalTools >= CURATION_HINT_THRESHOLD;
}
