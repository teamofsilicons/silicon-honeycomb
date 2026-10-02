import {
  createResource,
  createSignal,
  For,
  Show,
  onCleanup,
  createEffect,
  createUniqueId,
} from "solid-js";
import {
  ArrowLeft,
  ArrowUpRight,
  Check,
  Copy,
  Download,
  Package,
  Star,
  Terminal,
  BookOpen,
  ExternalLink,
} from "lucide-solid";
import {
  request,
  endpoint,
  releaseEndpoint,
  safeLink,
  type AppRecord,
} from "./api";
import { applicationPath, type ApplicationRoute } from "./application-route";
import { StatusBadge } from "./ui/Arc";
import "./public-application.css";

type Release = {
  version: string;
  channel: "prod" | "dev";
  visibility: string;
  created_at: number;
  size: number;
  sha256: string;
};
function InstallCommand(props: { command: string }) {
  const [copied, setCopied] = createSignal(false),
    [error, setError] = createSignal("");
  let timer: ReturnType<typeof setTimeout>;
  onCleanup(() => clearTimeout(timer));
  return (
    <>
      <div class="code-line">
        <code>{props.command}</code>
        <button
          aria-label="Copy install command"
          onClick={async () => {
            try {
              await navigator.clipboard.writeText(props.command);
              setCopied(true);
              setError("");
              clearTimeout(timer);
              timer = setTimeout(() => setCopied(false), 2000);
            } catch {
              setError("Copy is unavailable. Select the command to copy it.");
            }
          }}
        >
          {copied() ? <Check size={16} /> : <Copy size={16} />}
        </button>
      </div>
      <Show when={error()}>
        <small role="status">{error()}</small>
      </Show>
    </>
  );
}
export default function PublicApplication(props: {
  route: ApplicationRoute;
  authenticated: boolean;
}) {
  const [channel, setChannel] = createSignal<"prod" | "dev">(
    props.route.channel || "prod",
  );
  const [tab, setTab] = createSignal("overview");
  const sectionId = createUniqueId();
  const [notice, setNotice] = createSignal("");
  const [saving, setSaving] = createSignal(false);
  const [app, { refetch: refreshApp }] = createResource(
    () => props.route.appId,
    (id) => request<AppRecord>(endpoint(id)),
  );
  const [releases, { refetch: refreshReleases }] = createResource(
    () => ({ id: props.route.appId, channel: channel() }),
    (value) =>
      request<{ items: Release[] }>(
        `${releaseEndpoint(value.id)}/releases?channel=${value.channel}`,
      ),
  );
  const [reviews, { refetch: refreshReviews }] = createResource(
    () => props.route.appId,
    (id) =>
      request<{ items: { rating: number; review: string }[] }>(
        endpoint(id) + "/reviews",
      ),
  );
  const [production, { refetch: refreshProduction }] = createResource(
    () => props.route.appId,
    (id) =>
      request<{ items: Release[] }>(
        `${releaseEndpoint(id)}/releases?channel=prod`,
      ),
  );
  const appData = () => (app.error ? undefined : app());
  const releaseData = () => (releases.error ? undefined : releases());
  const productionData = () => (production.error ? undefined : production());
  const reviewData = () => (reviews.error ? undefined : reviews());
  const current = () =>
    productionData()?.items.find(
      (release) =>
        release.visibility === "public" && release.channel === "prod",
    );
  const history = () =>
    releaseData()?.items.filter((item) => item.visibility === "public");
  const exact = () =>
    releaseData()?.items.find(
      (release) =>
        release.version === props.route.version &&
        release.channel === props.route.channel,
    );
  const release = () => (props.route.version ? exact() : current());
  const releaseLoading = () =>
    props.route.version ? releases.loading : production.loading;
  const releaseError = () =>
    props.route.version ? releases.error : production.error;
  createEffect(() => {
    if (appData())
      document.title = `${appData()!.name}${props.route.version ? ` ${props.route.version} · ${props.route.channel === "dev" ? "Development" : "Production"}` : ""} — Honeycomb`;
  });
  async function starApplication() {
    if (saving()) return;
    setSaving(true);
    setNotice("");
    try {
      await request(endpoint(props.route.appId) + "/star", {
        method: "PUT",
        body: "{}",
      });
      await refreshApp();
      setNotice("Application starred.");
    } catch (error) {
      setNotice((error as Error).message);
    } finally {
      setSaving(false);
    }
  }
  async function saveReview(event: SubmitEvent) {
    event.preventDefault();
    if (saving()) return;
    const data = new FormData(event.currentTarget as HTMLFormElement);
    setSaving(true);
    setNotice("");
    try {
      await request(endpoint(props.route.appId) + "/reviews", {
        method: "PUT",
        body: JSON.stringify({
          rating: Number(data.get("rating")),
          review: data.get("review"),
        }),
      });
      await Promise.all([refreshReviews(), refreshApp()]);
      setNotice("Your review has been saved.");
    } catch (error) {
      setNotice((error as Error).message);
    } finally {
      setSaving(false);
    }
  }
  return (
    <section class="application-page">
      <a class="text-link application-back" href="/">
        <ArrowLeft size={16} /> All applications
      </a>
      <Show when={app.error}>
        <div class="banner error" role="alert">
          Could not open this application.{" "}
          {String(app.error?.message || "Please try again.")}
        </div>
        <button class="button outline" onClick={() => void refreshApp()}>
          Try again
        </button>
      </Show>
      <Show when={app.loading && !appData()}>
        <p class="loading" role="status">
          Opening application…
        </p>
      </Show>
      <Show when={appData()}>
        {(value) => (
          <>
            <header class="application-hero">
              <div class="application-hero-top">
                <div class="application-brand">
                  <div class="package-icon">
                    <Show
                      when={safeLink(value().config.logo_url)}
                      fallback={<Package size={34} stroke-width={1.3} />}
                    >
                      {(url) => (
                        <img src={url()} alt="" referrerpolicy="no-referrer" />
                      )}
                    </Show>
                  </div>
                  <div>
                    <span class="eyebrow">
                      {props.route.version
                        ? `${props.route.channel === "dev" ? "DEVELOPMENT" : "PRODUCTION"} RELEASE`
                        : "APPLICATION"}
                    </span>
                    <h1>{value().name}</h1>
                    <p class="app-id">
                      {value().app_id}
                      <Show when={props.route.version}>
                        {" "}
                        · {props.route.version}
                      </Show>
                    </p>
                  </div>
                </div>
                <Show when={props.authenticated}>
                  <button
                    class="button outline application-star"
                    disabled={saving()}
                    onClick={() => void starApplication()}
                  >
                    <Star size={16} />
                    Star <span>{value().stars}</span>
                  </button>
                </Show>
              </div>
              <div class="detail-stats">
                <span>
                  <Star size={15} />
                  {value().reviews
                    ? `${value().rating.toFixed(1)} · ${value().reviews} reviews`
                    : "No reviews yet"}
                </span>
                <span>
                  <Download size={15} />
                  {value().installs.toLocaleString()} downloads
                </span>
                <StatusBadge
                  tone={value().visibility === "public" ? "success" : "neutral"}
                >
                  {value().visibility === "public"
                    ? "Public application"
                    : "Private application"}
                </StatusBadge>
              </div>
            </header>
            <Show when={notice()}>
              <p role="status" class="inline-notice">
                {notice()}
              </p>
            </Show>
            <Show when={props.route.version}>
              <a
                class="text-link application-current-link"
                href={applicationPath(value().app_id)}
              >
                View current production release <ArrowUpRight size={14} />
              </a>
            </Show>
            <div class="application-columns">
              <div class="application-main">
                <div
                  class="application-section-tabs"
                  role="tablist"
                  aria-label="Application sections"
                  onKeyDown={(event) => {
                    if (
                      !["ArrowLeft", "ArrowRight", "Home", "End"].includes(
                        event.key,
                      )
                    )
                      return;
                    event.preventDefault();
                    const next =
                      event.key === "Home"
                        ? "overview"
                        : event.key === "End"
                          ? "reviews"
                          : tab() === "overview"
                            ? "reviews"
                            : "overview";
                    setTab(next);
                    event.currentTarget
                      .querySelector<HTMLButtonElement>(
                        `[data-section="${next}"]`,
                      )
                      ?.focus();
                  }}
                >
                  <button
                    type="button"
                    role="tab"
                    id={`${sectionId}-overview-tab`}
                    aria-controls={`${sectionId}-overview-panel`}
                    aria-selected={tab() === "overview"}
                    tabIndex={tab() === "overview" ? 0 : -1}
                    data-section="overview"
                    onClick={() => setTab("overview")}
                  >
                    Overview
                  </button>
                  <button
                    type="button"
                    role="tab"
                    id={`${sectionId}-reviews-tab`}
                    aria-controls={`${sectionId}-reviews-panel`}
                    aria-selected={tab() === "reviews"}
                    tabIndex={tab() === "reviews" ? 0 : -1}
                    data-section="reviews"
                    onClick={() => setTab("reviews")}
                  >
                    Reviews <span>{value().reviews}</span>
                  </button>
                </div>
                <Show when={tab() === "overview"}>
                  <div
                    class="application-tab-panel"
                    role="tabpanel"
                    id={`${sectionId}-overview-panel`}
                    aria-labelledby={`${sectionId}-overview-tab`}
                    tabIndex={0}
                  >
                    <section class="application-about">
                      <h2>About this application</h2>
                      <p class="application-description">
                        {value().description}
                      </p>
                      <div class="detail-links">
                        <For
                          each={[
                            ["website_url", "Website"],
                            ["docs_url", "Documentation"],
                          ]}
                        >
                          {([key, label]) => (
                            <Show when={safeLink(value().config[key])}>
                              {(url) => (
                                <a
                                  href={url()}
                                  target="_blank"
                                  rel="noopener noreferrer"
                                >
                                  {key === "docs_url" ? (
                                    <BookOpen size={15} />
                                  ) : (
                                    <ExternalLink size={15} />
                                  )}
                                  {label}
                                  <ArrowUpRight size={14} />
                                </a>
                              )}
                            </Show>
                          )}
                        </For>
                      </div>
                    </section>
                    <section class="public-release-history">
                      <div class="delegation-heading">
                        <div>
                          <h2>Release history</h2>
                          <p>
                            Explore releases and install a specific version.
                          </p>
                        </div>
                        <label class="compact-label">
                          Channel
                          <select
                            value={channel()}
                            onChange={(event) => {
                              if (props.route.version)
                                window.location.assign(
                                  applicationPath(props.route.appId),
                                );
                              else
                                setChannel(
                                  event.currentTarget.value as "prod" | "dev",
                                );
                            }}
                            disabled={!!props.route.version}
                          >
                            <option value="prod">Production</option>
                            <option value="dev">Development</option>
                          </select>
                        </label>
                      </div>
                      <Show when={channel() === "dev"}>
                        <p class="muted">
                          Development releases are experimental and receive
                          their own updates.
                        </p>
                      </Show>
                      <Show when={releases.error}>
                        <p class="field-error" role="alert">
                          Could not load releases.{" "}
                          {String(releases.error?.message || "")}
                        </p>
                        <button
                          class="text-button"
                          onClick={() => {
                            void refreshReleases();
                            void refreshProduction();
                          }}
                        >
                          Try again
                        </button>
                      </Show>
                      <Show when={releases.loading}>
                        <p role="status" class="muted">
                          Loading releases…
                        </p>
                      </Show>
                      <Show
                        when={
                          !releases.loading &&
                          !releases.error &&
                          !history()?.length
                        }
                      >
                        <p class="muted">
                          No{" "}
                          {channel() === "prod" ? "production" : "development"}{" "}
                          releases available.
                        </p>
                      </Show>
                      <For each={history()}>
                        {(item) => (
                          <a
                            class="public-release-row"
                            href={applicationPath(
                              value().app_id,
                              item.channel,
                              item.version,
                            )}
                          >
                            <div>
                              <strong>{item.version}</strong>
                              <StatusBadge
                                tone={
                                  item.channel === "prod"
                                    ? "neutral"
                                    : "warning"
                                }
                              >
                                {item.channel === "prod"
                                  ? "Production"
                                  : "Development"}
                              </StatusBadge>
                              <Show when={item.visibility !== "public"}>
                                <StatusBadge>Private</StatusBadge>
                              </Show>
                              <Show
                                when={
                                  item.version === current()?.version &&
                                  item.channel === "prod"
                                }
                              >
                                <StatusBadge tone="info">Current</StatusBadge>
                              </Show>
                            </div>
                            <span>
                              {new Date(
                                item.created_at * 1000,
                              ).toLocaleDateString(undefined, {
                                year: "numeric",
                                month: "short",
                                day: "numeric",
                              })}
                            </span>
                            <ArrowUpRight size={15} />
                          </a>
                        )}
                      </For>
                    </section>
                  </div>
                </Show>
                <Show when={tab() === "reviews"}>
                  <div
                    class="application-tab-panel application-reviews"
                    role="tabpanel"
                    id={`${sectionId}-reviews-panel`}
                    aria-labelledby={`${sectionId}-reviews-tab`}
                    tabIndex={0}
                  >
                    <div class="application-reviews-heading">
                      <h2>Community reviews</h2>
                      <p>Feedback from people using {value().name}.</p>
                    </div>
                    <Show when={reviews.error}>
                      <p class="field-error" role="alert">
                        Reviews could not be loaded.
                      </p>
                      <button
                        class="text-button"
                        onClick={() => void refreshReviews()}
                      >
                        Retry reviews
                      </button>
                    </Show>
                    <Show when={reviews.loading}>
                      <p role="status">Loading reviews…</p>
                    </Show>
                    <Show
                      when={
                        !reviews.loading &&
                        !reviews.error &&
                        !reviewData()?.items.length
                      }
                    >
                      <div class="application-reviews-empty">
                        <Star size={21} stroke-width={1.6} />
                        <h3>Be the first to share a review</h3>
                        <p>
                          Help others decide whether this application is right
                          for them.
                        </p>
                      </div>
                    </Show>
                    <For each={reviewData()?.items}>
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
                    <Show
                      when={props.authenticated}
                      fallback={
                        <a
                          class="button outline"
                          href={`/auth/login?next=${encodeURIComponent(window.location.pathname)}`}
                        >
                          Sign in to leave a review
                        </a>
                      }
                    >
                      <form class="review-form" onSubmit={saveReview}>
                        <h3>Write a review</h3>
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
                          />
                        </label>
                        <button class="button primary" disabled={saving()}>
                          {saving() ? "Saving…" : "Save review"}
                        </button>
                      </form>
                    </Show>
                  </div>
                </Show>
              </div>
              <aside
                class="application-install"
                aria-label="Install application"
              >
                <div class="application-install-heading">
                  <span class="application-terminal-icon">
                    <Terminal size={19} />
                  </span>
                  <h2>Install application</h2>
                </div>
                <span class="eyebrow">
                  {props.route.version
                    ? "EXACT VERSION"
                    : "CURRENT PRODUCTION RELEASE"}
                </span>
                <Show when={releaseLoading()}>
                  <p role="status">Loading release…</p>
                </Show>
                <Show when={release()}>
                  {(item) => (
                    <>
                      <div class="application-release-heading">
                        <h3>{item().version}</h3>
                        <StatusBadge
                          tone={
                            item().visibility === "public"
                              ? "success"
                              : "neutral"
                          }
                        >
                          {item().visibility === "public"
                            ? "Published"
                            : "Private"}
                        </StatusBadge>
                      </div>
                      <p class="muted application-release-channel">
                        {item().channel === "dev"
                          ? "Development"
                          : "Production"}{" "}
                        channel
                      </p>
                      <p class="release-date">
                        Uploaded{" "}
                        {new Date(item().created_at * 1000).toLocaleDateString(
                          undefined,
                          { year: "numeric", month: "long", day: "numeric" },
                        )}
                      </p>
                      <InstallCommand
                        command={`honeycomb install '${value().app_id}${item().channel === "dev" ? ">test" : ""}${props.route.version ? `@${item().version}` : ""}'`}
                      />
                      <p class="muted">
                        {props.route.version
                          ? "Installs this exact version."
                          : "Installs the current published production release and follows production updates."}
                      </p>
                      <Show when={!props.route.version}>
                        <a
                          class="text-link"
                          href={applicationPath(
                            value().app_id,
                            "prod",
                            item().version,
                          )}
                        >
                          Version details
                          <ArrowUpRight size={14} />
                        </a>
                      </Show>
                      <Show when={props.route.version}>
                        <dl class="release-facts">
                          <dt>Archive size</dt>
                          <dd>{(item().size / 1048576).toFixed(1)} MiB</dd>
                          <dt>SHA-256</dt>
                          <dd class="checksum">{item().sha256}</dd>
                        </dl>
                      </Show>
                    </>
                  )}
                </Show>
                <Show when={!releaseLoading() && !releaseError() && !release()}>
                  <p class="muted">
                    {props.route.version
                      ? "This release is not available to your account. Check the version and channel, or sign in if you have access."
                      : "No public production release is available yet."}
                  </p>
                </Show>
                <Show when={releaseError()}>
                  <p class="field-error">
                    Release information is unavailable.{" "}
                    <button
                      class="text-button"
                      onClick={() => {
                        void refreshReleases();
                        void refreshProduction();
                      }}
                    >
                      Try again
                    </button>
                  </p>
                </Show>
                <div class="application-install-help">
                  <h3>New to Honeycomb?</h3>
                  <p class="muted">
                    Install the Honeycomb CLI to add applications to your
                    workspace.
                  </p>
                  <a
                    class="text-link"
                    href="https://docs.honeycomb.teamofsilicons.com"
                    target="_blank"
                    rel="noopener noreferrer"
                  >
                    Installation guide
                    <ArrowUpRight size={14} />
                  </a>
                </div>
              </aside>
            </div>
          </>
        )}
      </Show>
    </section>
  );
}
