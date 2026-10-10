import type { SidecarInfo } from "@moor/types";
import { ApiRequestError } from "@/lib/api-error";
import { getApiRuntime, refreshApiRuntime, buildApiUrl, buildApiHeaders } from "./runtime";
import {
  formatApiNetworkError,
  formatApiRetryError,
  readApiFailure,
  parseApiResponse,
  isAbortError,
  shouldRetryNetworkError,
} from "./errors";

export interface RequestOptions extends RequestInit {
  signal?: AbortSignal;
}

async function fetchWithRuntime(
  path: string,
  options: RequestOptions | undefined,
  runtime: SidecarInfo,
): Promise<Response> {
  return fetch(buildApiUrl(runtime, path), {
    ...options,
    headers: buildApiHeaders(runtime, options?.headers),
    signal: options?.signal,
  });
}

async function retryWithFreshRuntime<T>(
  path: string,
  options: RequestOptions | undefined,
  originalError: unknown,
): Promise<T> {
  const runtime = await refreshApiRuntime();
  try {
    const retryResp = await fetchWithRuntime(path, options, runtime);
    if (!retryResp.ok) {
      throw await readApiFailure(retryResp);
    }
    return retryResp.json() as Promise<T>;
  } catch (retryErr) {
    if (retryErr instanceof ApiRequestError) throw retryErr;
    throw new ApiRequestError(
      formatApiRetryError(path, runtime, originalError, retryErr),
      "NETWORK_ERROR",
      undefined,
      originalError,
    );
  }
}

export async function api<T>(path: string, options?: RequestOptions): Promise<T> {
  const runtime = await getApiRuntime();
  let resp: Response;
  try {
    resp = await fetchWithRuntime(path, options, runtime);
  } catch (err) {
    if (isAbortError(err, options?.signal)) {
      throw err;
    }
    const networkError = new ApiRequestError(
      formatApiNetworkError(path, err, runtime),
      "NETWORK_ERROR",
      undefined,
      err,
    );
    if (!shouldRetryNetworkError(options)) {
      throw networkError;
    }
    return retryWithFreshRuntime<T>(path, options, networkError);
  }
  if (resp.status === 401) {
    return retryWithFreshRuntime<T>(path, options, await readApiFailure(resp));
  }
  return parseApiResponse<T>(resp);
}

export async function apiPost<T>(path: string, body: unknown): Promise<T> {
  return api<T>(path, {
    method: "POST",
    body: JSON.stringify(body),
  });
}

export async function apiPut<T>(path: string, body: unknown): Promise<T> {
  return api<T>(path, {
    method: "PUT",
    body: JSON.stringify(body),
  });
}

export async function apiDelete<T>(path: string): Promise<T> {
  return api<T>(path, { method: "DELETE" });
}
