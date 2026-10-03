import { onMount } from "solid-js";
export default function LoginComplete() {
  const params = new URLSearchParams(location.search);
  const state = params.get("state") || "";
  window.history.replaceState({}, "", "/login-complete");
  onMount(() => {
    if (/^[a-f0-9-]{36}$/.test(state) && window.opener) {
      window.opener.postMessage({ type: "honeycomb-login-complete", state }, location.origin);
    }
  });
  return <main class="storage-callback"><a class="wordmark" href="/"><img src="/brand/honeycomb.svg" alt=""/>honeycomb</a><section><span class="eyebrow">SIGNED IN</span><h1>You’re ready to continue.</h1><p>Your Honeycomb window will continue automatically.</p><a class="button primary" href="/">Open Honeycomb here</a></section></main>;
}
