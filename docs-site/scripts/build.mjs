import { readFile, writeFile, mkdir } from "node:fs/promises";
import { marked } from "marked";
const pages = JSON.parse(await readFile("pages.json", "utf8"));
const base = "https://docs.honeycomb.teamofsilicons.com";
const escape = (s) =>
  String(s).replace(
    /[&<>"']/g,
    (c) =>
      ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[
        c
      ],
  );
const href = (p) => `/${p.slug ? p.slug + "/" : ""}`;
const built = await readFile("dist/index.html", "utf8");
const assets = [
  ...built.matchAll(
    /<(?:script|link)\b[^>]*(?:src|href)="\/assets\/[^"<>]+"[^>]*>(?:<\/script>)?/g,
  ),
]
  .map((m) => m[0])
  .join("\n");
if (!assets.includes("<script") || !assets.includes("stylesheet"))
  throw new Error("Missing built assets");
const groups = [...new Set(pages.map((p) => p.group))];
const search = [];
const logo =
  '<svg viewBox="0 0 32 32" aria-hidden="true"><path d="M16 2 28 9v14l-12 7L4 23V9Z"/><path d="m4 9 12 7 12-7M16 16v14"/></svg>';
function shell(page, article, toc, i) {
  const previous = pages[i - 1],
    next = pages[i + 1];
  return `<!doctype html><html lang="en"><head><meta charset="UTF-8"><meta name="viewport" content="width=device-width,initial-scale=1"><meta name="theme-color" content="#fffdf9"><title>${escape(page.title)} · Honeycomb Docs</title><meta name="description" content="${escape(page.description)}"><link rel="canonical" href="${base}${href(page)}"><meta property="og:title" content="${escape(page.title)} · Honeycomb Docs"><meta property="og:description" content="${escape(page.description)}"><meta property="og:type" content="website"><meta property="og:url" content="${base}${href(page)}"><link rel="icon" href="/favicon.svg" type="image/svg+xml">${assets}</head><body><a class="skip" href="#content">Skip to content</a><header class="topbar"><a class="brand" href="/" aria-label="Honeycomb Docs home">${logo}<strong>honeycomb</strong><span>Docs</span></a><div id="tools"></div><nav class="external" aria-label="Products"><a href="https://honeycomb.teamofsilicons.com">Library ↗</a><a class="console" href="https://console.honeycomb.teamofsilicons.com">Open console ↗</a></nav></header><div class="layout"><nav id="sidebar" class="sidebar" aria-label="Documentation"><div class="edition">DOCUMENTATION <span>v0.1</span></div>${groups
    .map(
      (group) =>
        `<section><h2>${escape(group)}</h2>${pages
          .filter((p) => p.group === group)
          .map(
            (p) =>
              `<a ${p.slug === page.slug ? 'aria-current="page"' : ""} href="${href(p)}">${escape(p.title)}</a>`,
          )
          .join("")}</section>`,
    )
    .join(
      "",
    )}<a class="source-link" href="https://github.com/teamofsilicons/silicon-honeycomb">Source on GitHub ↗</a></nav><main id="content" tabindex="-1"><div class="eyebrow">${escape(page.group)} <span>/</span> HONEYCOMB</div><h1>${escape(page.title)}</h1><p class="lead">${escape(page.description)}</p>${page.slug === "" ? '<div class="intro-rule"><span>BUILD SOMETHING. SHARE IT WELL.</span><span>09 — 2026</span></div>' : ""}<article>${article}</article><footer class="article-footer"><div class="page-links">${previous ? `<a href="${href(previous)}"><small>← PREVIOUS</small>${escape(previous.title)}</a>` : "<span></span>"}${next ? `<a href="${href(next)}"><small>NEXT →</small>${escape(next.title)}</a>` : ""}</div><div class="meta"><span>Honeycomb · MIT · 0.1.0</span><a href="/markdown/${page.slug || "index"}.md">Read as Markdown</a><a href="https://github.com/teamofsilicons/silicon-honeycomb/blob/main/docs-site/content/${page.slug || "index"}.md">View source ↗</a></div></footer></main><aside class="toc" aria-label="On this page"><h2>On this page</h2>${toc.map((h) => `<a href="#${h.id}">${escape(h.text)}</a>`).join("")}<div class="toc-note">A shared home for<br>Silicon applications.</div></aside></div></body></html>`;
}
for (const [i, page] of pages.entries()) {
  let md = await readFile(`content/${page.slug || "index"}.md`, "utf8");
  md = md.replace(
    "<!-- ROUTES -->",
    await readFile("content/routes.generated.md", "utf8"),
  );
  const toc = [],
    ids = new Map();
  const renderer = new marked.Renderer();
  renderer.heading = function ({ tokens, depth }) {
    const html = this.parser.parseInline(tokens),
      text = html
        .replace(/<[^>]*>/g, "")
        .replace(/&gt;/g, ">")
        .replace(/&amp;/g, "&");
    const stem = text
      .toLowerCase()
      .replace(/[^a-z0-9]+/g, "-")
      .replace(/^-|-$/g, "");
    const n = ids.get(stem) || 0;
    ids.set(stem, n + 1);
    const id = stem + (n ? `-${n}` : "");
    if (depth === 2) toc.push({ id, text });
    return `<h${depth} id="${id}">${html}<a class="anchor" aria-label="Link to ${escape(text)}" href="#${id}">#</a></h${depth}>`;
  };
  let article = marked.parse(md, { renderer, gfm: true });
  article = article
    .replace(/<table>/g, '<div class="table-wrap"><table>')
    .replace(/<\/table>/g, "</table></div>");
  const dir = `dist${href(page)}`;
  await mkdir(dir, { recursive: true });
  await writeFile(`${dir}index.html`, shell(page, article, toc, i));
  await mkdir("dist/markdown", { recursive: true });
  await writeFile(`dist/markdown/${page.slug || "index"}.md`, md);
  search.push({
    title: page.title,
    group: page.group,
    url: href(page),
    description: page.description,
    text: article.replace(/<[^>]*>/g, " ").replace(/\s+/g, " "),
  });
}
await writeFile("dist/search.json", JSON.stringify(search));
await writeFile(
  "dist/sitemap.xml",
  `<?xml version="1.0" encoding="UTF-8"?><urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">${pages.map((p) => `<url><loc>${base}${href(p)}</loc></url>`).join("")}</urlset>`,
);
await writeFile(
  "dist/robots.txt",
  `User-agent: *\nAllow: /\nSitemap: ${base}/sitemap.xml\n`,
);
await writeFile(
  "dist/llms.txt",
  "# Honeycomb documentation\n\n" +
    pages
      .map(
        (p) =>
          `- [${p.title}](${base}/markdown/${p.slug || "index"}.md): ${p.description}`,
      )
      .join("\n"),
);
await writeFile(
  "dist/404.html",
  shell(
    {
      slug: "404",
      title: "This page is missing.",
      description: "Find your way back to the Honeycomb documentation.",
      group: "Documentation",
    },
    '<p>Try the search above, or <a href="/">return to the documentation home</a>.</p>',
    [],
    -1,
  ).replace(
    '<meta name="description"',
    '<meta name="robots" content="noindex"><meta name="description"',
  ),
);
console.log(
  `Built ${pages.length} static documentation pages, search, Markdown, and sitemap.`,
);
