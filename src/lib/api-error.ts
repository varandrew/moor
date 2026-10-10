export class ApiRequestError extends Error {
  constructor(
    message: string,
    readonly code: string,
    readonly status?: number,
    cause?: unknown,
  ) {
    super(message);
    (this as Error & { cause?: unknown }).cause = cause;
    this.name = "ApiRequestError";
  }
}

export const API_ERROR_SUMMARIES: Record<string, string> = {
  NOT_FOUND: "The requested item was not found.",
  VALIDATION_ERROR: "Check the configuration values.",
  ACTIVE_PROFILE: "The active profile cannot be removed.",
  SERVER_NOT_RUNNING: "Start the server before checking its health.",
  SERVER_BUSY: "The server is busy. Try again after the current request.",
  SERVER_CHANGED: "The server changed during the check. Try again.",
  UPDATE_CHECK_FAILED: "Unable to check for updates.",
  ORDER_INVALID: "Unable to save the server order.",
  PAYLOAD_TOO_LARGE: "The configuration is too large.",
  INTERNAL_ERROR: "Moor could not complete this operation.",
  NETWORK_ERROR: "Unable to connect to Moor. Check that it is running.",
};
