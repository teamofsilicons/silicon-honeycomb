import { cp, mkdir, rm, writeFile } from "node:fs/promises";
import { resolve } from "node:path";

const site = process.env.HONEYCOMB_SITE;
if (!["library", "console"].includes(site)) {
  throw new Error("Set HONEYCOMB_SITE to library or console before building Vercel output.");
}
const backend = new URL(process.env.HONEYCOMB_API_URL || "https://backend.honeycomb.teamofsilicons.com");
if (backend.protocol !== "https:" || backend.pathname !== "/" || backend.search || backend.hash || backend.username || backend.password) {
  throw new Error("HONEYCOMB_API_URL must be an exact HTTPS backend origin.");
}
const output = resolve(".vercel/output");
await rm(output, { recursive: true, force: true });
await mkdir(output, { recursive: true });
await cp("dist", `${output}/static`, { recursive: true });
await writeFile(`${output}/config.json`, JSON.stringify({
  version: 3,
  routes: [
    { src: "/(.*)", headers: {
      "X-Content-Type-Options": "nosniff", "Referrer-Policy": "no-referrer",
      "X-Frame-Options": "DENY",
      "Content-Security-Policy": "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data: https:; font-src 'self'; connect-src 'self'; object-src 'none'; frame-ancestors 'none'; base-uri 'self'; form-action 'self'",
    }, continue: true },
    { src: "/((?:api|auth)(?:/.*)?)", dest: `${backend.origin}/web/${site}/$1`, headers: { "Cache-Control": "private, no-store" } },
    { src: "/assets/(.*)", headers: { "Cache-Control": "public, max-age=31536000, immutable" }, continue: true },
    { handle: "filesystem" },
    { src: "/(.*)", dest: "/index.html", headers: { "Cache-Control": "no-store" } },
  ],
}, null, 2));
console.info(`Prepared static ${site} deployment with authenticated requests proxied to the backend session service.`);
