import { createContext, useContext, useEffect, useRef, useCallback, type ReactNode } from "react";
import { getApiRuntime, buildApiUrl, buildApiHeaders, resetRuntime } from "@/lib/api/runtime";
import type { MoorEvent, MoorEventData, MoorEventType, ServerStatus } from "@moor/types";

interface SSEContextValue {
  subscribe: <T extends MoorEventType>(
    eventType: T,
    handler: (data: MoorEventData<T>) => void,
  ) => () => void;
}

const SSEContext = createContext<SSEContextValue | null>(null);

type WarnFn = (message: string) => void;
const serverStatuses = new Set<ServerStatus>(["stopped", "starting", "running", "error"]);

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function invalidEvent(event: string, warn: WarnFn): null {
  warn(`Dropped invalid SSE payload for ${event}`);
  return null;
}

function isServerStatusEvent(data: unknown): data is MoorEventData<"server:status"> {
  return (
    isRecord(data) &&
    typeof data.serverId === "string" &&
    typeof data.status === "string" &&
    serverStatuses.has(data.status as ServerStatus) &&
    (data.errorMessage === undefined ||
      data.errorMessage === null ||
      typeof data.errorMessage === "string")
  );
}

function isServerToolsEvent(data: unknown): data is MoorEventData<"server:tools"> {
  return isRecord(data) && typeof data.serverId === "string";
}

function isProfileActivatedEvent(data: unknown): data is MoorEventData<"profile:activated"> {
  return isRecord(data) && typeof data.profileId === "string";
}

// 只钉顶层 envelope：新增 settings 组无需改这里；组成员的形状由消费侧
// 走 GET 的类型化响应保证（见 useSettings 的 invalidate 策略）。
function isSettingsChangedEvent(data: unknown): data is MoorEventData<"settings:changed"> {
  return isRecord(data) && typeof data.version === "number";
}

export function parseMoorSSEEvent(
  event: string,
  data: unknown,
  warn: WarnFn = (message) => console.warn(message),
): MoorEvent | null {
  if (event === "connected") return null;

  switch (event) {
    case "server:status":
      return isServerStatusEvent(data) ? { type: event, data } : invalidEvent(event, warn);
    case "server:health":
      return isRecord(data) &&
        typeof data.serverId === "string" &&
        ["unknown", "healthy", "unhealthy", "unsupported"].includes(String(data.status)) &&
        (data.checkedAt === null || typeof data.checkedAt === "string") &&
        typeof data.consecutiveFailures === "number" &&
        Number.isInteger(data.consecutiveFailures) &&
        data.consecutiveFailures >= 0 &&
        (data.errorMessage === null || typeof data.errorMessage === "string")
        ? { type: event, data: data as unknown as MoorEventData<"server:health"> }
        : invalidEvent(event, warn);
    case "server:tools":
      return isServerToolsEvent(data) ? { type: event, data } : invalidEvent(event, warn);
    case "profile:activated":
      return isProfileActivatedEvent(data) ? { type: event, data } : invalidEvent(event, warn);
    case "settings:changed":
      return isSettingsChangedEvent(data) ? { type: event, data } : invalidEvent(event, warn);
    default:
      return invalidEvent(event, warn);
  }
}

export function SSEProvider({ children }: { children: ReactNode }) {
  const handlersRef = useRef(new Map<MoorEventType, Set<(data: unknown) => void>>());

  useEffect(() => {
    const controller = new AbortController();
    const scheduleReconnect = () => {
      setTimeout(() => {
        if (!controller.signal.aborted) void connect();
      }, 5000);
    };

    async function connect() {
      try {
        const runtime = await getApiRuntime();
        const url = buildApiUrl(runtime, "/api/events");
        const headers = buildApiHeaders(runtime, {
          Accept: "text/event-stream",
        });
        const response = await fetch(url, {
          headers,
          signal: controller.signal,
        });
        if (!response.ok || !response.body) throw new Error(`SSE failed: ${response.status}`);

        const reader = response.body.getReader();
        const decoder = new TextDecoder();
        let buffer = "";

        while (!controller.signal.aborted) {
          const { done, value } = await reader.read();
          if (done) break;
          buffer += decoder.decode(value, { stream: true });
          const events = buffer.split("\n\n");
          buffer = events.pop() ?? "";

          for (const chunk of events) {
            const event = chunk
              .split("\n")
              .find((line) => line.startsWith("event: "))
              ?.slice(7);
            const dataLine = chunk.split("\n").find((line) => line.startsWith("data: "));
            if (!event || !dataLine) continue;

            let data: unknown;
            try {
              data = JSON.parse(dataLine.slice(6));
            } catch {
              console.warn(`Dropped invalid SSE JSON for ${event}`);
              continue;
            }

            const typedEvent = parseMoorSSEEvent(event, data);
            if (!typedEvent) continue;

            const handlers = handlersRef.current.get(typedEvent.type);
            if (handlers) {
              for (const handler of handlers) {
                handler(typedEvent.data);
              }
            }
          }
        }
        if (!controller.signal.aborted) scheduleReconnect();
      } catch (err) {
        if (err instanceof DOMException && err.name === "AbortError") return;
        resetRuntime();
        scheduleReconnect();
      }
    }

    void connect();
    return () => controller.abort();
  }, []);

  const subscribe = useCallback(
    <T extends MoorEventType>(eventType: T, handler: (data: MoorEventData<T>) => void) => {
      if (!handlersRef.current.has(eventType)) {
        handlersRef.current.set(eventType, new Set());
      }
      const unknownHandler = handler as (data: unknown) => void;
      handlersRef.current.get(eventType)!.add(unknownHandler);
      return () => {
        handlersRef.current.get(eventType)?.delete(unknownHandler);
      };
    },
    [],
  );

  return <SSEContext.Provider value={{ subscribe }}>{children}</SSEContext.Provider>;
}

export function useSSEEvent<T extends MoorEventType>(
  eventType: T,
  handler: (data: MoorEventData<T>) => void,
) {
  const ctx = useContext(SSEContext);
  if (!ctx) throw new Error("useSSEEvent must be used within SSEProvider");

  const handlerRef = useRef(handler);
  handlerRef.current = handler;

  useEffect(() => {
    return ctx.subscribe(eventType, (data) => handlerRef.current(data));
  }, [ctx, eventType]);
}
