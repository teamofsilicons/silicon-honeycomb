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
      onCancel={(e) => {
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
  webhook_url: "",
  webhook_secret: "",
  webhook_scope: ["membership"],
  base_url: "",
  website_url: "",
  docs_url: "",
  logo_url: "",
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
    consoleOrigin: "http://localhost:4174",
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
  const [selected, setSelected] = createSignal<AppRecord>();
  const [detailsTab, setDetailsTab] = createSignal("overview");
  const [reviews, setReviews] = createSignal<any[]>([]);
  const [publication, setPublication] = createSignal<any>();
  const [showCreate, setShowCreate] = createSignal(false);
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
    setDetailsTab("overview");
    setReviews([]);
    setPublication(undefined);
    await act(async () => {
      setSelected(await request<AppRecord>(endpoint(app.app_id)));
      setReviews((await request(endpoint(app.app_id) + "/reviews")).items);
      if (canManage(app))
        setPublication(await request(endpoint(app.app_id) + "/publication"));
    });
  }
  function beginCreate(draft?: any) {
    setError("");
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
    await act(async () => {
      const payload: Record<string, any> = {
        ...form(),
        app_scope: JSON.parse(scopeText()),
        obo_endpoints: JSON.parse(oboText()),
      };
      for (const field of Object.keys(payload))
        if (field.startsWith("draft_")) delete payload[field];
      for (const key of ["base_url", "website_url", "docs_url", "logo_url"])
        if (!payload[key]) payload[key] = null;
      const result = await request(
        editing() ? endpoint(editing()!) : "/api/v1/apps",
        {
          method: editing() ? "PUT" : "POST",
          headers: editing()
            ? { "If-Match": String(form().draft_edit_revision) }
            : {},
          body: JSON.stringify(payload),
        },
      );
      setShowCreate(false);
      saveGeneration++;
      clearTimeout(saveTimer);
      if (result.app_secret) setSecret(result.app_secret);
      setNotice(
        result.state === "accepted"
          ? editing()
            ? "Application configuration updated."
            : "Application created. Upload its first CLI release to continue."
          : "Application saved. IAM configuration is pending.",
      );
      await load();
    });
  }
  async function upload(file: File | undefined) {
    if (!file || !selected()) return;
    await act(async () => {
      await request(endpoint(selected()!.app_id) + "/releases", {
        method: "POST",
        headers: {
          "Content-Type": "application/gzip",
          "If-Match": String(selected()!.revision),
        },
        body: file,
      });
      setSelected(await request(endpoint(selected()!.app_id)));
      await load();
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
                      Applications start private to their organization. You
                      decide when to request a public release.
                    </span>
                  </div>
                </section>
              }
            >
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
                                  : "No releases"}
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
                        '/bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/teamofsilicons/silicon-honeycomb/main/install.sh)"'
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
                      Applications start private. Upload a release, then request
                      publication when you’re ready.
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
        close={() => setSelected(undefined)}
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
                    {app().latest_version || "No releases"}
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
                </Show>
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
              <Show when={detailsTab() === "release"}>
                <Show when={app().iam_revision === 0}>
                  <div class="inline-notice">
                    Your application is saved and awaiting IAM activation.
                    Public access remains disabled.
                  </div>
                </Show>
                <h3>Upload a CLI release</h3>
                <p class="muted">
                  Package all required targets with honeycomb pack, then upload
                  the .tar.gz archive.
                </p>
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
                    disabled={busy()}
                    onChange={(e) => void upload(e.currentTarget.files?.[0])}
                  />
                </label>
                <div class="section-divider" />
                <h3>Request public release</h3>
                <p class="muted">
                  Provider scope approvals and Honeycomb verification are
                  required before public access is enabled.
                </p>
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
                    }, "Publication request saved. Your application remains private during review.");
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
                      app().iam_revision === 0
                    }
                  >
                    Request publication
                    <ArrowRight size={16} />
                  </button>
                </form>
                <For each={publication()?.items || []}>
                  {(p) => (
                    <section class="review-thread">
                      <Badge>{p.state.replaceAll("_", " ")}</Badge>
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
              : "Start with a private home for your application. Your organization’s admins can continue from this draft."}
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
              disabled={busy() || !admins().length}
            >
              {busy()
                ? "Saving…"
                : editing()
                  ? "Save configuration"
                  : "Create private application"}
              <ArrowRight size={16} />
            </button>
          </div>
        </form>
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
                when={!["ready", "deleted", "purged"].includes(env().state)}
              >
                <p class="muted">
                  This environment becomes available after every service
                  confirms readiness. Retry continues the incomplete steps.
                </p>
                <button
                  class="button primary"
                  disabled={busy()}
                  onClick={() => void runEnvironmentAction("retry")}
                >
                  Retry setup
                </button>
              </Show>
              <Show when={env().state === "ready"}>
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
