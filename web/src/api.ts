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
export async function request<T = any>(
  path: string,
  options: RequestInit = {},
): Promise<T> {
  const headers = new Headers(options.headers);
  if (options.body && !(options.body instanceof File))
    headers.set("Content-Type", "application/json");
  if (options.method && !["GET", "HEAD"].includes(options.method) && !headers.has("Idempotency-Key"))
    headers.set("Idempotency-Key", crypto.randomUUID());
  const response = await fetch(path, {
    ...options,
    headers,
    credentials: "same-origin",
  });
  const text = await response.text();
  let value: any;
  try {
    value = JSON.parse(text);
  } catch {
    value = {
      error: { message: "The service returned an unexpected response." },
    };
  }
  if (!response.ok) {
    throw new Error(
      [
        value.error?.message || `Request failed (${response.status})`,
        ...(value.error?.details || []),
      ].join("\n"),
    );
  }
  return value;
}
export function endpoint(id: string) {
  return `/api/v1/apps/${encodeURIComponent(id)}`;
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
