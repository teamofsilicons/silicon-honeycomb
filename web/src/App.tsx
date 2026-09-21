import Bundles from "./Bundles";
import Releases, { type ReleaseChannel } from "./Releases";
import { LogoField } from "./LogoField";
import { telemetryEnabled, setTelemetry, setTelemetrySource, setTelemetryAuthenticated, diagnostic } from "./telemetry";
import EnvironmentRetention from "./EnvironmentRetention";
import SentRequests, { publicationStatus } from "./SentRequests";
import WebhookSettings from "./WebhookSettings";
import ScopePicker from "./ScopePicker";
import {
  createSignal,
  createEffect,
  onMount,
  onCleanup,
  Show,
  For,
  type JSX,
} from "solid-js";
import {
  Search,
  Settings2,
  ArrowUpRight,
  ArrowRight,
  Plus,
  Package,
  Terminal,
  Lock,
  Globe,
  Star,
  Download,
  ChevronLeft,
  ChevronRight,
  X,
  Copy,
  Check,
  FlaskConical,
  FileText,
  LayoutGrid,
  LogOut,
  RefreshCw,
  BookOpen,
  Upload,
  Menu,
} from "lucide-solid";
import {
  request,
  endpoint,
  releaseEndpoint,
  safeLink,
  type AppRecord,
  type Session,
  type Config,
} from "./api";

function Dialog(props: {
  open: boolean;
  title: string;
  close: () => void;
  children: JSX.Element;
}) {
  let dialog!: HTMLDialogElement;
  createEffect(() => {
    if (props.open && !dialog.open) dialog.showModal();
    else if (!props.open && dialog.open) dialog.close();
  });
  return (
    <dialog
      ref={dialog}
      aria-label={props.title}
      onCancel={(e) => {
        if (e.target !== e.currentTarget) return;
        e.preventDefault();
        props.close();
      }}
    >
      <header class="dialog-head">
        <h2>{props.title}</h2>
        <button
          class="icon-button"
          aria-label="Close dialog"
          onClick={props.close}
        >
          <X size={20} />
        </button>
      </header>
      <div class="dialog-body">{props.children}</div>
    </dialog>
  );
}
function Code(props: { text: string }) {
  const [copied, setCopied] = createSignal(false);
  let timer: ReturnType<typeof setTimeout>;
  onCleanup(() => clearTimeout(timer));
  return (
    <div class="code-line">
      <code>{props.text}</code>
      <button
        aria-label="Copy command"
        onClick={async () => {
          await navigator.clipboard.writeText(props.text);
          setCopied(true);
          timer = setTimeout(() => setCopied(false), 2000);
        }}
      >
        {copied() ? <Check size={16} /> : <Copy size={16} />}
      </button>
    </div>
  );
}
function Badge(props: { children: JSX.Element; tone?: string }) {
  return <span class={`badge ${props.tone || ""}`}>{props.children}</span>;
}
function Empty(props: { title: string; text: string; children?: JSX.Element }) {
  return (
    <div class="empty">
      <Package size={28} stroke-width={1.2} />
      <h2>{props.title}</h2>
      <p>{props.text}</p>
      {props.children}
    </div>
  );
}
const blank = () => ({
  org_id: "",
  local_app_id: "",
  name: "",
  description: "",
  visibility: "public",
  webhook_url: "",
  webhook_secret: "",
  webhook_scope: ["membership"],
  base_url: "",
  website_url: "",
  docs_url: "",
  logo_url: "",
  testing_idle_days: 30,
  app_scope: {
    iam: ["self.identity.read", "self.profile.read", "self.membership.read"],
    external: [],
  },
  obo_endpoints: [],
});

