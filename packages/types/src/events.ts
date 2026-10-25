import type { ServerStatus } from "./server.js";
import type { Settings } from "./settings.js";
import type { ServerHealth } from "./health.js";
import type { ServerAuthorization } from "./authorization.js";

export interface ServerHealthEvent {
  type: "server:health";
  data: ServerHealth;
}

export interface ServerStatusEvent {
  type: "server:status";
  data: {
    serverId: string;
    status: ServerStatus;
    errorMessage?: string | null;
  };
}

export interface ServerToolsEvent {
  type: "server:tools";
  data: {
    serverId: string;
  };
}

export interface ProfileActivatedEvent {
  type: "profile:activated";
  data: {
    profileId: string;
  };
}

export interface ToolsChangedEvent {
  type: "tools:changed";
  data: {
    reason: string;
  };
}

export interface SettingsChangedEvent {
  type: "settings:changed";
  data: Settings;
}

export interface ServerAuthorizationEvent {
  type: "server:authorization";
  data: ServerAuthorization;
}

export type MoorEvent =
  | ServerHealthEvent
  | ServerStatusEvent
  | ServerAuthorizationEvent
  | ServerToolsEvent
  | ProfileActivatedEvent
  | ToolsChangedEvent
  | SettingsChangedEvent;

export type MoorEventType = MoorEvent["type"];

export type MoorEventData<T extends MoorEventType> = Extract<MoorEvent, { type: T }>["data"];
