export interface ToolDetail {
  toolName: string;
  exposedName: string;
  description: string | null;
  inputSchema: unknown;
  disabled: boolean;
  /** 是否被当前 Profile 置顶。策展模式下只有置顶工具会出现在 tools/list。 */
  pinned: boolean;
}
