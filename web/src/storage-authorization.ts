import { request } from "./api";
export type StorageAuthorization = {
  authorization_id: string;
  consent_url: string;
  state: string;
};
export function trustedConsentUrl(value: unknown): string {
  if (typeof value !== "string")
    throw new Error("IAM returned an invalid permission link.");
  const url = new URL(value);
  if (
    url.username ||
    url.password ||
    url.hash ||
    !(
      url.protocol === "https:" ||
      (url.protocol === "http:" &&
        ["localhost", "127.0.0.1", "[::1]"].includes(url.hostname))
    )
  )
    throw new Error("IAM returned an invalid permission link.");
  return url.href;
}
export function isStorageCompletion(
  event: Pick<MessageEvent, "origin" | "source" | "data">,
  popup: Window,
  origin: string,
  authorization: StorageAuthorization,
) {
  return (
    event.origin === origin &&
    event.source === popup &&
    event.data?.type === "honeycomb-storage-authorized" &&
    event.data?.authorization_id === authorization.authorization_id &&
    event.data?.state === authorization.state
  );
}
/** Credentials stay in the backend; the popup only returns an authenticated completion signal. */
export async function authorizeStorage(orgId: string, redirectUrl?: string): Promise<void> {
  const popup = window.open(
    "about:blank",
    "honeycomb-storage-authorization",
    "popup,width=780,height=820",
  );
  if (!popup)
    throw new Error(
      "Allow this site's pop-up window to review storage permissions, then try again.",
    );
  let authorization: StorageAuthorization;
  try {
    authorization = await request("/api/v1/storage-authorizations", {
      method: "POST",
      body: JSON.stringify({
        org_id: orgId,
        return_url: window.location.origin + "/storage-authorization",
        ...(redirectUrl ? { redirect_url: new URL(redirectUrl, window.location.origin).href } : {}),
      }),
    });
    const consent = new URL(trustedConsentUrl(authorization.consent_url));
    consent.searchParams.set("display", "popup");
    if (!authorization.authorization_id || !authorization.state)
      throw new Error("IAM could not start storage authorization.");
    await new Promise<void>((resolve, reject) => {
      let completed = false;
      const stop = () => {
        completed = true;
        window.removeEventListener("message", receive);
        clearInterval(closed);
        clearTimeout(timeout);
      };
      const receive = (event: MessageEvent) => {
        if (
          isStorageCompletion(
            event,
            popup,
            window.location.origin,
            authorization,
          )
        ) {
          stop();
          if (event.data.error)
            reject(
              new Error(
                "Storage permission was not approved. You can review it again when ready.",
              ),
            );
          else {
            resolve();
            if (redirectUrl) {
              const target = new URL(redirectUrl, window.location.origin);
              if (target.origin === window.location.origin) window.location.assign(target.href);
            }
          }
        }
      };
      const closed = setInterval(() => {
        if (popup.closed && !completed) {
          stop();
          reject(
            new Error(
              "The permission window was closed. Review storage access to continue.",
            ),
          );
        }
      }, 600);
      const timeout = setTimeout(
        () => {
          stop();
          reject(new Error("Storage authorization expired. Please try again."));
        },
        10 * 60 * 1000,
      );
      window.addEventListener("message", receive);
      popup.location.assign(consent.href);
    });
    popup.close();
  } catch (error) {
    popup.close();
    throw error;
  }
}
