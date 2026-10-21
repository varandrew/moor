export interface ToolDetail {
  toolName: string;
  exposedName: string;
  description: string | null;
  inputSchema: unknown;
  disabled: boolean;
  /** 是否被当前 Profile 置顶。策展模式下只有置顶工具会出现在 tools/list。 */
  pinned: boolean;
  riskLevel: RiskLevel;
  /** heuristic = 推断，user = 用户纠正过。 */
  riskSource: "heuristic" | "user";
}

export type RiskLevel = "safe" | "write" | "destructive" | "unknown";
export type RiskPolicy = "advisory" | "block_destructive";
