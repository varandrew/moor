import type { ServerStatus } from "./server.js";
import type { Settings } from "./settings.js";
import type { ServerHealth } from "./health.js";

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

export interface SettingsChangedEvent {
  type: "settings:changed";
  data: Settings;
}

export type MoorEvent =
  | ServerHealthEvent
  | ServerStatusEvent
  | ServerToolsEvent
  | ProfileActivatedEvent
  | SettingsChangedEvent;

export type MoorEventType = MoorEvent["type"];

export type MoorEventData<T extends MoorEventType> = Extract<MoorEvent, { type: T }>["data"];
