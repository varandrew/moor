export type AuthorizationState =
  | "not_required"
  | "required"
  | "authorizing"
  | "authorized"
  | "error";

export interface ServerAuthorization {
  serverId: string;
  state: AuthorizationState;
  errorMessage?: string | null;
  issuer?: string | null;
  clientId?: string | null;
  scopes: string[];
}

export interface AuthorizationClientInput {
  clientId: string;
  clientSecret?: string;
}

export interface AuthorizationStart {
  authorizationUrl: string;
  expiresAt: string;
}

export interface RevocationResult {
  snapshot: ServerAuthorization;
  warning?: string | null;
}
