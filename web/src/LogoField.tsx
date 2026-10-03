import { Show, createSignal, onCleanup, createEffect } from "solid-js";
import { ImagePlus, Link2, LoaderCircle, Upload, X } from "lucide-solid";
import { authorizeStorage } from "./storage-authorization";
import { ApiError, request, safeLink } from "./api";
import "./release-pages.css";
export function LogoField(props: {
  org: string;
  value: string;
  change: (url: string) => void;
  onBusy: (busy: boolean) => void;
}) {
  const [busy, setBusy] = createSignal(false),
    [error, setError] = createSignal(""),
    [previewFailed, setPreviewFailed] = createSignal(false);
  const [needsAuthorization, setNeedsAuthorization] = createSignal(false);
  const [dragging, setDragging] = createSignal(false);
  const [attempt, setAttempt] = createSignal<{
    file: File;
    org: string;
    key: string;
  }>();
  let active = true;
  let picker: HTMLInputElement | undefined;
  createEffect(() => {
    props.value;
    setPreviewFailed(false);
  });
  onCleanup(() => {
    active = false;
    props.onBusy(false);
  });
  async function upload(file?: File) {
    if (busy()) return;
    if (file) {
      if (!file.size || file.size > 2 * 1024 * 1024) {
        setAttempt(undefined);
        setError("Choose an image no larger than 2 MiB.");
        return;
      }
      if (!["image/png", "image/jpeg", "image/webp"].includes(file.type)) {
        setAttempt(undefined);
        setError("Choose a PNG, JPEG, or WebP image.");
        return;
      }
      setAttempt({ file, org: props.org, key: crypto.randomUUID() });
    }
    const pending = attempt();
    if (!pending || pending.org !== props.org) return;
    setBusy(true);
    props.onBusy(true);
    setError("");
    setNeedsAuthorization(false);
    try {
      const result = await request(
        `/api/v1/organizations/${encodeURIComponent(pending.org)}/logos`,
        {
          method: "POST",
          headers: {
            "Content-Type": "application/octet-stream",
            "Idempotency-Key": pending.key,
          },
          body: pending.file,
        },
      );
      if (active && props.org === pending.org) {
        props.change(result.logo_url);
        setAttempt(undefined);
      }
    } catch (error) {
      if (active) {
        const required =
          error instanceof ApiError &&
          error.code === "storage_authorization_required";
        setError(required ? "" : (error as Error).message);
        setNeedsAuthorization(required);
      }
    } finally {
      if (active) {
        setBusy(false);
        props.onBusy(false);
      }
    }
  }
  async function connect() {
    setBusy(true);
    props.onBusy(true);
    setError("");
    try {
      await authorizeStorage(props.org);
      setNeedsAuthorization(false);
    } catch (error) {
      setError((error as Error).message);
      return;
    } finally {
      setBusy(false);
      props.onBusy(false);
    }
    await upload();
  }
  return (
    <section class="logo-field arc-logo-field" aria-label="Application logo" aria-busy={busy()}>
      <div class="delegation-heading logo-field-heading">
        <div><h3>Application logo <span class="optional">Optional</span></h3><p>Give your application a recognizable face in the library.</p></div>
        <Show when={props.value}>
          <button
            type="button"
            class="text-button"
            disabled={busy()}
            onClick={() => props.change("")}
          >
            <X size={14} />
            Remove logo
          </button>
        </Show>
      </div>
      <div class="logo-upload-row">
        <div class="logo-preview-frame">
          <Show
            when={safeLink(props.value) && !previewFailed()}
            fallback={<ImagePlus size={27} stroke-width={1.4} />}
          >
            <img
              src={safeLink(props.value)}
              alt="Application logo preview"
              referrerpolicy="no-referrer"
              onError={() => setPreviewFailed(true)}
            />
          </Show>
        </div>
        <button
          type="button"
          class="upload-zone logo-dropzone"
          classList={{ "is-dragging": dragging(), "is-busy": busy() }}
          disabled={busy() || !props.org}
          onClick={() => picker?.click()}
          onDragOver={(event) => {
            event.preventDefault();
            if (!busy() && props.org) setDragging(true);
          }}
          onDragLeave={() => setDragging(false)}
          onDrop={(event) => {
            event.preventDefault();
            setDragging(false);
            if (!busy() && props.org) void upload(event.dataTransfer?.files[0]);
          }}
        >
          <span class="logo-upload-icon"><Show when={busy()} fallback={<Upload size={20} aria-hidden="true"/>}><LoaderCircle size={20} class="logo-upload-spinner" aria-hidden="true"/></Show></span>
          <strong>
            {busy()
              ? needsAuthorization()
                ? "Waiting for permission…"
                : "Uploading logo…"
              : dragging()
                ? "Drop your logo here"
                : props.value
                ? "Replace logo"
                : "Choose a logo"}
          </strong>
          <span class="logo-dropzone-hint">{busy() ? needsAuthorization() ? "Complete the storage permission step" : "Your image is being prepared" : "or drag and drop an image here"}</span>
          <span class="logo-file-limits">PNG, JPEG or WebP · 2 MiB · Up to 2048 × 2048</span>
        </button>
        <input
            ref={picker}
            hidden
            tabIndex={-1}
            type="file"
            aria-label="Upload a logo"
            accept="image/png,image/jpeg,image/webp"
            disabled={busy() || !props.org}
            onChange={(e) => {
              const file = e.currentTarget.files?.[0];
              e.currentTarget.value = "";
              if (file) void upload(file);
            }}
        />
      </div>
      <Show when={busy() && attempt()}><div class="logo-upload-status" role="status"><span>{attempt()?.file.name}</span><span>{needsAuthorization() ? "Waiting for permission…" : "Uploading…"}</span><Show when={!needsAuthorization()}><div class="logo-upload-progress" role="progressbar" aria-label="Uploading application logo"><span/></div></Show></div></Show>
      <Show when={!props.org}>
        <small>Select an organization to upload a logo.</small>
      </Show>
      <Show when={previewFailed()}>
        <small role="status">
          The logo preview could not load. Check the URL or upload another
          image.
        </small>
      </Show>
      <details class="logo-url-option">
        <summary><Link2 size={14} aria-hidden="true"/>Use an image URL instead</summary>
        <label>
          Logo URL
          <input
            type="url"
            placeholder="https://example.com/logo.png"
            value={props.value || ""}
            disabled={busy()}
            onInput={(e) => props.change(e.currentTarget.value)}
          />
        </label>
      </details>
      <small class="logo-privacy-note">
        Your logo is publicly viewable wherever your application appears.
      </small>
      <Show when={needsAuthorization()}>
        <div class="logo-permission-note"><div><strong>Connect storage to upload your logo</strong><p>Review how Honeycomb can store your image. Your selected file is kept ready.</p></div><button class="button primary" type="button" disabled={busy()} onClick={() => void connect()}>Review storage permissions</button></div>
      </Show>
      <Show when={error()}>
        <p class="field-error" role="alert">
          {error()}
        </p>
        <Show when={attempt()?.org === props.org && !needsAuthorization()}>
          <button
            type="button"
            class="button outline"
            disabled={busy()}
            onClick={() => void upload()}
          >
            Retry logo upload
          </button>
        </Show>
      </Show>
    </section>
  );
}
