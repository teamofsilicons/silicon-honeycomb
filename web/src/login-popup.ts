export type IdentityKind = "carbon" | "silicon";
export function isLoginCompletion(event: Pick<MessageEvent, "origin" | "source" | "data">, popup: Window, origin: string, state: string) {
  return event.origin === origin && event.source === popup
    && event.data?.type === "honeycomb-login-complete" && event.data?.state === state;
}
export function loginWithPopup(kind: IdentityKind): Promise<void> {
  const state = crypto.randomUUID();
  const query = new URLSearchParams({ identity_kind: kind, popup: "1", popup_nonce: state, next: location.pathname + location.search });
  const popup = window.open(`/auth/login?${query}`, `honeycomb-login-${state}`, "popup,width=540,height=760");
  if (!popup) return Promise.reject(new Error("Allow pop-ups for Honeycomb, then try signing in again."));
  return new Promise((resolve, reject) => {
    const stop = () => { window.removeEventListener("message", receive); clearInterval(closed); clearTimeout(timeout); };
    const receive = (event: MessageEvent) => {
      if (!isLoginCompletion(event, popup, location.origin, state)) return;
      stop(); popup.close(); resolve();
    };
    const closed = setInterval(() => {
      if (popup.closed) { stop(); reject(new Error("The sign-in window was closed. You can try again when ready.")); }
    }, 600);
    const timeout = setTimeout(() => { stop(); popup.close(); reject(new Error("Sign-in expired. Please start again.")); }, 10 * 60 * 1000);
    window.addEventListener("message", receive);
  });
}
