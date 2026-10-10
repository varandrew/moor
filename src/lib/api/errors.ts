import { ApiRequestError } from "@/lib/api-error";
import type { SidecarInfo } from "@moor/types";
import { isRecord } from "@/lib/utils";

export function formatApiNetworkError(path: string, err: unknown, runtime?: SidecarInfo): string {
  const detail = err instanceof Error ? err.message : String(err);
  const target = runtime ? ` at ${runtime.baseUrl}` : "";
  return `Unable to connect to the Moor sidecar while requesting ${path}${target}. Check that Moor is running and the Sidecar API port/token are current. Original error: ${detail}`;
}

export function formatApiRetryError(
  path: string,
  runtime: SidecarInfo,
  original: unknown,
  retryFailure: unknown,
): string {
  const originalDetail = original instanceof Error ? original.message : String(original);
  const retryDetail = retryFailure instanceof Error ? retryFailure.message : String(retryFailure);
  return `Unable to connect to the Moor sidecar while requesting ${path} at ${runtime.baseUrl} after refreshing runtime. Original error: ${originalDetail}. Retry error: ${retryDetail}`;
}

export async function readApiFailure(resp: Response): Promise<ApiRequestError> {
  const parsed = (await resp.json().catch(() => null)) as unknown;
  let code = "UNKNOWN_ERROR";
  let message = resp.statusText || `API error: ${resp.status}`;
  if (isRecord(parsed)) {
    if (typeof parsed.error === "string") message = parsed.error;
    if (isRecord(parsed.error)) {
      if (typeof parsed.error.code === "string" && parsed.error.code) code = parsed.error.code;
      if (typeof parsed.error.message === "string" && parsed.error.message)
        message = parsed.error.message;
      else if (code !== "UNKNOWN_ERROR") message = code;
    }
  }
  return new ApiRequestError(message, code, resp.status);
}

export async function readApiError(resp: Response): Promise<string> {
  return (await readApiFailure(resp)).message;
}

export async function parseApiResponse<T>(resp: Response): Promise<T> {
  if (!resp.ok) {
    throw await readApiFailure(resp);
  }
  return resp.json() as Promise<T>;
}

export function isAbortError(err: unknown, signal?: AbortSignal): boolean {
  return signal?.aborted === true || (isRecord(err) && err.name === "AbortError");
}

export function shouldRetryNetworkError(options?: RequestInit): boolean {
  const method = options?.method?.toUpperCase() ?? "GET";
  return method === "GET" || method === "HEAD";
}
