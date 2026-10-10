import { translateCurrentText } from "@/lib/messages";
import { ApiRequestError, API_ERROR_SUMMARIES } from "@/lib/api-error";
import { clsx, type ClassValue } from "clsx";
import { twMerge } from "tailwind-merge";

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
  return [translateCurrentText(summary), ...details].join("\n");
}