export default function App() {
  const [config, setConfig] = createSignal<Config>({
    site: "library",
    libraryOrigin: "/",
    consoleOrigin: "https://console.honeycomb.teamofsilicons.com",
  });
  const [session, setSession] = createSignal<Session>({ authenticated: false });
  const [sessionReady, setSessionReady] = createSignal(false);
  const [view, setView] = createSignal("applications");
  const [query, setQuery] = createSignal("");
  const [page, setPage] = createSignal(1);
  const [apps, setApps] = createSignal<AppRecord[]>([]);
  const [total, setTotal] = createSignal(0);
  const [busy, setBusy] = createSignal(false);
  const [error, setError] = createSignal("");
  const [notice, setNotice] = createSignal("");
  const [showSettings, setShowSettings] = createSignal(false);
  const [telemetry, setTelemetryPreference] = createSignal(telemetryEnabled());
  const [selected, setSelected] = createSignal<AppRecord>();
  const [detailsTab, setDetailsTab] = createSignal("overview");
  const [reviewInbox, setReviewInbox] = createSignal<any[]>([]);
  const [sentRevision, setSentRevision] = createSignal(0);
  const [selectedReview, setSelectedReview] = createSignal<any>();
  const [reviewDecision, setReviewDecision] = createSignal("approve");
  const [reviewReason, setReviewReason] = createSignal("");
  const [reviews, setReviews] = createSignal<any[]>([]);
  const [reconciliation, setReconciliation] = createSignal<any>();
  const [appOperations, setAppOperations] = createSignal<any[]>([]);
  const [rotationConfirmation, setRotationConfirmation] = createSignal("");
  const [rotationProof, setRotationProof] = createSignal("");
  const [publication, setPublication] = createSignal<any>();
  const [showCreate, setShowCreate] = createSignal(false);
  const [logoUploading, setLogoUploading] = createSignal(false);
  const [firstRelease, setFirstRelease] = createSignal<File>();
  const [pendingRelease, setPendingRelease] = createSignal<{ appId: string; file: File; key: string; channel: ReleaseChannel }>();
  const [uploadChannel, setUploadChannel] = createSignal<ReleaseChannel | "">("");
  const [releaseRevision, setReleaseRevision] = createSignal(0);
  const [form, setForm] = createSignal<Record<string, any>>(blank());
  const editing = () => form().draft_edit_app_id as string | undefined;
  const [advanced, setAdvanced] = createSignal(false);
  const [scopeText, setScopeText] = createSignal(
    JSON.stringify(blank().app_scope, null, 2),
  );
  const [oboText, setOboText] = createSignal("[]");
  const [draftId, setDraftId] = createSignal("");
  const [draftRevision, setDraftRevision] = createSignal(0);
  const [draftStatus, setDraftStatus] = createSignal("");
  const [drafts, setDrafts] = createSignal<any[]>([]);
  const [secret, setSecret] = createSignal("");
  const [envs, setEnvs] = createSignal<any[]>([]);
  const [selectedEnv, setSelectedEnv] = createSignal<any>();
  const [environmentAction, setEnvironmentAction] = createSignal("");
  const [confirmationName, setConfirmationName] = createSignal("");
  const [showEnv, setShowEnv] = createSignal(false);
  const [importApp, setImportApp] = createSignal("");
  const [importRelease, setImportRelease] = createSignal("");
  const [refreshImport, setRefreshImport] = createSignal(false);
  const [envName, setEnvName] = createSignal("");
  const [envOrg, setEnvOrg] = createSignal("");
  const [envKey, setEnvKey] = createSignal("");
  const [mobileNav, setMobileNav] = createSignal(false);
  let searchTimer: ReturnType<typeof setTimeout>;
  let saveTimer: ReturnType<typeof setTimeout>;
  let saveGeneration = 0;
  let saving: Promise<void> = Promise.resolve();
  let fetchRevision = 0;
  const isConsole = () => config().site === "console";
  createEffect(() => {
    setTelemetrySource(config().site);
    setTelemetryAuthenticated(session().authenticated);
    if (sessionReady()) diagnostic("page_view", selected() ? "application" : isConsole() ? view() : "catalog", 0, true);
  });
  const memberships = () =>
    Object.entries(session().identity?.organizations || {});
  const admins = () =>
    memberships().filter(
      ([, role]) => role === "org_owner" || role === "org_admin",
    );
  const canManage = (app: AppRecord) =>
    admins().some(([org]) => org === app.org_id);
  async function load() {
    const rev = ++fetchRevision;
    setBusy(true);
    setError("");
    try {
      const result = await request<{ items: AppRecord[]; total: number }>(
        `/api/v1/apps?q=${encodeURIComponent(query())}&page=${page()}&managed=${isConsole()}`,
      );
      if (rev === fetchRevision) {
        setApps(result.items);
        setTotal(result.total);
      }
    } catch (e) {
      if (rev === fetchRevision) setError(String((e as Error).message));
    } finally {
      if (rev === fetchRevision) setBusy(false);
    }
  }
  async function loadView() {
    setError("");
    try {
      if (view() === "applications") await load();
      if (view() === "drafts") {
        const result = await Promise.all(
          admins().map(async ([org]) => {
            const page = await request(
              `/api/v1/organizations/${encodeURIComponent(org)}/drafts`,
            );
            return page.items.map((d: any) => ({ ...d, org_id: org }));
          }),
        );
        setDrafts(result.flat());
      }
      if (view() === "review-requests") setReviewInbox((await request("/api/v1/review-requests")).items);
      if (view() === "environments") {
        setEnvs((await request("/api/v1/environments")).items);
      }
    } catch (e) {
      setError((e as Error).message);
    }
  }
  onMount(async () => {
    try {
      setConfig(await request<Config>("/api/config"));
      document.title = isConsole()
        ? "Honeycomb Console"
        : "Honeycomb — Application library";
      setSession(await request<Session>("/api/session"));
    } catch (e) {
      setError((e as Error).message);
    } finally {
      setSessionReady(true);
    }
    if (!isConsole() || session().authenticated) await load();
  });
  onCleanup(() => {
    clearTimeout(searchTimer);
    clearTimeout(saveTimer);
    saveGeneration++;
  });
  async function act(action: () => Promise<unknown>, success?: string) {
    setBusy(true);
    setError("");
    try {
      const result = await action();
      if (success) setNotice(success);
      return result;
    } catch (e) {
      setError((e as Error).message);
    } finally {
      setBusy(false);
    }
  }
  function navigate(next: string) {
    setView(next);
    setMobileNav(false);
    setTimeout(() => void loadView(), 0);
  }
  async function select(app: AppRecord) {
    setSelected(app);
    setUploadChannel("");
    setDetailsTab("overview");
    setReviews([]);
    setPublication(undefined);
    setAppOperations([]);
    setReconciliation(undefined);
    setRotationConfirmation("");
    await act(async () => {
      setSelected(await request<AppRecord>(endpoint(app.app_id)));
      setReviews((await request(endpoint(app.app_id) + "/reviews")).items);
      if (canManage(app)) {
        setPublication(await request(endpoint(app.app_id) + "/publication"));
        setAppOperations((await request(endpoint(app.app_id) + "/operations")).items);
        setReconciliation(await request(endpoint(app.app_id) + "/reconciliation"));
      }
    });
  }
  const reviewEndpoint=(r:any)=>`/api/v1/review-requests/${encodeURIComponent(r.id)}/${encodeURIComponent(r.provider)}`;
  async function openReview(item:any) {
    await act(async()=>{
      const detail=await request(reviewEndpoint(item));
      setSelectedReview(detail);
      setReviewDecision(detail.decision?.decision || "approve");
      setReviewReason(detail.decision?.reason || "");
    });
  }
  function beginCreate(draft?: any) {
    setError("");
    setFirstRelease(undefined);
    saveGeneration++;
    setDraftId(draft?.id || crypto.randomUUID());
    setDraftRevision(draft?.revision || 0);
    const value = draft?.body
      ? { ...blank(), ...draft.body, org_id: draft.org_id }
      : { ...blank(), org_id: admins()[0]?.[0] || "" };
    setForm(value);
    setScopeText(
      value.draft_scope_text ?? JSON.stringify(value.app_scope, null, 2),
    );
    setOboText(
      value.draft_obo_text ?? JSON.stringify(value.obo_endpoints, null, 2),
    );
    setDraftStatus(draft ? "Saved draft" : "");
    setShowCreate(true);
  }
  function beginEdit(app: AppRecord) {
    beginCreate({
      org_id: app.org_id,
      body: {
        ...app.config,
        visibility: app.config.visibility ?? app.visibility,
        draft_edit_app_id: app.app_id,
        draft_edit_revision: app.revision,
      },
    });
    setSelected(undefined);
  }
  function edit(key: string, value: any) {
    setForm({ ...form(), [key]: value });
    scheduleSave();
  }
  function scheduleSave() {
    clearTimeout(saveTimer);
    saveTimer = setTimeout(() => {
      void saveDraft().catch(() => {});
    }, 650);
  }
  function saveDraft() {
    clearTimeout(saveTimer);
    const generation = saveGeneration;
    const id = draftId();
    const body: Record<string, any> = {
      ...form(),
      draft_scope_text: scopeText(),
      draft_obo_text: oboText(),
    };
    delete body.webhook_secret;
    if (!body.org_id) return Promise.resolve();
    setDraftStatus("Saving…");
    saving = saving.then(async () => {
      if (generation !== saveGeneration) return;
      const result = await request(
        `/api/v1/organizations/${encodeURIComponent(body.org_id)}/drafts/${id}`,
        {
          method: "PUT",
          headers: { "If-Match": String(draftRevision()) },
          body: JSON.stringify(body),
        },
      );
      if (generation === saveGeneration) {
        setDraftRevision(result.revision);
        setDraftStatus("Draft saved");
      }
    });
    const result = saving;
    saving = saving.catch((e) => {
      if (generation === saveGeneration) setDraftStatus((e as Error).message);
    });
    return result;
  }
  async function closeCreate() {
    await act(async () => {
      await saveDraft();
      setShowCreate(false);
      saveGeneration++;
      if (view() === "drafts") await load();
    });
  }
  async function createApp(e: Event) {
    e.preventDefault();
    if (busy() || logoUploading()) return;
    await act(async () => {
      const editingId = editing();
      const editingRevision = form().draft_edit_revision;
      const payload: Record<string, any> = {
        ...form(),
        app_scope: JSON.parse(scopeText()),
        obo_endpoints: JSON.parse(oboText()),
      };
      const archive = editingId ? undefined : firstRelease();
      const channel = form().draft_release_channel as ReleaseChannel | undefined;
      const appId = `${payload.org_id}>${payload.local_app_id}`;
      if (archive) {
        if (channel !== "prod" && channel !== "dev") throw new Error("Choose production or development for the first CLI release.");
        if (archive.size > 512 * 1024 * 1024) throw new Error("CLI archives must be no larger than 512 MiB.");
        const validation = await request("/api/v1/packages/validate", {
          method: "POST", headers: { "Content-Type": "application/gzip" }, body: archive,
        });
        if (!validation.valid) throw new Error(validation.errors.join("\n"));
        if (!/^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$/.test(validation.manifest?.version || ""))
          throw new Error("CLI release version must use x.x.x format without prerelease or build suffixes. Update honeycomb.yaml and pack the archive again.");
        if (validation.manifest?.app_id != null && validation.manifest.app_id !== appId)
          throw new Error(`The archive app_id must match ${appId}. Correct or omit app_id in honeycomb.yaml, then run honeycomb pack again.`);
      }
      for (const field of Object.keys(payload))
        if (field.startsWith("draft_")) delete payload[field];
      for (const key of ["base_url", "website_url", "docs_url", "logo_url"])
        if (!payload[key]) payload[key] = null;
      const result = await request(
        editingId ? endpoint(editingId) : "/api/v1/apps",
        {
          method: editingId ? "PUT" : "POST",
          headers: editingId
            ? { "If-Match": String(editingRevision) }
            : {},
          body: JSON.stringify(payload),
        },
      );
      setShowCreate(false);
      saveGeneration++;
      clearTimeout(saveTimer);
      try {
        setNotice(
          result.state === "accepted"
            ? editingId
              ? "Application configuration updated."
              : "Application created. Upload its first CLI release to continue."
            : "Application saved. IAM configuration is pending.",
        );
        await load();
        if (archive) {
          setPendingRelease({ appId, file: archive, key: crypto.randomUUID(), channel: channel! });
          setUploadChannel(channel!);
          setSelected(await request(endpoint(appId)));
          setDetailsTab("release");
          setPublication(undefined);
          setReviews([]);
          setAppOperations([]);
          setReconciliation(undefined);
          setFirstRelease(undefined);
          setNotice("Application saved. Uploading its first CLI release…");
          try {
            await sendRelease();
            setNotice("Application saved and first CLI release uploaded.");
          } catch (e) {
            setNotice("Application saved. Its first CLI release still needs to be uploaded.");
            throw new Error(`The application was saved, but the release upload failed. Retry the upload below. ${(e as Error).message}`);
          }
        }
      } finally {
        if (result.app_secret) setSecret(result.app_secret);
      }
    });
  }
  async function sendRelease() {
    const pending = pendingRelease();
    const app = selected();
    if (!pending || !app || pending.appId !== app.app_id) return;
    const result = await request(releaseEndpoint(app.app_id) + `/releases?channel=${pending.channel}`, {
      method: "POST",
      headers: {
        "Content-Type": "application/gzip",
        "If-Match": String(app.revision),
        "Idempotency-Key": pending.key,
      },
      body: pending.file,
    });
    setPendingRelease(undefined);
    setSelected(await request(endpoint(app.app_id)));
    setPublication(await request(endpoint(app.app_id) + "/publication"));
    setReleaseRevision(value => value + 1);
    await load();
    if (result.publication?.state === "request_pending") {
      throw new Error(`The CLI release was uploaded, but its public approval request needs attention. ${result.publication.error} Retry the request below.`);
    }
  }
  async function upload(file: File | undefined) {
    if (!file || !selected() || busy()) return;
    const channel = uploadChannel();
    if (!channel) { setError("Choose production or development before uploading a CLI release."); return; }
    setPendingRelease({ appId: selected()!.app_id, file, key: crypto.randomUUID(), channel });
    await act(async () => {
      await sendRelease();
    }, "CLI release uploaded.");
  }
  async function runEnvironmentAction(action: string) {
    const env = selectedEnv();
    if (!env) return;
    await act(async () => {
      const result = await request(
        `/api/v1/environments/${env.environment_id}/actions/${action}`,
        {
          method: "POST",
          headers: { "If-Match": String(env.revision) },
        },
      );
      setSelectedEnv(
        await request(`/api/v1/environments/${env.environment_id}`),
      );
      setEnvironmentAction("");
      setConfirmationName("");
      if (result.testing_key) setEnvKey(result.testing_key);
      await load();
    });
  }
  async function logout() {
    await act(async () => {
      await request("/auth/logout", { method: "POST" });
      setSession({ authenticated: false });
      setSelected(undefined);
      setApps([]);
      if (!isConsole()) await load();
    });
  }
  return (
    <div class="app-shell">
      <aside
        id="honeycomb-navigation"
        class={`sidebar ${mobileNav() ? "open" : ""}`}
      >
        <a class="wordmark" href="/">
          <img src="/brand/honeycomb.svg" alt="" />
          <span>honeycomb</span>
        </a>
        <div class="workspace-label">
          <span class="eyebrow">TEAM OF SILICONS</span>
          <span>
            {isConsole() ? "Developer console" : "Application library"}
          </span>
        </div>
        <nav aria-label="Main navigation">
          <button
            class={view() === "applications" ? "active" : ""}
            onClick={() => navigate("applications")}
          >
            <LayoutGrid size={18} />
            {isConsole() ? "Applications" : "Browse applications"}
          </button>
          <Show when={isConsole()}>
            <button class={view() === "bundles" ? "active" : ""} onClick={() => navigate("bundles")}><Package size={18} />Bundles</button>
            <button
              class={view() === "drafts" ? "active" : ""}
              onClick={() => navigate("drafts")}
            >
              <FileText size={18} />
              Drafts
            </button>
            <button
              class={view() === "environments" ? "active" : ""}
              onClick={() => navigate("environments")}
            >
              <FlaskConical size={18} />
              Testing environments
            </button>
            <button class={view() === "review-requests" ? "active" : ""} onClick={()=>navigate("review-requests")}><Check size={18}/>Review requests</button>
            <button class={view() === "sent-requests" ? "active" : ""} onClick={()=>navigate("sent-requests")}><ArrowUpRight size={18}/>Sent requests</button>
          </Show>
          <button
            class={view() === "docs" ? "active" : ""}
            onClick={() => navigate("docs")}
          >
            <BookOpen size={18} />
            Get started
          </button>
        </nav>
        <div class="sidebar-bottom">
          <a href="https://docs.honeycomb.teamofsilicons.com">
            Documentation <ArrowUpRight size={15} />
          </a>
          <a
            href={isConsole() ? config().libraryOrigin : config().consoleOrigin}
          >
            {isConsole()
              ? "Open application library"
              : "Open developer console"}
            <ArrowUpRight size={15} />
          </a>
          <div class="sidebar-footnote">
            <span class="mini-mark">H</span>
            <span>A team of silicons space</span>
          </div>
        </div>
      </aside>
      <div class="workspace">
        <header class="topbar">
          <div class="breadcrumb">
            <button
              class="icon-button mobile-toggle"
              aria-label="Open navigation"
              aria-expanded={mobileNav()}
              aria-controls="honeycomb-navigation"
              onClick={() => setMobileNav(!mobileNav())}
            >
              <Menu size={20} />
            </button>
            <span>Honeycomb</span>
            <span class="slash">/</span>
            <span>{isConsole() ? "Console" : "Library"}</span>
          </div>
          <div class="topbar-actions">
            <Show when={!isConsole()}>
              <a class="button primary" href={config().consoleOrigin}>
                Create an app <ArrowUpRight size={16} />
              </a>
            </Show>
            <button class="icon-button" aria-label="Telemetry settings" onClick={() => setShowSettings(true)}><Settings2 size={18} /></button>
            <Show
              when={session().authenticated}
              fallback={
                <a class="button subtle" href="/auth/login">
                  Sign in with IAM
                  <ArrowRight size={16} />
                </a>
              }
            >
              <span class="account">
                {session().identity?.actor_type === "silicon"
                  ? "Silicon"
                  : "Carbon"}{" "}
                account
              </span>
              <button
                class="icon-button"
                aria-label="Sign out"
                onClick={logout}
              >
                <LogOut size={17} />
              </button>
            </Show>
          </div>
        </header>
        <main>
          <Show when={error()}>
            <div class="banner error" role="alert">
              <span>{error()}</span>
              <button aria-label="Dismiss error" onClick={() => setError("")}>
                <X size={16} />
              </button>
            </div>
          </Show>
          <Show when={notice()}>
            <div class="banner notice" role="status">
              <span>{notice()}</span>
              <button
                aria-label="Dismiss notification"
                onClick={() => setNotice("")}
              >
                <X size={16} />
              </button>
            </div>
          </Show>
          <Show
            when={sessionReady()}
            fallback={
              <div class="loading" role="status">
                Opening Honeycomb…
              </div>
            }
          >
            <Show
              when={
                !isConsole() || session().authenticated || view() === "docs"
              }
              fallback={
                <section class="sign-in">
                  <span class="eyebrow">HONEYCOMB CONSOLE</span>
                  <h1>
                    Your next application.
                    <br />
                    <em>At home in Honeycomb.</em>
                  </h1>
                  <p>
                    Bring your tools to the Silicon ecosystem. Manage releases,
                    configure access, and publish when you’re ready.
                  </p>
                  <a href="/auth/login" class="button primary">
                    Continue with IAM
                    <ArrowRight size={17} />
                  </a>
                  <small>Sign in as a Carbon or Silicon to get started.</small>
                  <div class="signin-note">
                    <Lock size={19} />
                    <span>
                      Public approval is requested automatically once your
                      application and first production release are ready, unless you choose to keep it private.
                    </span>
                  </div>
                </section>
              }
            >
              <Show when={!admins().length && isConsole()}>
                <div class="banner notice" id="application-access-help" role="status">
                  <span>
                    {memberships().some(([, role]) => role == null) || !memberships().length
                      ? "IAM hasn’t provided an organization owner or admin role. Honeycomb needs membership access from IAM before you can create applications. After that access is enabled, sign in again and approve it."
                      : "Creating applications requires an organization owner or admin role. Ask an organization owner to update your access, then sign in again."}
                  </span>
                  <a class="text-link" href="/auth/login">Sign in again <ArrowRight size={16} /></a>
                </div>
              </Show>
              <Show when={view() === "review-requests"}>
                <section class="page-heading"><div><span class="eyebrow">APPLICATION ACCESS</span><h1>Review requests.</h1><p>Review applications requesting your critical scopes. Honeycomb validation follows accepted provider decisions.</p></div></section>
                <Show when={!reviewInbox().length}><p class="muted">There are no requests you can currently review. Provider reviews require current owner/admin access; Honeycomb validation requires explicit reviewer permission.</p></Show>
                <For each={reviewInbox()}>{(item)=><button class="review-request-row" onClick={()=>void openReview(item)}><div><h2>{item.configuration?.name || item.app_id}</h2><p class="app-id">{item.app_id}</p><p>{item.provider} · {item.scopes.join(", ") || "Publication validation"}</p></div><Badge>{item.state}</Badge><ArrowUpRight size={17}/></button>}</For>
              </Show>
              <Show when={view() === "sent-requests" && isConsole()}>
                <SentRequests revision={sentRevision()} open={app => { void select(app); setDetailsTab("release"); }} discuss={item => void openReview(item)} />
              </Show>
              <Show when={view() === "bundles" && isConsole()}><Bundles organizations={admins().map(([org]) => org)} /></Show>
              <Show when={view() === "applications"}>
                <section class="page-heading">
                  <div>
                    <span class="eyebrow">
                      {isConsole() ? "YOUR WORKSPACE" : "THE SILICON ECOSYSTEM"}
                    </span>
                    <h1>
                      {isConsole()
                        ? "Your applications."
                        : "Tools for what comes next."}
                    </h1>
                    <p>
                      {isConsole()
                        ? "Build, release, and care for the tools your team uses."
                        : "Discover applications for you, your team, and your silicons."}
                    </p>
                  </div>
                  <Show when={isConsole()}>
                    <button
                      class="button primary"
                      disabled={!admins().length}
                      aria-describedby={!admins().length ? "application-access-help" : undefined}
                      onClick={() => beginCreate()}
                    >
                      <Plus size={17} />
                      Create application
                    </button>
                  </Show>
                </section>
                <div class="catalog-toolbar">
                  <label class="search-field">
                    <Search size={19} />
                    <input
                      type="search"
                      aria-label="Search applications"
                      placeholder="Search by name, application ID, or description…"
                      value={query()}
                      onInput={(e) => {
                        setQuery(e.currentTarget.value);
                        setPage(1);
                        clearTimeout(searchTimer);
                        searchTimer = setTimeout(() => void load(), 250);
                      }}
                    />
                    <kbd>/</kbd>
                  </label>
                  <button
                    class="icon-button"
                    aria-label="Refresh applications"
                    disabled={busy()}
                    onClick={load}
                  >
                    <RefreshCw size={17} class={busy() ? "spinning" : ""} />
                  </button>
                </div>
                <div class="list-caption">
                  <span>
                    {query()
                      ? "SEARCH RESULTS"
                      : isConsole()
                        ? "MANAGED APPLICATIONS"
                        : "APPLICATION LIBRARY"}
                  </span>
                  <span>
                    {total()} {total() === 1 ? "application" : "applications"}
                  </span>
                </div>
                <Show
                  when={!busy() || apps().length}
                  fallback={
                    <div class="loading" role="status">
                      Finding applications…
                    </div>
                  }
                >
                  <Show
                    when={apps().length}
                    fallback={
                      <Empty
                        title={
                          query()
                            ? "No matching applications."
                            : isConsole()
                              ? "Your first application starts here."
                              : "A little space for what’s next."
                        }
                        text={
                          query()
                            ? "Try another name or a shorter search."
                            : isConsole()
                              ? admins().length
                                ? "Create an application or pick up a saved draft."
                                : "An organization owner or admin can create and manage applications."
                              : "Published applications will appear here. Sign in to see applications private to your organization."
                        }
                      >
                        <Show when={isConsole() && admins().length}>
                          <button
                            class="button outline"
                            onClick={() => beginCreate()}
                          >
                            <Plus size={16} />
                            Create application
                          </button>
                        </Show>
                      </Empty>
                    }
                  >
                    <div class="package-list">
                      <For each={apps()}>
                        {(app) => (
                          <button
                            class="package-row"
                            onClick={() => void select(app)}
                          >
                            <div class="package-icon">
                              <Show
                                when={safeLink(app.config.logo_url)}
                                fallback={
                                  <Package size={25} stroke-width={1.3} />
                                }
                              >
                                <img
                                  src={safeLink(app.config.logo_url)}
                                  alt=""
                                  referrerpolicy="no-referrer"
                                  loading="lazy"
                                />
                              </Show>
                            </div>
                            <div class="package-info">
                              <div class="package-title">
                                <h2>{app.name}</h2>
                                <Badge
                                  tone={
                                    app.visibility === "private"
                                      ? "private"
                                      : ""
                                  }
                                >
                                  {app.visibility === "private" ? (
                                    <Lock size={11} />
                                  ) : (
                                    <Globe size={11} />
                                  )}{" "}
                                  {app.visibility}
                                </Badge>
                              </div>
                              <span class="app-id">{app.app_id}</span>
                              <p>{app.description}</p>
                            </div>
                            <div class="package-meta">
                              <span>
                                <Star size={14} />
                                {app.reviews ? app.rating.toFixed(1) : "—"}
                              </span>
                              <span class="mono">
                                {app.latest_version
                                  ? `v${app.latest_version}`
                                  : "No production releases"}
                              </span>
                              <Show when={app.state !== "active"}>
                                <Badge>Pending IAM</Badge>
                              </Show>
                            </div>
                            <ArrowUpRight class="row-arrow" size={18} />
                          </button>
                        )}
                      </For>
                    </div>
                    <div class="pagination">
                      <span>
                        Page {page()} of {Math.max(1, Math.ceil(total() / 20))}
                      </span>
                      <div>
                        <button
                          aria-label="Previous page"
                          disabled={page() === 1}
                          onClick={() => {
                            setPage(page() - 1);
                            void load();
                          }}
                        >
                          <ChevronLeft size={17} />
                        </button>
                        <button
                          aria-label="Next page"
                          disabled={page() * 20 >= total()}
                          onClick={() => {
                            setPage(page() + 1);
                            void load();
                          }}
                        >
                          <ChevronRight size={17} />
                        </button>
                      </div>
                    </div>
                  </Show>
                </Show>
                <Show when={!isConsole()}>
                  <div class="cli-callout">
                    <Terminal size={23} stroke-width={1.4} />
                    <div>
                      <h3>Make yourself at home in the terminal.</h3>
                      <p>
                        Search, install, and update applications with the
                        Honeycomb CLI.
                      </p>
                    </div>
                    <button class="text-link" onClick={() => navigate("docs")}>
                      Get the CLI
                      <ArrowRight size={16} />
                    </button>
                  </div>
                </Show>
              </Show>
              <Show when={view() === "drafts"}>
                <section class="page-heading">
                  <div>
                    <span class="eyebrow">PICK UP WHERE YOU LEFT OFF</span>
                    <h1>Work in progress.</h1>
                    <p>
                      Shared drafts for your organization’s next applications.
                    </p>
                  </div>
                  <button
                    class="button primary"
                    disabled={!admins().length}
                    onClick={() => beginCreate()}
                  >
                    <Plus size={16} />
                    New draft
                  </button>
                </section>
                <Show
                  when={drafts().length}
                  fallback={
                    <Empty
                      title="No drafts yet."
                      text="Your application details are saved as you work, so another admin can continue."
                    />
                  }
                >
                  <div class="package-list">
                    <For each={drafts()}>
                      {(d) => (
                        <button
                          class="package-row"
                          onClick={() => beginCreate(d)}
                        >
                          <FileText size={22} />
                          <div class="package-info">
                            <h2>{d.body.name || "Untitled application"}</h2>
                            <span class="app-id">
                              {d.org_id} · Revision {d.revision}
                            </span>
                          </div>
                          <ArrowUpRight size={17} />
                        </button>
                      )}
                    </For>
                  </div>
                </Show>
              </Show>
              <Show when={view() === "environments"}>
                <section class="page-heading">
                  <div>
                    <span class="eyebrow">ROOM TO EXPERIMENT</span>
                    <h1>Testing environments.</h1>
                    <p>
                      Keep test identities, releases, and application data in
                      their own space.
                    </p>
                  </div>
                  <button
                    class="button primary"
                    onClick={() => {
                      setEnvOrg(memberships()[0]?.[0] || "");
                      setShowEnv(true);
                    }}
                    disabled={!memberships().length}
                  >
                    <Plus size={17} />
                    Create environment
                  </button>
                </section>
                <Show
                  when={envs().length}
                  fallback={
                    <Empty
                      title="A fresh start for every experiment."
                      text="Create an isolated environment for your organization."
                    />
                  }
                >
                  <div class="environment-list">
                    <For each={envs()}>
                      {(env) => (
                        <article class="environment-row">
                          <FlaskConical size={23} />
                          <div>
                            <h2>{env.name}</h2>
                            <span class="app-id">
                              {env.org_id} · {env.environment_id}
                            </span>
                            <p>
                              Generation {env.generation} · Revision{" "}
                              {env.revision}
                            </p>
                          </div>
                          <Badge>{env.state}</Badge>
                          <Show when={env.can_manage}>
                            <button
                              class="button outline"
                              onClick={() =>
                                void act(async () => {
                                  setSelectedEnv(
                                    await request(
                                      `/api/v1/environments/${env.environment_id}`,
                                    ),
                                  );
                                  setEnvironmentAction("");
                                  setConfirmationName("");
                                })
                              }
                            >
                              Manage
                            </button>

                            <button
                              class="button outline"
                              onClick={() =>
                                void act(async () =>
                                  setEnvKey(
                                    (
                                      await request(
                                        `/api/v1/environments/${env.environment_id}/key`,
                                        { method: "POST" },
                                      )
                                    ).testing_key,
                                  ),
                                )
                              }
                            >
                              View key
                            </button>
                          </Show>
                        </article>
                      )}
                    </For>
                  </div>
                </Show>
              </Show>
              <Show when={view() === "docs"}>
                <section class="page-heading">
                  <div>
                    <span class="eyebrow">A GOOD PLACE TO START</span>
                    <h1>Honeycomb, from your terminal.</h1>
                    <p>
                      One CLI for discovering, installing, and publishing
                      applications.
                    </p>
                  </div>
                </section>
                <div class="docs-grid">
                  <section class="doc-section">
                    <span class="step-number">01</span>
                    <h2>Install Honeycomb</h2>
                    <p>Use the shell installer on macOS or Linux.</p>
                    <Code
                      text={
                        "printf \"Starting Honeycomb installer…\\n\"; /bin/bash -c \"$(curl -fL --progress-bar --connect-timeout 20 --max-time 120 https://raw.githubusercontent.com/teamofsilicons/silicon-honeycomb/main/install.sh)\""
                      }
                    />
                    <p>Or install with Cargo.</p>
                    <Code text="cargo install silicon-honeycomb-cli --locked" />
                    <p class="muted">
                      These distribution commands become available with
                      Honeycomb’s first published release.
                    </p>
                  </section>
                  <section class="doc-section">
                    <span class="step-number">02</span>
                    <h2>Find your next tool</h2>
                    <Code text="honeycomb search briefcase" />
                    <Code text="honeycomb install 'tos>briefcase'" />
                    <p>
                      Sign in with a short-lived token from IAM to access your
                      organization’s private applications.
                    </p>
                    <Code text="honeycomb login <slt>" />
                    <Code text="honeycomb login status --json" />
                  </section>
                  <section class="doc-section">
                    <span class="step-number">03</span>
                    <h2>Publish an application</h2>
                    <p>
                      Define your commands and all six required targets in
                      honeycomb.yaml. Validate before packaging.
                    </p>
                    <Code text="honeycomb validate" />
                    <Code text="honeycomb pack" />
                    <Code text="honeycomb apps create application.json" />
                    <p>
                      Upload a release to request public approval automatically,
                      or choose to keep your application private.
                    </p>
                    <Code text="honeycomb --help" />
                  </section>
                </div>
              </Show>
            </Show>
          </Show>
        </main>
        <footer>
          <span>Built for Carbons & Silicons.</span>
          <span>HONEYCOMB / TEAM OF SILICONS</span>
        </footer>
      </div>
      <Dialog
        open={!!selected()}
        title={selected()?.name || "Application"}
        close={() => { setSelected(undefined); setSentRevision(value => value + 1); }}
      >
        <Show when={selected()}>
          {(app) => (
            <>
              <div class="detail-id">
                <span class="app-id">{app().app_id}</span>
                <Badge>{app().visibility}</Badge>
              </div>
              <div class="tabs">
                <button
                  class={detailsTab() === "overview" ? "active" : ""}
                  onClick={() => setDetailsTab("overview")}
                >
                  Overview
                </button>
                <button
                  class={detailsTab() === "reviews" ? "active" : ""}
                  onClick={() => setDetailsTab("reviews")}
                >
                  Reviews ({app().reviews})
                </button>
                <Show when={isConsole() && canManage(app())}>
                  <button
                    class={detailsTab() === "release" ? "active" : ""}
                    onClick={() => setDetailsTab("release")}
                  >
                    Releases & publication
                  </button>
                  <button class={detailsTab() === "access" ? "active" : ""} onClick={() => setDetailsTab("access")}>Access & secrets</button>
                </Show>
              </div>
              <Show when={error()}>
                <p class="field-error" role="alert">
                  {error()}
                </p>
              </Show>
              <Show when={detailsTab() === "overview"}>
                <div class="detail-stats">
                  <span>
                    <Star size={16} />
                    {app().reviews ? app().rating.toFixed(1) : "Unrated"}
                  </span>
                  <span>
                    <Download size={16} />
                    {app().installs} downloads
                  </span>
                  <span class="mono">
                    {app().latest_version || "No production releases"}
                  </span>
                </div>
                <p class="description">{app().description}</p>
                <Show when={config().site === "console" && canManage(app())}>
                  <button
                    class="button outline"
                    onClick={() => beginEdit(app())}
                  >
                    Edit configuration
                  </button>
                </Show>
                <Show when={app().latest_version}>
                  <h3 class="section-label">INSTALL FROM YOUR TERMINAL</h3>
                  <Code text={`honeycomb install '${app().app_id}'`} />
                  <p class="muted">Installs the latest official production release and follows production updates.</p>
                </Show>
                <details class="experimental-install">
                  <summary>Experimental development releases</summary>
                  <p class="muted">If this application offers development releases, use this command to follow experimental updates. Honeycomb asks before switching an installed application between channels.</p>
                  <Code text={`honeycomb install '${app().app_id}>test'`} />
                  <p class="muted">For an exact version, append @x.x.x to either application identifier.</p>
                </details>
                <div class="detail-links">
                  <For each={["website_url", "docs_url", "base_url"]}>
                    {(key) => (
                      <Show when={safeLink(app().config[key])}>
                        <a
                          href={safeLink(app().config[key])}
                          target="_blank"
                          rel="noopener noreferrer"
                        >
                          {key === "website_url"
                            ? "Website"
                            : key === "docs_url"
                              ? "Documentation"
                              : "Backend"}
                          <ArrowUpRight size={14} />
                        </a>
                      </Show>
                    )}
                  </For>
                </div>
                <Show when={session().authenticated}>
                  <button
                    class="button outline"
                    disabled={busy()}
                    onClick={() =>
                      void act(async () => {
                        await request(endpoint(app().app_id) + "/star", {
                          method: "PUT",
                          body: "{}",
                        });
                        setSelected(await request(endpoint(app().app_id)));
                      }, "Application starred.")
                    }
                  >
                    <Star size={16} />
                    Star application · {app().stars}
                  </button>
                </Show>
              </Show>
              <Show when={detailsTab() === "reviews"}>
                <For each={reviews()}>
                  {(review) => (
                    <article class="review">
                      <span>
                        <Star size={14} />
                        {review.rating.toFixed(1)}
                      </span>
                      <p>{review.review}</p>
                    </article>
                  )}
                </For>
                <Show when={!reviews().length}>
                  <p class="muted">
                    No reviews yet. Be the first to share your experience.
                  </p>
                </Show>
                <Show
                  when={session().authenticated}
                  fallback={
                    <a class="button outline" href="/auth/login">
                      Sign in to leave a review
                    </a>
                  }
                >
                  <form
                    class="review-form"
                    onSubmit={(e) => {
                      e.preventDefault();
                      const data = new FormData(e.currentTarget);
                      void act(async () => {
                        await request(endpoint(app().app_id) + "/reviews", {
                          method: "PUT",
                          body: JSON.stringify({
                            rating: Number(data.get("rating")),
                            review: data.get("review"),
                          }),
                        });
                        setReviews(
                          (await request(endpoint(app().app_id) + "/reviews"))
                            .items,
                        );
                        setSelected(await request(endpoint(app().app_id)));
                      }, "Review saved.");
                    }}
                  >
                    <label>
                      Your rating
                      <input
                        name="rating"
                        type="number"
                        min="0"
                        max="5"
                        step="0.1"
                        value="5"
                        required
                      />
                    </label>
                    <label>
                      Your review
                      <textarea
                        name="review"
                        rows="4"
                        maxLength={10000}
                        required
                        placeholder="What was it like to use this application?"
                      />
                    </label>
                    <button class="button primary" disabled={busy()}>
                      Save review
                    </button>
                  </form>
                </Show>
              </Show>
              <Show when={detailsTab() === "access"}>
                <h3>IAM synchronization</h3>
                <p class="muted">Refresh the accepted configuration after an IAM change.</p>
                <Show when={reconciliation()?.state !== "idle"}><p role="status">Synchronization {reconciliation()?.state}<Show when={reconciliation()?.error}> · {reconciliation()?.error}</Show></p></Show>
                <button class="button outline" disabled={busy()} onClick={()=>void act(async()=>{
                  setReconciliation(await request(endpoint(app().app_id)+"/reconciliation",{method:"POST",body:"{}"}));
                  setSelected(await request(endpoint(app().app_id)));await load();
                })}>Refresh IAM state</button>
                <div class="section-divider" />
                <h3>Application access</h3>
                <p class="muted">Requested revision {app().revision} · Effective revision {app().effective_revision}. IAM accepts scopes before they take effect.</p>
                <details><summary>Requested scopes</summary><Code text={JSON.stringify(app().config.app_scope || {}, null, 2)} /></details>
                <details><summary>Effective scopes</summary><Code text={JSON.stringify(app().effective_config?.app_scope || {}, null, 2)} /></details>
                <h3>Application secret</h3>
                <p class="muted">Rotating replaces the credential used by your backend. Save the replacement and update your backend configuration as soon as IAM accepts the rotation.</p>
                <form class="application-form" onSubmit={(e) => {
                  e.preventDefault();
                  void act(async () => {
                    const proof=rotationProof(); setRotationProof("");
                    const result=await request(endpoint(app().app_id)+"/secret-rotations", {method:"POST",headers:{"If-Match":String(app().revision)},body:JSON.stringify({step_up_assertion:proof || null})});
                    if(result.app_secret) setSecret(result.app_secret);
                    setNotice(result.state === "accepted" ? "Secret rotated. Save the replacement." : "Rotation is pending IAM acceptance. Its operation is saved below.");
                    setAppOperations((await request(endpoint(app().app_id)+"/operations")).items);
                    setRotationConfirmation(""); setRotationProof("");
                  });
                }}>
                  <label>IAM application-secret step-up assertion<input type="password" autocomplete="off" value={rotationProof()} onInput={e=>setRotationProof(e.currentTarget.value)} /></label>
                  <p class="muted">Use fresh IAM verification for application.client_secret.rotate.</p>
                  <label>Type the application ID to rotate its secret<input value={rotationConfirmation()} onInput={(e)=>setRotationConfirmation(e.currentTarget.value)} /></label>
                  <button class="button outline" disabled={busy() || rotationConfirmation()!==app().app_id || app().effective_revision!==app().revision || appOperations().some((o)=>o.state==="pending")}>Rotate application secret</button>
                </form>
                <h3>Your configuration operations</h3>
                <For each={appOperations().filter(op=>!op.kind.startsWith("webhook."))}>{(op)=><article class="review-thread">
                  <strong>{op.kind==="secret.rotate"?"Secret rotation":"Application configuration"}</strong> <Badge>{op.state}</Badge>
                  <p class="app-id">{op.id}</p>
                  <Show when={op.error}><p class="muted">{op.error === "integration_unavailable" ? "Waiting for IAM’s protected management integration." : op.error === "step_up_required" ? "IAM requires fresh identity verification before accepting this rotation." : op.error}</p></Show>
                  <Show when={op.state==="pending"}><button class="button outline" disabled={busy()} onClick={()=>void act(async()=>{
                    const proof=rotationProof(); setRotationProof("");
                    const result=op.kind==="secret.rotate" ? await request(endpoint(app().app_id)+"/secret-rotations",{method:"POST",headers:{"If-Match":String(op.revision),"Idempotency-Key":op.idempotency_key},body:JSON.stringify({step_up_assertion:proof || null})}) : await request(`/api/v1/operations/${op.id}/retry`,{method:"POST",body:"{}"});
                    setRotationProof("");
                    if(result.app_secret) setSecret(result.app_secret);
                    setAppOperations((await request(endpoint(app().app_id)+"/operations")).items);
                    setSelected(await request(endpoint(app().app_id)));
                  })}>Retry operation</button></Show>
                  <button class="button outline" disabled={busy()} onClick={()=>void act(async()=>{
                    const result=await request(`/api/v1/operations/${op.id}/result`,{method:"POST",body:"{}"});
                    if(result.app_secret) setSecret(result.app_secret);else setNotice(result.message || "IAM has not returned a secret for this operation.");
                    setAppOperations((await request(endpoint(app().app_id)+"/operations")).items);
                  })}>Recover one-time secret</button>
                </article>}</For>
                <WebhookSettings app={app()} refresh={async()=>{
                  setSelected(await request(endpoint(app().app_id)));
                  setAppOperations((await request(endpoint(app().app_id)+"/operations")).items);
                }} />
              </Show>
              <Show when={detailsTab() === "release"}>
                <Show when={pendingRelease()?.appId === app().app_id}>
                  <p class="muted">Selected archive: {pendingRelease()?.file.name} · {pendingRelease()?.channel === "dev" ? "Development" : "Production"}</p>
                  <button class="button outline" disabled={busy()} onClick={() => void act(sendRelease, "CLI release uploaded.")}>Retry release upload</button>
                </Show>
                <Show when={app().iam_revision === 0}>
                  <div class="inline-notice">
                    Your application is saved and awaiting IAM activation.
                    Public access remains disabled.
                  </div>
                </Show>
                <h3>Upload a CLI release</h3>
                <p class="muted">
                  Package all required targets with honeycomb pack, then upload
                  the .tar.gz archive. Its version must use x.x.x format.
                </p>
                <label>Upload release channel
                  <select value={uploadChannel()} disabled={busy()} onChange={e => setUploadChannel(e.currentTarget.value as ReleaseChannel | "")}>
                    <option value="" disabled>Choose a release channel</option>
                    <option value="prod">Production — official releases</option>
                    <option value="dev">Development — experimental releases</option>
                  </select>
                </label>
                <label class="upload-zone">
                  <Upload size={24} />
                  <strong>
                    {busy()
                      ? "Uploading and validating…"
                      : "Choose a .tar.gz release"}
                  </strong>
                  <span>All six required targets · up to 512 MiB</span>
                  <input
                    aria-label="Upload CLI archive"
                    type="file"
                    accept=".gz,.tar.gz,application/gzip"
                    disabled={busy() || !uploadChannel()}
                    onChange={(e) => {
                      const file = e.currentTarget.files?.[0];
                      e.currentTarget.value = "";
                      void upload(file);
                    }}
                  />
                </label>
                <div class="section-divider" />
                <Releases app={app()} refresh={releaseRevision()} busy={busy()} setBusy={setBusy} changed={async () => {
                  setSelected(await request(endpoint(app().app_id)));
                  setPublication(await request(endpoint(app().app_id) + "/publication"));
                  setReleaseRevision(value => value + 1);
                  await load();
                }} />
                <div class="section-divider" />
                <h3>{app().visibility === "public" ? "Request scope review" : "Request public release"}</h3>
                <p class="muted">
                  Provider scope approvals and Honeycomb verification are
                  required before public access is enabled.
                </p>
                <Show when={app().config.visibility === "public" && !app().latest_version}>
                  <p class="muted">Public approval will be requested automatically after your first valid production CLI release is uploaded and IAM accepts the configuration. Upload a production release or promote a development release when it is ready.</p>
                </Show>
                <Show when={!(publication()?.items || []).some((p: any) => p.revision === app().revision)}>
                <form
                  class="review-form"
                  onSubmit={(e) => {
                    e.preventDefault();
                    const data = new FormData(e.currentTarget);
                    void act(async () => {
                      await request(endpoint(app().app_id) + "/publication", {
                        method: "POST",
                        headers: { "If-Match": String(app().revision) },
                        body: JSON.stringify({ message: data.get("message") }),
                      });
                      setPublication(
                        await request(endpoint(app().app_id) + "/publication"),
                      );
                    }, "Publication requested. Honeycomb will publish automatically once all required approvals pass.");
                  }}
                >
                  <label>
                    Tell reviewers about your application
                    <textarea
                      name="message"
                      rows="4"
                      required
                      maxLength={10000}
                    />
                  </label>
                  <button
                    class="button primary"
                    disabled={
                      busy() ||
                      !app().latest_version ||
                      app().iam_revision === 0 || (app().visibility !== "public" && app().effective_revision !== app().revision)
                    }
                  >
                    {app().visibility === "public" ? "Request scope review" : "Request publication"}
                    <ArrowRight size={16} />
                  </button>
                </form>
                </Show>
                <For each={publication()?.items || []}>
                  {(p) => (
                    <section class="review-thread">
                      <Badge>{publicationStatus(p.state)}</Badge>
                      <Show when={p.state === "awaiting_activation" || p.state === "activating"}><p class="muted">All approvals passed. Honeycomb is completing publication with IAM and release storage.</p></Show>
                      <Show when={p.error}><p class="muted">{p.error}</p></Show>
                      <Show when={p.activation?.error}><p class="field-error">{p.activation.error}</p></Show>
                      <For each={p.activation?.archives || []}>{(archive)=><p>{archive.channel === "dev" ? "Dev" : "Production"} release {archive.version} · {archive.state}{archive.error ? ` · ${archive.error}` : ""}</p>}</For>
                      <Show when={(p.state==="awaiting_activation" || p.state==="activating") && (p.error || p.activation?.error)}><button class="button primary" disabled={busy() || (p.state==="activating" && !p.activation?.idempotency_key)} onClick={()=>void act(async()=>{
                        const headers:Record<string,string>={"If-Match":String(p.revision)};if(p.activation?.idempotency_key)headers["Idempotency-Key"]=p.activation.idempotency_key;
                        const result=await request(`/api/v1/review-requests/${p.id}/activate`,{method:"POST",headers,body:"{}"});
                        setPublication(await request(endpoint(app().app_id)+"/publication"));setSelected(await request(endpoint(app().app_id)));await load();
                        setNotice(result.state==="accepted" ? "Application published." : "Publication is pending. Check IAM and archive progress above.");
                      })}>Retry publication</button></Show>
                      <Show when={p.state === "awaiting_review_plan"}><button class="button outline" disabled={busy()} onClick={()=>void act(async()=>{await request(`/api/v1/review-requests/${p.id}/plan`,{method:"POST",headers:{"If-Match":String(p.revision)},body:"{}"});setPublication(await request(endpoint(app().app_id)+"/publication"));})}>Retry review planning</button></Show>
                      <For each={p.gates || []}>{(gate)=><p>{gate.provider} · {gate.state} <button class="text-button" onClick={()=>void openReview({id:p.id,provider:gate.provider})}>Open discussion</button></p>}</For>
                      <For each={p.messages}>{(m) => <p>{m.message}</p>}</For>
                      <form
                        onSubmit={(e) => {
                          e.preventDefault();
                          const data = new FormData(e.currentTarget);
                          void act(async () => {
                            await request(
                              endpoint(app().app_id) + "/publication/messages",
                              {
                                method: "POST",
                                body: JSON.stringify({
                                  message: data.get("message"),
                                }),
                              },
                            );
                            setPublication(
                              await request(
                                endpoint(app().app_id) + "/publication",
                              ),
                            );
                          });
                        }}
                      >
                        <label>
                          Add to the discussion
                          <textarea name="message" required maxLength={10000} />
                        </label>
                        <button class="button outline" disabled={busy()}>
                          Send reply
                        </button>
                      </form>
                    </section>
                  )}
                </For>
              </Show>
            </>
          )}
        </Show>
      </Dialog>
      <Dialog
        open={showCreate()}
        title={editing() ? "Edit application" : "Create an application"}
        close={() => void closeCreate()}
      >
        <form class="application-form" onSubmit={createApp}>
          <Show when={error()}>
            <p class="field-error" role="alert">
              {error()}
            </p>
          </Show>
          <p class="muted">
            {editing()
              ? "Changes take effect after IAM accepts them. Other organization admins can continue from this draft."
              : "Public approval is requested automatically when your configuration and first production release are ready. Your organization’s admins can continue from this draft."}
          </p>
          <div class="form-grid">
            <label>
              Organization
              <select
                required
                value={form().org_id}
                disabled={!!editing()}
                onChange={(e) => {
                  saveGeneration++;
                  setDraftId(crypto.randomUUID());
                  setDraftRevision(0);
                  edit("org_id", e.currentTarget.value);
                }}
              >
                <For each={admins()}>
                  {([org]) => <option value={org}>{org}</option>}
                </For>
              </select>
            </label>
            <label>
              Application handle
              <input
                required
                pattern="[a-z0-9][a-z0-9-]{0,63}"
                maxLength={64}
                disabled={!!editing()}
                placeholder="my-application"
                value={form().local_app_id}
                onInput={(e) => edit("local_app_id", e.currentTarget.value)}
              />
              <small>
                {form().org_id || "organization"}&gt;
                {form().local_app_id || "my-application"} · Cannot change after
                creation
              </small>
            </label>
          </div>
          <label>
            Application name
            <input
              required
              maxLength={100}
              placeholder="A name for your application"
              value={form().name}
              onInput={(e) => edit("name", e.currentTarget.value)}
            />
          </label>
          <label>
            Description
            <textarea
              required
              rows="5"
              value={form().description}
              onInput={(e) => edit("description", e.currentTarget.value)}
              aria-label="Description"
              placeholder="What does your application do, who is it for, and how will they use it?"
            />
            <small>
              {form().description.trim().split(/\s+/).filter(Boolean).length}{" "}
              words · 50–1,000 required
            </small>
          </label>
          <label>
            Publication preference
            <select value={form().visibility} onChange={e => edit("visibility", e.currentTarget.value)}>
              <option value="public">Public — automatically request approval</option>
              <option value="private">Private — do not request approval</option>
            </select>
            <small>Public access starts only after all required approvals. This preference controls new requests; it does not revoke an existing public release.</small>
          </label>
          <div class="section-divider" />
          <Show when={!editing()}>
            <h3>First CLI release</h3>
            <p class="muted">Choose the .tar.gz produced by honeycomb pack. It will be validated before registration and uploaded after your application is saved.</p>
            <label>First release channel
              <select value={form().draft_release_channel || ""} required={!!firstRelease()} disabled={busy()} onChange={e => edit("draft_release_channel", e.currentTarget.value)}>
                <option value="" disabled>Choose a release channel</option>
                <option value="prod">Production — official releases</option>
                <option value="dev">Development — experimental releases</option>
              </select>
              <small>Each channel has its own x.x.x versions. A development release can be promoted later with a new production version.</small>
            </label>
            <label class="upload-zone">
              <Upload size={24} />
              <strong>{firstRelease()?.name || "Choose a .tar.gz release"}</strong>
              <span>All six required targets · up to 512 MiB</span>
              <input type="file" aria-label="First CLI archive" accept=".gz,.tar.gz,application/gzip" disabled={busy()} onChange={e => setFirstRelease(e.currentTarget.files?.[0])} />
            </label>
            <Show when={firstRelease()}><button type="button" class="text-link" disabled={busy()} onClick={() => setFirstRelease(undefined)}>Add release later</button></Show>
            <p class="muted">You can add the release later. A production release is required before publishing; a development release can be promoted when ready. Files are not saved in drafts; select the archive again when reopening a draft.</p>
            <div class="section-divider" />
          </Show>
          <LogoField org={form().org_id} value={form().logo_url} change={url => edit("logo_url", url)} onBusy={setLogoUploading} />
          <div class="section-divider" />
          <h3>Connect to IAM</h3>
          <div class="form-grid">
            <label>
              Webhook URL
              <input
                type="url"
                required
                placeholder="https://backend.example.com/webhook/"
                value={form().webhook_url}
                onInput={(e) => edit("webhook_url", e.currentTarget.value)}
              />
            </label>
            <label>
              Webhook signing secret
              <input
                type="password"
                autocomplete="new-password"
                required={!editing()}
                minLength={32}
                maxLength={512}
                value={form().webhook_secret}
                onInput={(e) =>
                  setForm({ ...form(), webhook_secret: e.currentTarget.value })
                }
              />
              <small>
                {editing()
                  ? "Leave blank to keep the current secret. New values need at least 32 characters."
                  : "At least 32 characters. Not saved in drafts."}
              </small>
            </label>
          </div>
          <label>Test application idle days<input type="number" min="1" max="36500" required value={form().testing_idle_days ?? 30} onInput={e=>edit("testing_idle_days", Number(e.currentTarget.value))} /><small>Default: 30 days. Active applications keep their dependencies available.</small></label>
          <fieldset>
            <legend>Webhook updates</legend>
            <div class="checkboxes">
              <For each={["membership", "updates", "trust", "full"]}>
                {(category) => (
                  <label>
                    <input
                      type="checkbox"
                      checked={form().webhook_scope.includes(category)}
                      onChange={(e) =>
                        edit(
                          "webhook_scope",
                          e.currentTarget.checked
                            ? [...form().webhook_scope, category]
                            : form().webhook_scope.filter(
                                (c: string) => c !== category,
                              ),
                        )
                      }
                    />
                    {category}
                  </label>
                )}
              </For>
            </div>
          </fieldset>
          <div class="form-grid">
            <label>
              Website <span class="optional">Optional</span>
              <input
                type="url"
                value={form().website_url}
                onInput={(e) => edit("website_url", e.currentTarget.value)}
              />
            </label>
            <label>
              Documentation <span class="optional">Optional</span>
              <input
                type="url"
                value={form().docs_url}
                onInput={(e) => edit("docs_url", e.currentTarget.value)}
              />
            </label>
          </div>
          <button
            type="button"
            class="text-link"
            onClick={() => setAdvanced(!advanced())}
          >
            {advanced() ? "Hide" : "Configure"} scopes & OBO endpoints
            <ArrowRight size={14} />
          </button>
          <Show when={advanced()}>
            <div class="advanced">
              <ScopePicker org={form().org_id} value={scopeText()} change={(value)=>{setScopeText(value);scheduleSave();}} />
              <label>
                Backend origin
                <input
                  type="url"
                  placeholder="https://backend.example.com"
                  value={form().base_url}
                  onInput={(e) => edit("base_url", e.currentTarget.value)}
                />
                <small>
                  Required if you expose OBO endpoints. Use an origin without a
                  trailing slash.
                </small>
              </label>
              <label>
                Application scopes
                <textarea
                  class="json-input"
                  rows="8"
                  value={scopeText()}
                  onInput={(e) => {
                    setScopeText(e.currentTarget.value);
                    scheduleSave();
                  }}
                />
                <small>
                  IAM and external scopes must be accepted by IAM before taking
                  effect.
                </small>
              </label>
              <label>
                OBO endpoints
                <textarea
                  class="json-input"
                  rows="5"
                  value={oboText()}
                  onInput={(e) => {
                    setOboText(e.currentTarget.value);
                    scheduleSave();
                  }}
                />
              </label>
            </div>
          </Show>
          <div class="form-footer">
            <span role="status">{draftStatus()}</span>
            <button
              class="button primary"
              disabled={busy() || logoUploading() || !admins().length}
            >
              {busy()
                ? "Saving…"
                : editing()
                  ? "Save configuration"
                  : "Create application"}
              <ArrowRight size={16} />
            </button>
          </div>
        </form>
      </Dialog>
      <Dialog open={showSettings()} title="Telemetry settings" close={() => setShowSettings(false)}>
        <p>Share usage and diagnostics to help improve Honeycomb. This setting applies to this website in this browser.</p>
        <label class="checkboxes"><input type="checkbox" checked={telemetry()} onChange={e => { const enabled=e.currentTarget.checked; setTelemetryPreference(enabled); setTelemetry(enabled); }} />Share usage and diagnostics</label>
        <p class="muted">Events include screen names, operation outcomes and timings. Form contents, tokens, secrets and search text are excluded.</p>
      </Dialog>
      <Dialog
        open={!!selectedEnv()}
        title={selectedEnv()?.name || "Testing environment"}
        close={() => {
          setSelectedEnv(undefined);
          setEnvironmentAction("");
        }}
      >
        <Show when={selectedEnv()}>
          {(env) => (
            <>
              <Show when={error()}>
                <p class="field-error" role="alert">
                  {error()}
                </p>
              </Show>
              <p class="app-id">{env().environment_id}</p>
              <p>
                <Badge>{env().state}</Badge> · Generation {env().generation} ·
                Revision {env().revision}
              </p>
              <h3>Service progress</h3>
              <div class="environment-services">
                <For each={env().services || []}>
                  {(service) => (
                    <article>
                      <strong>{service.app_id}</strong>{" "}
                      <Badge>{service.state}</Badge>
                      <Show when={service.error}>
                        <p class="field-error">{service.error}</p>
                      </Show>
                    </article>
                  )}
                </For>
              </div>
              <Show
                when={env().operation_pending || !["ready", "deleted", "purged"].includes(env().state)}
              >
                <p class="muted">
                  Every participating service must confirm the operation. Retry
                  continues the incomplete steps; existing ready imports stay available.
                </p>
                <button
                  class="button primary"
                  disabled={busy()}
                  onClick={() => void runEnvironmentAction("retry")}
                >
                  Retry setup
                </button>
              </Show>
              <EnvironmentRetention environment={env()} refresh={async()=>setSelectedEnv(await request(`/api/v1/environments/${env().environment_id}`))} />
              <Show when={env().imports?.length}>
                <h3>Pinned applications</h3>
                <For each={env().imports}>{(item) => <p><strong>{item.app_id}</strong><br /><small>Source revision {item.source_revision}{item.selected_release ? ` · Release ${item.selected_release}` : ""}</small></p>}</For>
              </Show>
              <Show when={env().state === "ready" && !env().operation_pending}>
                <details class="advanced">
                  <summary>Import an application</summary>
                  <form class="application-form" onSubmit={(e) => {
                    e.preventDefault();
                    void act(async () => {
                      await request(`/api/v1/environments/${env().environment_id}/imports`, { method: "POST", headers: { "If-Match": String(env().revision) }, body: JSON.stringify({ app_id: importApp(), release: importRelease() || null, refresh: refreshImport() }) });
                      setSelectedEnv(await request(`/api/v1/environments/${env().environment_id}`));
                    }, "Import requested. Check service progress for readiness.");
                  }}>
                    <p class="muted">Dependencies are included automatically. Private applications require current production access. Imports keep their organization and start private in this environment.</p>
                    <label>Application ID<input required placeholder="tos>briefcase" value={importApp()} onInput={(e) => setImportApp(e.currentTarget.value)} /></label>
                    <label>Release (optional)<input placeholder="Latest available" value={importRelease()} onInput={(e) => setImportRelease(e.currentTarget.value)} /></label>
                    <label class="check"><input type="checkbox" checked={refreshImport()} onChange={(e) => setRefreshImport(e.currentTarget.checked)} />Refresh existing pins from production</label>
                    <button class="button primary" disabled={busy()}>Import application</button>
                  </form>
                </details>
                <div class="environment-actions">
                  <For
                    each={[
                      ["rotate-key", "Rotate key"],
                      ["clean", "Clean test data"],
                      ["delete", "Delete environment"],
                    ]}
                  >
                    {([action, label]) => (
                      <button
                        class="button outline"
                        onClick={() => {
                          setEnvironmentAction(action);
                          setConfirmationName("");
                        }}
                      >
                        {label}
                      </button>
                    )}
                  </For>
                </div>
              </Show>
              <Show when={env().state === "deleted"}>
                <p class="muted">
                  Recovery deadline:{" "}
                  {new Date(env().purge_after * 1000).toLocaleString()}
                </p>
                <Show
                  when={env().purge_after > Date.now() / 1000}
                  fallback={
                    <button
                      class="button outline"
                      onClick={() => setEnvironmentAction("purge")}
                    >
                      Permanently purge
                    </button>
                  }
                >
                  <button
                    class="button primary"
                    disabled={busy()}
                    onClick={() => void runEnvironmentAction("restore")}
                  >
                    Restore environment
                  </button>
                </Show>
              </Show>
              <Show when={environmentAction()}>
                <div class="environment-confirm">
                  <p>
                    {environmentAction() === "clean"
                      ? "Cleaning permanently removes this environment’s test data and sessions. The environment and current root key remain."
                      : environmentAction() === "delete"
                        ? "Deletion disables access and starts a 30-day recovery window."
                        : environmentAction() === "purge"
                          ? "Purging permanently erases the remaining test data and root key."
                          : "Rotation invalidates the old key immediately. Save the new key shown after this request."}
                  </p>
                  <label>
                    Type the environment name to confirm
                    <input
                      value={confirmationName()}
                      onInput={(e) =>
                        setConfirmationName(e.currentTarget.value)
                      }
                    />
                  </label>
                  <button
                    class="button primary"
                    disabled={busy() || confirmationName() !== env().name}
                    onClick={() =>
                      void runEnvironmentAction(environmentAction())
                    }
                  >
                    Confirm {environmentAction()}
                  </button>
                  <button
                    class="button quiet"
                    onClick={() => setEnvironmentAction("")}
                  >
                    Cancel
                  </button>
                </div>
              </Show>
            </>
          )}
        </Show>
      </Dialog>
      <Dialog
        open={showEnv()}
        title="Create a testing environment"
        close={() => setShowEnv(false)}
      >
        <form
          class="application-form"
          onSubmit={(e) => {
            e.preventDefault();
            void act(async () => {
              await request("/api/v1/environments", {
                method: "POST",
                body: JSON.stringify({ org_id: envOrg(), name: envName() }),
              });
              setShowEnv(false);
              setEnvs((await request("/api/v1/environments")).items);
            }, "Environment created. Open Manage to check service readiness.");
          }}
        >
          <Show when={error()}>
            <p class="field-error" role="alert">
              {error()}
            </p>
          </Show>
          <label>
            Organization
            <select
              value={envOrg()}
              onChange={(e) => setEnvOrg(e.currentTarget.value)}
            >
              <For each={memberships()}>
                {([org]) => <option value={org}>{org}</option>}
              </For>
            </select>
          </label>
          <label>
            Environment name
            <input
              required
              maxLength={100}
              value={envName()}
              onInput={(e) => setEnvName(e.currentTarget.value)}
              placeholder="Release testing"
            />
          </label>
          <p class="muted">
            The environment becomes ready after IAM and every linked application
            confirm preparation.
          </p>
          <button class="button primary" disabled={busy()}>
            Create environment
            <ArrowRight size={16} />
          </button>
        </form>
      </Dialog>
      <Dialog open={!!selectedReview()} title="Application review" close={()=>{setSelectedReview(undefined);setSentRevision(value=>value+1);}}>
        <Show when={selectedReview()}>{(review)=><>
          <Show when={error()}><p class="field-error" role="alert">{error()}</p></Show>
          <h3>{review().configuration?.name || review().app_id}</h3><p class="app-id">{review().app_id} · Revision {review().revision}</p>
          <p>{review().configuration?.description}</p><p><strong>{review().provider}</strong> · {review().scopes.join(", ") || "Publication validation"}</p><Badge>{review().state}</Badge>
          <For each={review().messages}>{(message)=><article class="review-thread"><small>{message.actor}</small><p>{message.message}</p></article>}</For>
          <form class="review-form" onSubmit={(e)=>{e.preventDefault();const data=new FormData(e.currentTarget);void act(async()=>{await request(reviewEndpoint(review())+"/messages",{method:"POST",body:JSON.stringify({message:data.get("message")})});setSelectedReview(await request(reviewEndpoint(review())));});}}>
            <label>Reply to this review<textarea name="message" required maxLength={10000} rows="3"/></label><button class="button outline" disabled={busy()}>Send reply</button>
          </form>
          <Show when={review().can_decide}><h3>Review decision</h3><p class="muted">IAM verifies your current reviewer authority. Approval takes effect only after IAM accepts it. Denial requires an explanation.</p>
          <form class="review-form" onSubmit={(e)=>{e.preventDefault();void act(async()=>{
            const headers:Record<string,string>={"If-Match":String(review().revision)};if(review().decision?.idempotency_key) headers["Idempotency-Key"]=review().decision.idempotency_key;
            const result = await request(reviewEndpoint(review())+"/decisions",{method:"POST",headers,body:JSON.stringify({decision:reviewDecision(),reason:reviewReason()})});
            setNotice(result.publication_state === "published" ? "Approved and published." : result.publication_error || (result.publication_state === "activating" ? "Approved. Publication is completing automatically." : "Review decision saved."));
            setSelectedReview(await request(reviewEndpoint(review())));setReviewInbox((await request("/api/v1/review-requests")).items);
          });}}>
            <label>Decision<select value={reviewDecision()} disabled={!!review().decision} onChange={(e)=>setReviewDecision(e.currentTarget.value)}><option value="approve">Approve</option><option value="deny">Deny</option></select></label>
            <label>Reason<textarea required={reviewDecision()==="deny"} maxLength={10000} rows="3" value={reviewReason()} disabled={!!review().decision} onInput={(e)=>setReviewReason(e.currentTarget.value)}/></label>
            <button class="button primary" disabled={busy() || review().decision?.state==="accepted" || (!!review().decision && !review().decision.idempotency_key)}>{review().decision?.state==="pending" ? "Retry decision" : "Submit decision"}</button>
          </form></Show>
        </>}</Show>
      </Dialog>
      <Dialog
        open={!!secret()}
        title="Save your application secret"
        close={() => setSecret("")}
      >
        <p>
          This secret is shown once. Save it in your application’s backend
          configuration.
        </p>
        <Code text={secret()} />
        <button class="button primary" onClick={() => setSecret("")}>
          I’ve saved it
        </button>
      </Dialog>
      <Dialog
        open={!!envKey()}
        title="Testing environment key"
        close={() => setEnvKey("")}
      >
        <p>
          This key grants root access inside this testing environment. It grants
          no production access.
        </p>
        <Code text={envKey()} />
        <button class="button outline" onClick={() => setEnvKey("")}>
          Close
        </button>
      </Dialog>
    </div>
  );
}
