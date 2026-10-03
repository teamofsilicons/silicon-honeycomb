# Honeycomb documentation website

Public static documentation at https://docs.honeycomb.teamofsilicons.com.
Markdown content renders to individual HTML pages; SolidJS supplies local search,
keyboard controls, mobile navigation, and copy buttons. Pages remain readable without
JavaScript. The build has no runtime backend or secret dependencies.

```sh
npm ci
npm run build
npx playwright install chromium
npm test
npm run preview
```

From the repository root, after building `honeycomb`:

```sh
python3 docs-site/scripts/generate_reference.py
python3 docs-site/scripts/generate_reference.py --check
python3 docs-site/scripts/check_examples.py
```

Edit guides in `content/` and navigation metadata in `pages.json`. CLI help and the
route table are generated; do not hand-edit them. Keep availability statements dated
and distinguish local fixture coverage from actual production acceptance.
The six payloads in the documentation checker validate structure, not native execution.

Editorial walkthroughs live in `content/guides/` and use the same static build as
reference documentation. Register each one in `pages.json` with a stable nested
slug, `group: "Guides"`, and `layout: "article"`, for example:

```json
{
  "slug": "guides/team-of-silicons-ready-applications",
  "title": "Building a Team of Silicons-ready application",
  "group": "Guides",
  "layout": "article",
  "description": "A practical guide to integrating an application with the Silicon ecosystem."
}
```

Write the body without an H1 or frontmatter; the template renders the title and
description. H2 headings become chapter links, and reading time is estimated at
build time. Link new guides from the documentation homepage when they introduce a
primary workflow. Guides have their own reading layout, remain searchable, and do
not interrupt the previous/next sequence of reference pages. Their HTML and
Markdown downloads work without JavaScript.

Build emits page HTML, per-page Markdown, local search JSON, a sitemap, `robots.txt`,
and `llms.txt`. Assets and downloadable templates are in `public/`. A normal Vercel
project uses `npm run build` and `dist` as configured in `vercel.json`.

```sh
vercel link --project silicon-honeycomb-docs --scope saketdev12-5675s-projects
vercel deploy --prod --yes --scope saketdev12-5675s-projects
```

DNS remains at Namecheap; add only the project-specific record reported by Vercel
for `docs.honeycomb.teamofsilicons.com`. See Vercel's
[custom domain instructions](https://vercel.com/docs/domains/working-with-domains/add-a-domain).
