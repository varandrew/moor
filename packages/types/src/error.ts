export type ApiErrorCode =
  | "NOT_FOUND"
  | "VALIDATION_ERROR"
  | "ACTIVE_PROFILE"
  | "SERVER_NOT_RUNNING"
  | "SERVER_BUSY"
  | "SERVER_CHANGED"
  | "UPDATE_CHECK_FAILED"
  | "ORDER_INVALID"
  | "PAYLOAD_TOO_LARGE"
  | "INTERNAL_ERROR";

export interface ApiError {
  code: ApiErrorCode;
  message: string;
}
