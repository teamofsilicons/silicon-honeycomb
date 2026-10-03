import { applicationPath, parseApplicationRoute } from "./application-route";

export type ApplicationTab = "overview" | "reviews" | "release" | "access";
const segments: Record<ApplicationTab, string> = { overview: "", reviews: "/reviews", release: "/releases", access: "/access" };
export function consoleApplicationPath(appId: string, tab: string = "overview") {
  return applicationPath(appId) + (segments[tab as ApplicationTab] || "");
}
export function parseConsoleApplicationRoute(path: string) {
  const match = /^\/apps\/([^/]+)(?:\/(reviews|releases|access))?\/?$/.exec(path);
  if (!match) return;
  const app = parseApplicationRoute(`/apps/${match[1]}`);
  if (!app) return;
  return { ...app, tab: (match[2] === "releases" ? "release" : match[2] || "overview") as ApplicationTab };
}
export type DiscussionRoute = { id: string; provider: string; direction: "received" | "sent" };
export function discussionPath(item: { id: string; provider: string }, direction: "received" | "sent") {
  return `/requests/${direction}/${encodeURIComponent(item.id)}/${encodeURIComponent(item.provider)}`;
}
export function parseDiscussionRoute(path: string): DiscussionRoute | undefined {
  const match = /^\/requests\/(received|sent)\/([^/]+)\/([^/]+)\/?$/.exec(path);
  if (!match) return;
  try {
    const id = decodeURIComponent(match[2]), provider = decodeURIComponent(match[3]);
    if (!id || !provider || /[\/?#\u0000-\u001f]/.test(id + provider)) return;
    return { id, provider, direction: match[1] as "received" | "sent" };
  } catch { return; }
}
export const consoleViews: Record<string, string> = {
  applications: "/", bundles: "/bundles", drafts: "/drafts", environments: "/environments",
  "ata-verifications": "/app-to-app", "review-requests": "/requests/received",
  "sent-requests": "/requests/sent", docs: "/get-started", "request-link": "/requests",
};
export function viewForPath(path: string) {
  if (parseDiscussionRoute(path)) return "discussion";
  return Object.entries(consoleViews).find(([, value]) => value === path.replace(/\/$/, "") || (value === "/" && path === "/"))?.[0] || "applications";
}
