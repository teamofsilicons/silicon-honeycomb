import { diagnostic, requestAction, telemetryEnabled } from "./telemetry";
export type AppRecord = {
  app_id: string;
  org_id: string;
  name: string;
  description: string;
  visibility: string;
  state: string;
  revision: number;
  iam_revision: number;
  effective_revision: number;
  effective_config: Record<string, any>;
  config: Record<string, any>;
  latest_version: string | null;
  rating: number;
  reviews: number;
  stars: number;
  installs: number;
};
export type Session = {
  authenticated: boolean;
  identity?: {
    principal_id: string;
    actor_type?: string;
    organizations: Record<string, string | null>;
  };
};
export type Config = {
  site: "library" | "console";
  libraryOrigin: string;
  consoleOrigin: string;
};
export class ApiError extends Error {
  constructor(message:string,public code?:string,public status?:number){super(message);this.name="ApiError";}
}
export async function request<T = any>(
  path: string,
  options: RequestInit = {},
): Promise<T> {
  const headers = new Headers(options.headers);
  headers.set("X-Honeycomb-Telemetry", String(telemetryEnabled()));
  if (options.body && !(options.body instanceof File))
    headers.set("Content-Type", "application/json");
  if (options.method && !["GET", "HEAD"].includes(options.method) && !headers.has("Idempotency-Key"))
    headers.set("Idempotency-Key", crypto.randomUUID());
  const started = performance.now();
  const response = await fetch(path, {
    ...options,
    headers,
    credentials: "same-origin",
  }).catch(error => { diagnostic("http_completed", requestAction(path), performance.now()-started, false); throw error; });
  diagnostic("http_completed", requestAction(path), performance.now()-started, response.ok);
  const text = await response.text();
  let value: any;
  try {
    value = JSON.parse(text);
  } catch {
    if (response.ok && response.status === 204) return undefined as T;
    throw new ApiError("The service returned an unexpected response. Please try again.", "invalid_response", response.status);
  }
  if (!response.ok) {
    throw new ApiError(
      [
        value.error?.message || `Request failed (${response.status})`,
        ...(Array.isArray(value.error?.details) ? value.error.details.filter((detail: unknown) => typeof detail === "string") : []),
      ].join("\n"),
      typeof value.error?.code === "string" ? value.error.code : undefined, response.status,
    );
  }
  return value;
}
export function endpoint(id: string) {
  return `/api/v1/apps/${encodeURIComponent(id)}`;
}
export function releaseEndpoint(id: string) {
  return `/api/v2/apps/${encodeURIComponent(id)}`;
}
export function safeLink(value: unknown): string | undefined {
  if (typeof value !== "string") return;
  try {
    const url = new URL(value);
    if (url.protocol === "https:" && !url.username && !url.password)
      return url.href;
  } catch {
    return;
  }
}
