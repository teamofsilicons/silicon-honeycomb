export type ApplicationRoute = {
  appId: string;
  channel?: "prod" | "dev";
  version?: string;
};
export function applicationPath(
  appId: string,
  channel?: "prod" | "dev",
  version?: string,
) {
  const path = `/apps/${encodeURIComponent(appId)}`;
  return channel && version
    ? `${path}/releases/${channel}/${encodeURIComponent(version)}`
    : path;
}
export function parseApplicationRoute(
  path: string,
): ApplicationRoute | undefined {
  const match = /^\/apps\/([^/]+)(?:\/releases\/(prod|dev)\/([^/]+))?\/?$/.exec(
    path,
  );
  if (!match) return;
  try {
    const appId = decodeURIComponent(match[1]);
    const version = match[3] && decodeURIComponent(match[3]);
    if (
      !/^[a-z][a-z0-9_-]{0,79}$/.test(appId) ||
      (version && !/^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$/.test(version))
    )
      return;
    return {
      appId,
      ...(version ? { channel: match[2] as "prod" | "dev", version } : {}),
    };
  } catch {
    return;
  }
}
