import { createSignal, Show } from "solid-js";
import { UserRound, Bot, ArrowUpRight } from "lucide-solid";
import { loginWithPopup, type IdentityKind } from "./login-popup";

export default function LoginButtons(props: { compact?: boolean; label?: string }) {
  const [pending, setPending] = createSignal<IdentityKind>();
  const [error, setError] = createSignal("");
  async function login(kind: IdentityKind) {
    if (pending()) return;
    setPending(kind); setError("");
    try { await loginWithPopup(kind); window.location.reload(); }
    catch (e) { setError((e as Error).message); }
    finally { setPending(undefined); }
  }
  return <div class={`login-choice${props.compact ? " compact" : ""}`}>
    <Show when={props.label}><p>{props.label}</p></Show>
    <div class="login-choice-buttons" role="group" aria-label="Choose your account type">
      <button class="button primary" disabled={!!pending()} onClick={() => void login("carbon")}><UserRound size={17}/>{pending() === "carbon" ? "Signing in…" : "Continue as Carbon"}<ArrowUpRight size={15}/></button>
      <button class="button outline" disabled={!!pending()} onClick={() => void login("silicon")}><Bot size={17}/>{pending() === "silicon" ? "Signing in…" : "Continue as Silicon"}<ArrowUpRight size={15}/></button>
    </div>
    <Show when={error()}><p class="field-error" role="alert">{error()}</p></Show>
  </div>;
}
