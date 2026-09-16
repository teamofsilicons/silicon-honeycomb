import {
  createSignal,
  createMemo,
  For,
  Show,
  onMount,
  onCleanup,
} from "solid-js";
import { render } from "solid-js/web";
import "@fontsource/ibm-plex-sans/400.css";
import "@fontsource/ibm-plex-sans/500.css";
import "@fontsource/ibm-plex-mono/400.css";
import "@fontsource/source-serif-4/400.css";
import "./styles.css";
type Page = {
  title: string;
  group: string;
  description: string;
  text: string;
  url: string;
};
function Tools() {
  const [query, setQuery] = createSignal("");
  const [pages, setPages] = createSignal<Page[]>([]);
  const [error, setError] = createSignal(false);
  const [loaded, setLoaded] = createSignal(false);
  const [menu, setMenu] = createSignal(false);
  let dialog!: HTMLDialogElement;
  let input!: HTMLInputElement;
  let opener: HTMLElement | null = null;
  const results = createMemo(() => {
    const words = query().toLowerCase().trim().split(/\s+/).filter(Boolean);
    return pages()
      .map((p) => ({
        p,
        score: words.reduce(
          (n, w) =>
            n +
            (p.title.toLowerCase().includes(w) ? 10 : 0) +
            (p.description.toLowerCase().includes(w) ? 3 : 0),
          0,
        ),
      }))
      .filter(({ p }) =>
        words.every((w) =>
          (p.title + " " + p.description + " " + p.text)
            .toLowerCase()
            .includes(w),
        ),
      )
      .sort((a, b) => b.score - a.score)
      .slice(0, 12)
      .map(({ p }) => p);
  });
  async function load() {
    setError(false);
    try {
      const r = await fetch("/search.json");
      if (!r.ok) throw new Error("Search unavailable");
      setPages(await r.json());
      setLoaded(true);
    } catch {
      setError(true);
    }
  }
  function open() {
    opener = document.activeElement as HTMLElement;
    dialog.showModal();
    input.focus();
    if (!loaded()) void load();
  }
  function close() {
    dialog.close();
    opener?.focus();
  }
  function toggleMenu() {
    const next = !menu();
    setMenu(next);
    document.getElementById("sidebar")?.classList.toggle("is-open", next);
  }
  onMount(() => {
    document.documentElement.classList.add("enhanced");
    const key = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "k") {
        e.preventDefault();
        if (dialog.open) close();
        else open();
      }
      if (e.key === "Escape" && menu()) toggleMenu();
    };
    document.addEventListener("keydown", key);
    const cleanup: Array<() => void> = [];
    document.querySelectorAll<HTMLPreElement>("article pre").forEach((pre) => {
      const button = document.createElement("button");
      button.type = "button";
      button.className = "copy";
      button.textContent = "Copy";
      button.setAttribute("aria-label", "Copy code");
      const status = document.createElement("span");
      status.className = "sr-only";
      status.setAttribute("role", "status");
      button.onclick = async () => {
        try {
          await navigator.clipboard.writeText(
            pre.querySelector("code")?.textContent || "",
          );
          button.textContent = "Copied";
          status.textContent = "Code copied";
        } catch {
          button.textContent = "Select code";
          status.textContent =
            "Clipboard unavailable. Select the code to copy it.";
          const range = document.createRange();
          range.selectNodeContents(pre.querySelector("code")!);
          getSelection()?.removeAllRanges();
          getSelection()?.addRange(range);
        }
      };
      pre.append(button, status);
      cleanup.push(() => {
        button.remove();
        status.remove();
      });
    });
    onCleanup(() => {
      document.removeEventListener("keydown", key);
      cleanup.forEach((fn) => fn());
    });
  });
  return (
    <>
      <button
        type="button"
        class="menu-button"
        aria-label="Toggle documentation menu"
        aria-expanded={menu()}
        aria-controls="sidebar"
        onClick={toggleMenu}
      >
        ☰
      </button>
      <button
        type="button"
        class="search-button"
        aria-label="Search documentation"
        onClick={open}
      >
        <span aria-hidden="true">⌕</span>
        <span>Search documentation</span>
        <kbd>⌘ K</kbd>
      </button>
      <dialog
        ref={dialog}
        aria-label="Search documentation"
        onCancel={close}
        onClick={(e) => {
          if (e.target === dialog) close();
        }}
      >
        <div class="search-inner">
          <div class="search-heading">
            <label for="docs-search">Search the docs</label>
            <button type="button" onClick={close} aria-label="Close search">
              Esc
            </button>
          </div>
          <input
            ref={input}
            id="docs-search"
            type="search"
            autocomplete="off"
            placeholder="Try uploads, CLI, permissions…"
            value={query()}
            onInput={(e) => setQuery(e.currentTarget.value)}
            onKeyDown={(e) => {
              if (e.key === "ArrowDown") {
                e.preventDefault();
                dialog
                  .querySelector<HTMLAnchorElement>(".search-result")
                  ?.focus();
              }
            }}
          />
          <div class="search-results" aria-live="polite">
            <Show when={error()}>
              <p>
                Search could not load.{" "}
                <button class="text-button" onClick={load}>
                  Retry
                </button>
              </p>
            </Show>
            <Show when={!error() && !loaded()}>
              <p>Loading documentation…</p>
            </Show>
            <Show when={loaded() && !results().length}>
              <p>No results. Try “package”, “login”, or “client”.</p>
            </Show>
            <For each={results()}>
              {(p) => (
                <a class="search-result" href={p.url}>
                  <small>{p.group}</small>
                  <strong>{p.title}</strong>
                  <span>{p.description}</span>
                </a>
              )}
            </For>
          </div>
          <div class="search-foot">
            Search stays in your browser.{" "}
            <span>Tab to navigate · Enter to open</span>
          </div>
        </div>
      </dialog>
    </>
  );
}
render(() => <Tools />, document.getElementById("tools")!);
