import { parseDiscussionRoute } from "../src/console-route.ts";
import { parseApplicationRoute } from "../src/application-route.ts";
export function loginReturnPath(value: unknown): string {
  if (
    typeof value !== "string" ||
    value.length > 2048 ||
    !value.startsWith("/") ||
    value.startsWith("//") ||
    /[\\\r\n]/.test(value)
  )
    return "/";
  try {
    const url = new URL(value, "https://honeycomb.invalid");
    if (url.origin !== "https://honeycomb.invalid") return "/";
    if (
      !["/", "/requests", "/requests/received", "/requests/sent"].includes(
        url.pathname,
      ) &&
      !parseApplicationRoute(url.pathname) &&
      !parseDiscussionRoute(url.pathname)
    )
      return "/";
    return url.pathname + url.search;
  } catch {
    return "/";
  }
}
