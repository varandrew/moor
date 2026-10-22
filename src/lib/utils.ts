import { translateCurrentText } from "@/lib/messages";
import { ApiRequestError, API_ERROR_SUMMARIES } from "@/lib/api-error";
import { clsx, type ClassValue } from "clsx";
import { twMerge } from "tailwind-merge";

// 仅翻译 Moor 的固定说明，下游原始诊断保留原文。
const localizedApiDetails: Record<string, string> = {
  SERVER_CHANGED: "Server changed while refreshing tools. Refresh again.",
  TOOL_DISCOVERY_FAILED: "Failed to refresh tools from the server.",
  INTERNAL_ERROR: "Failed to save discovered tools.",
};

export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs));
}

export function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

export function createErrorWithCause(message: string, cause: unknown): Error {
  const error = new Error(message) as Error & { cause?: unknown };
  error.cause = cause;
  return error;
}

export function getErrorMessage(err: unknown, fallback = "Unknown error"): string {
  if (err instanceof ApiRequestError) {
    const summary = API_ERROR_SUMMARIES[err.code] ?? "Moor could not complete this operation.";
    return `${summary}\n${err.message}`;
  }
  return err instanceof Error ? err.message : fallback;
}

export function getNoticeErrorMessage(error: unknown): string {
  const [summary, ...details] = getErrorMessage(error).split("\n");
  const translateDetails =
    error instanceof ApiRequestError && localizedApiDetails[error.code] === error.message;
  return [
    translateCurrentText(summary),
    ...details.map((detail) => (translateDetails ? translateCurrentText(detail) : detail)),
  ].join("\n");
}
