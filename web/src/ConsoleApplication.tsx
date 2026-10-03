import { For, Show, createSignal, onCleanup, type JSX } from "solid-js";
import { ArrowLeft, ArrowUpRight, Check, ChevronDown, Copy, Package, Settings2, Terminal } from "lucide-solid";
import { safeLink, type AppRecord } from "./api";
import { StatusBadge } from "./ui/Arc";
import "./console-application.css";

export default function ConsoleApplication(props: { app: AppRecord; tab: string; canManage: boolean; libraryUrl: string; onTabChange: (tab: string) => void; onBack: () => void; onEdit: () => void; children: JSX.Element }) {
  const tabs = () => [ { value: "overview", label: "Overview" }, { value: "reviews", label: `Reviews (${props.app.reviews})` }, ...(props.canManage ? [{ value: "release", label: "Releases & publication" }, { value: "access", label: "Access & secrets" }] : []) ];
  const keys = (event: KeyboardEvent, index: number) => {
    const list = tabs();
    const next = event.key === "ArrowRight" ? (index + 1) % list.length : event.key === "ArrowLeft" ? (index - 1 + list.length) % list.length : event.key === "Home" ? 0 : event.key === "End" ? list.length - 1 : -1;
    if (next < 0) return;
    event.preventDefault(); props.onTabChange(list[next].value);
    document.getElementById(`console-tab-${list[next].value}`)?.focus();
  };
  return <section class="console-application">
    <button class="text-link app-back" onClick={props.onBack}><ArrowLeft size={15}/>All applications</button>
    <header class="console-app-header">
      <div class="console-app-identity"><div class="console-app-logo"><Show when={safeLink(props.app.config.logo_url)} fallback={<Package size={30} stroke-width={1.4}/>}>{url => <img src={url()} alt=""/>}</Show></div><div><span class="console-app-kicker">Application · {props.app.org_id}</span><h1>{props.app.name}</h1><code>{props.app.app_id}</code></div></div>
      <div class="console-app-actions"><a class="button outline" href={props.libraryUrl} target="_blank" rel="noopener noreferrer">View in library<ArrowUpRight size={15}/></a><Show when={props.canManage}><button class="button primary" onClick={props.onEdit}><Settings2 size={15}/>Edit configuration</button></Show></div>
      <div class="console-app-summary"><StatusBadge tone={props.app.visibility === "public" ? "success" : "neutral"}>{props.app.visibility === "public" ? "Public application" : "Private application"}</StatusBadge><span>Configuration {props.app.revision}</span><Show when={props.app.iam_revision === 0}><StatusBadge tone="warning">Activation pending</StatusBadge></Show><Show when={props.app.latest_version}><span>Latest recorded version <strong>{props.app.latest_version}</strong></span></Show></div>
    </header>
    <div class="console-app-tabs" role="tablist" aria-label="Application sections"><For each={tabs()}>{(tab, index) => <button type="button" role="tab" id={`console-tab-${tab.value}`} aria-controls="console-app-panel" aria-selected={props.tab === tab.value} tabIndex={props.tab === tab.value ? 0 : -1} onClick={() => props.onTabChange(tab.value)} onKeyDown={event => keys(event, index())}>{tab.label}</button>}</For></div>
    <div class={`console-app-panel tab-${props.tab}`} id="console-app-panel" role="tabpanel" aria-labelledby={`console-tab-${props.tab}`} tabIndex={0}>{props.children}</div>
  </section>;
}

function InstallCommand(props: { command: string }) {
  const [copied, setCopied] = createSignal(false), [error, setError] = createSignal("");
  let timer: ReturnType<typeof setTimeout>;
  onCleanup(() => clearTimeout(timer));
  return <><div class="install-command"><code>{props.command}</code><button class="icon-button" aria-label="Copy install command" onClick={async () => { try { await navigator.clipboard.writeText(props.command); setCopied(true); setError(""); clearTimeout(timer); timer = setTimeout(() => setCopied(false), 2000); } catch {setError("Copy is unavailable. Select and copy the command.");} }}>{copied() ? <Check size={16}/> : <Copy size={16}/>}</button></div><Show when={error()}><p role="status" class="field-error">{error()}</p></Show><span class="sr-only" role="status">{copied() ? "Command copied" : ""}</span></>;
}
export function InstallPanel(props: { appId: string; hasRelease: boolean }) {
  return <aside class="console-install-panel" aria-label="Install application"><div class="console-section-heading"><span class="section-icon"><Terminal size={19}/></span><h2>Install with the CLI</h2></div><Show when={props.hasRelease} fallback={<p class="muted">Upload your first production release to make this application installable.</p>}><p class="install-channel">Production</p><InstallCommand command={`honeycomb install '${props.appId}'`}/><p class="install-help">Installs the latest available production release and follows updates to that channel.</p></Show><details class="development-install"><summary><span><strong>Development channel</strong><small>For experimental builds</small></span><ChevronDown size={17}/></summary><div><InstallCommand command={`honeycomb install '${props.appId}>test'`}/><p class="install-help">Available when a development release exists. Honeycomb asks before switching an installed application’s channel.</p></div></details><p class="install-version-note">Need a specific version? Choose it from <strong>Releases & publication</strong>.</p></aside>;
}
