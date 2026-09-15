let source: "library" | "console" = "library";
let pending = 0;
let authenticated = false;
export function setTelemetryAuthenticated(value: boolean) { authenticated = value; }
export function telemetryEnabled() {
  try { return localStorage.getItem("honeycomb.telemetry") !== "false"; }
  catch { return !document.cookie.split("; ").includes("honeycomb_telemetry=false"); }
}
export function setTelemetry(enabled: boolean) {
  try { localStorage.setItem("honeycomb.telemetry", String(enabled)); } catch { /* The cookie still supports the preference. */ }
  document.cookie = `honeycomb_telemetry=${enabled}; Path=/; Max-Age=31536000; SameSite=Lax${location.protocol === "https:" ? "; Secure" : ""}`;
}
export function setTelemetrySource(site: "library" | "console") { source = site; }
export function diagnostic(event: "page_view" | "http_completed", action: string, duration: number, success: boolean) {
  if (!authenticated || !telemetryEnabled() || pending >= 8) return;
  const allowed = ["apps", "releases", "publication", "drafts", "environments", "operations", "search", "iam", "login", "logout", "review", "report", "star", "catalog", "applications", "application", "review_inbox", "sign_in"];
  pending++;
  void fetch("/api/v1/telemetry", { method: "POST", credentials: "same-origin", keepalive: true,
    headers: { "Content-Type": "application/json", "X-Honeycomb-Telemetry": "true" },
    body: JSON.stringify({source, event, action: allowed.includes(action) ? action : "unknown", duration_ms: Math.min(86_400_000, Math.max(0, Math.round(duration))), success}),
    signal: AbortSignal.timeout(1000),
  }).catch(() => {}).finally(() => { pending--; });
}
export function requestAction(path: string) {
  const family = path.match(/^\/api\/v1\/([a-z-]+)(?:[/?]|$)/)?.[1];
  return family === "reports" ? "report" : family || "unknown";
}
