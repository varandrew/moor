import type { Server } from "./server.js";
import type { RiskPolicy, ToolDetail } from "./tool.js";

/** Profile 的曝光模式：全量直出，或只直出置顶工具。 */
export type ExposureMode = "direct" | "curated";

export interface Profile {
  id: string;
  name: string;
  isActive: boolean;
  /** 老库升级后恒为 direct；新 Profile 默认 direct。 */
  exposureMode: ExposureMode;
  /** advisory = 只标记；block_destructive = 拒绝破坏性工具。 */
  riskPolicy: RiskPolicy;
  serverCount?: number;
  createdAt: string;
  updatedAt: string;
}

export interface ProfileServerState {
  enabled: boolean;
  disabledTools: string[];
  pinnedTools: string[];
}

export interface ProfileDetail extends Profile {
  servers: Array<Server & { profileServer: ProfileServerState }>;
}

/** 治理面板分组：一个 server 在指定 profile 下的全部已发现工具及禁用态。 */
export interface ProfileToolGroup {
  serverId: string;
  serverName: string;
  serverEnabled: boolean;
  tools: ToolDetail[];
}

/** 批量更新 profile-server 状态的单项补丁；enabled 与 disabledTools 相互独立、可缺省。 */
export interface ProfileServerUpsert {
  serverId: string;
  enabled?: boolean;
  disabledTools?: string[];
  pinnedTools?: string[];
}
