import { readFile, stat } from "node:fs/promises";
import path from "node:path";
const pages = JSON.parse(await readFile("pages.json", "utf8"));
const routes = new Set(pages.map((p) => `/${p.slug ? p.slug + "/" : ""}`));
let checked = 0;
for (const page of pages) {
  const file = `dist/${page.slug ? page.slug + "/" : ""}index.html`;
  const html = await readFile(file, "utf8");
  if ((html.match(/<h1\b/g) || []).length !== 1)
    throw new Error(`${file}: expected one h1`);
  if (/<!-- ROUTES -->/.test(html)) throw new Error("Unexpanded routes");
  const ids = [...html.matchAll(/\bid="([^"]+)"/g)].map((m) => m[1]);
  if (new Set(ids).size !== ids.length)
    throw new Error(`${file}: duplicate IDs`);
  for (const match of html.matchAll(/(?:href|src)="([^"<>]+)"/g)) {
    const url = match[1];
    if (url.startsWith("#")) {
      if (!ids.includes(url.slice(1)))
        throw new Error(`${file}: missing anchor ${url}`);
    } else if (url.startsWith("/") && !url.startsWith("//")) {
      const target = url.split("#")[0];
      if (target.endsWith("/")) {
        if (!routes.has(target))
          throw new Error(`${file}: broken route ${target}`);
      } else await stat(path.join("dist", target));
    }
    checked++;
  }
}
const manifest = await readFile("public/examples/honeycomb.yaml", "utf8");
const app = JSON.parse(
  await readFile("public/examples/application.json", "utf8"),
);
if (app.description.split(/\s+/).length < 50)
  throw new Error("Example description is too short");
for (const target of [
  "linux-x86_64",
  "linux-aarch64",
  "windows-x86_64",
  "windows-aarch64",
  "macos-x86_64",
  "macos-aarch64",
])
  if (!manifest.includes(target + ":")) throw new Error(`Missing ${target}`);
const index = JSON.parse(await readFile("dist/search.json", "utf8"));
if (index.length !== pages.length)
  throw new Error("Search index missing pages");
console.log(
  `Validated ${pages.length} pages, ${checked} links/assets, search coverage, and download examples.`,
);
