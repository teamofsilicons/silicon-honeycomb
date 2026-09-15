import express from "express";
import { AsyncLocalStorage } from "node:async_hooks";
import { DatabaseSync } from "node:sqlite";
import {
  randomBytes,
  createCipheriv,
  createDecipheriv,
  createHash,
  timingSafeEqual,
} from "node:crypto";
import { mkdirSync } from "node:fs";
import { dirname, resolve } from "node:path";

const production = process.env.NODE_ENV === "production";
const port = Number(process.env.PORT || 4173),
  host = process.env.HOST || "127.0.0.1";
const origin = process.env.WEB_ORIGIN || `http://localhost:${port}`;
const backend = process.env.HONEYCOMB_API_URL || "http://127.0.0.1:8080";
const site = process.env.HONEYCOMB_SITE === "console" ? "console" : "library";
const sessionCookie = `honeycomb_${site}_session`,
  stateCookie = `honeycomb_${site}_login_state`;
const library =
  process.env.LIBRARY_ORIGIN || "https://honeycomb.teamofsilicons.com";
const consoleOrigin =
  process.env.CONSOLE_ORIGIN || "https://console.honeycomb.teamofsilicons.com";
for (const address of [origin, backend, library, consoleOrigin]) {
  const url = new URL(address);
  if (
    url.username ||
    url.password ||
    (!["https:"].includes(url.protocol) &&
      !["localhost", "127.0.0.1", "[::1]"].includes(url.hostname))
  )
    throw new Error("Origins require HTTPS outside loopback");
}
const rawKey = process.env.WEB_SESSION_KEY;
if (production && !rawKey)
  throw new Error("WEB_SESSION_KEY is required in production");
if (rawKey && !/^[a-fA-F0-9]{64}$/.test(rawKey))
  throw new Error("WEB_SESSION_KEY must contain 64 hexadecimal characters");
const key = rawKey ? Buffer.from(rawKey, "hex") : randomBytes(32);
const dbPath = process.env.WEB_SESSION_DB || "data/web-sessions.db";
mkdirSync(dirname(resolve(dbPath)), { recursive: true });
const db = new DatabaseSync(dbPath);
db.exec(
  "PRAGMA journal_mode=WAL; CREATE TABLE IF NOT EXISTS sessions (id TEXT PRIMARY KEY, tokens TEXT NOT NULL, expires INTEGER NOT NULL)",
);
const app = express();
const telemetryContext = new AsyncLocalStorage<boolean>();
app.use((req, _res, next) => telemetryContext.run(req.get("x-honeycomb-telemetry") !== "false" && cookie(req, "honeycomb_telemetry") !== "false", next));
app.disable("x-powered-by");
app.use((req, res, next) => {
  res.setHeader("Cache-Control", "no-store");
  res.setHeader("X-Content-Type-Options", "nosniff");
  res.setHeader("Referrer-Policy", "no-referrer");
  res.setHeader("X-Frame-Options", "DENY");
  if (production)
    res.setHeader(
      "Content-Security-Policy",
      "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data: https:; font-src 'self'; connect-src 'self'; frame-ancestors 'none'; base-uri 'self'; form-action 'self'",
    );
  next();
});
function cookie(req: express.Request, name: string) {
  return req.headers.cookie
    ?.split(";")
    .map((s) => s.trim().split("="))
    .find(([key]) => key === name)?.[1];
}
function setCookie(
  res: express.Response,
  name: string,
  value: string,
  age: number,
  path = "/",
) {
  res.append(
    "Set-Cookie",
    `${name}=${value}; Path=${path}; HttpOnly; SameSite=Lax; Max-Age=${age}${production ? "; Secure" : ""}`,
  );
}
function encrypt(value: unknown) {
  const nonce = randomBytes(12);
  const cipher = createCipheriv("aes-256-gcm", key, nonce);
  const bytes = Buffer.concat([
    cipher.update(JSON.stringify(value), "utf8"),
    cipher.final(),
  ]);
  return Buffer.concat([nonce, cipher.getAuthTag(), bytes]).toString("base64");
}
function decrypt(value: string) {
  const bytes = Buffer.from(value, "base64");
  const decipher = createDecipheriv("aes-256-gcm", key, bytes.subarray(0, 12));
  decipher.setAuthTag(bytes.subarray(12, 28));
  return JSON.parse(
    Buffer.concat([
      decipher.update(bytes.subarray(28)),
      decipher.final(),
    ]).toString(),
  );
}
function session(
  req: express.Request,
): { id: string; tokens: any } | undefined {
  const id = cookie(req, sessionCookie);
  if (!id) return;
  const hash = createHash("sha256").update(id).digest("hex");
  const row = db
    .prepare("SELECT tokens,expires FROM sessions WHERE id=?")
    .get(hash) as { tokens: string; expires: number } | undefined;
  if (!row || row.expires < Date.now()) {
    if (row) db.prepare("DELETE FROM sessions WHERE id=?").run(hash);
    return;
  }
  try {
    return { id: hash, tokens: decrypt(row.tokens) };
  } catch {
    return;
  }
}
async function api(path: string, options: RequestInit = {}) {
  const headers = new Headers(options.headers);
  if (telemetryContext.getStore() === false) headers.set("x-honeycomb-telemetry", "false");
  return fetch(`${backend}/api/v1/${path}`, {
    ...options,
    headers,
    redirect: "error",
    signal: AbortSignal.timeout(300000),
  });
}
// Refresh once per session even when several browser requests arrive together.
const refreshing = new Map<string, Promise<any>>();
async function currentSession(req: express.Request) {
  const active = session(req);
  if (!active) return;
  if (active.tokens.expires_at >= Date.now() + 30000) return active;
  let pending = refreshing.get(active.id);
  if (!pending) {
    pending = (async () => {
      if (!active.tokens.refresh_token) {
        db.prepare("DELETE FROM sessions WHERE id=?").run(active.id);
        return;
      }
      const response = await api("auth/refresh", {
        method: "POST",
        headers: {
          "Content-Type": "application/json",
          "Idempotency-Key": randomBytes(16).toString("hex"),
        },
        body: JSON.stringify({ refresh_token: active.tokens.refresh_token }),
      });
      if ([400, 401, 403].includes(response.status)) {
        db.prepare("DELETE FROM sessions WHERE id=?").run(active.id);
        return;
      }
      if (!response.ok)
        throw new Error("IAM could not renew this session. Please retry.");
      const tokens = (await response.json()) as any;
      if (
        typeof tokens.access_token !== "string" ||
        !tokens.access_token ||
        !Number.isFinite(tokens.expires_in) ||
        tokens.expires_in <= 0
      )
        throw new Error("IAM returned an invalid session response.");
      tokens.refresh_token ||= active.tokens.refresh_token;
      tokens.expires_at = Date.now() + tokens.expires_in * 1000;
      const update = db
        .prepare("UPDATE sessions SET tokens=? WHERE id=?")
        .run(encrypt(tokens), active.id);
      // A concurrent sign-out must never recreate a session.
      if (!update.changes) return;
      return { id: active.id, tokens };
    })();
    refreshing.set(active.id, pending);
  }
  try {
    return await pending;
  } finally {
    if (refreshing.get(active.id) === pending) refreshing.delete(active.id);
  }
}
function fail(res: express.Response, error: unknown) {
  res.status(502).json({
    error: {
      code: "service_unavailable",
      message:
        error instanceof Error
          ? error.message
          : "Honeycomb could not complete this request.",
    },
  });
}
function csrf(
  req: express.Request,
  res: express.Response,
  next: express.NextFunction,
) {
  if (req.headers.origin !== origin) {
    res.status(403).json({
      error: {
        code: "origin_mismatch",
        message: "Reload Honeycomb before trying again.",
      },
    });
    return;
  }
  next();
}
app.get("/api/config", (_req, res) =>
  res.json({ site, libraryOrigin: library, consoleOrigin }),
);
app.get("/api/session", async (req, res) => {
  try {
    const active = await currentSession(req);
    if (!active) {
      res.json({ authenticated: false });
      return;
    }
    const response = await api("auth/status", {
      headers: { Authorization: `Bearer ${active.tokens.access_token}` },
    });
    if (response.status === 401) {
      db.prepare("DELETE FROM sessions WHERE id=?").run(active.id);
      setCookie(res, sessionCookie, "", 0);
      res.json({ authenticated: false });
      return;
    }
    res
      .status(response.status)
      .type("json")
      .send(await response.text());
  } catch (e) {
    fail(res, e);
  }
});
app.get("/auth/login", async (_req, res) => {
  try {
    const response = await api("iam");
    if (!response.ok)
      throw new Error("IAM sign-in is temporarily unavailable.");
    const iam = (await response.json()) as {
      app_id: string;
      login_url: string;
    };
    const state = randomBytes(32).toString("hex");
    setCookie(res, stateCookie, state, 600, "/auth");
    const callback = new URL("/auth/callback", origin);
    callback.searchParams.set("state", state);
    const login = new URL("/login", iam.login_url);
    login.searchParams.set("app_id", iam.app_id);
    login.searchParams.set("redirect_uri", callback.href);
    res.redirect(login.href);
  } catch (e) {
    fail(res, e);
  }
});
app.get("/auth/callback", async (req, res) => {
  try {
    const supplied = typeof req.query.state === "string" ? req.query.state : "";
    const expected = cookie(req, stateCookie) || "";
    const slt = typeof req.query.slt === "string" ? req.query.slt : "";
    if (
      !expected ||
      supplied.length !== expected.length ||
      !timingSafeEqual(Buffer.from(supplied), Buffer.from(expected)) ||
      !slt
    ) {
      res
        .status(400)
        .send(
          "This sign-in expired or was not started here. Return to Honeycomb and sign in again.",
        );
      return;
    }
    setCookie(res, stateCookie, "", 0, "/auth");
    const response = await api("auth/login", {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
        "Idempotency-Key": randomBytes(16).toString("hex"),
      },
      body: JSON.stringify({ slt }),
    });
    if (!response.ok)
      throw new Error(
        "IAM could not exchange this sign-in token. Please start sign-in again.",
      );
    const tokens = (await response.json()) as any;
    if (
      typeof tokens.access_token !== "string" || !tokens.access_token ||
      !Number.isFinite(tokens.expires_in) || tokens.expires_in <= 0
    ) throw new Error("IAM returned an invalid session response.");
    const id = randomBytes(32).toString("hex");
    db.prepare("INSERT INTO sessions(id,tokens,expires) VALUES(?,?,?)").run(
      createHash("sha256").update(id).digest("hex"),
      encrypt({ ...tokens, expires_at: Date.now() + tokens.expires_in * 1000 }),
      Date.now() + 30 * 86400000,
    );
    setCookie(res, sessionCookie, id, 30 * 86400);
    res.redirect("/");
  } catch (e) {
    fail(res, e);
  }
});
app.post("/auth/logout", csrf, async (req, res) => {
  const active = session(req);
  if (active) {
    try {
      const response = await api("auth/logout", {
        method: "POST",
        headers: {
          Authorization: `Bearer ${active.tokens.access_token}`,
          "Content-Type": "application/json",
          "Idempotency-Key": randomBytes(16).toString("hex"),
        },
        body: JSON.stringify({ refresh_token: active.tokens.refresh_token }),
      });
      if (!response.ok && response.status !== 401) {
        res.status(502).json({
          error: {
            message:
              "IAM could not revoke this session. Try signing out again.",
          },
        });
        return;
      }
    } catch (e) {
      fail(res, e);
      return;
    }
    db.prepare("DELETE FROM sessions WHERE id=?").run(active.id);
  }
  setCookie(res, sessionCookie, "", 0);
  res.json({ authenticated: false });
});
app.use(
  "/api/v1",
  (req, res, next) => {
    if (!["GET", "HEAD"].includes(req.method)) return csrf(req, res, next);
    next();
  },
  express.raw({ type: () => true, limit: "512mb" }),
  async (req, res) => {
    if (req.path.startsWith("/auth/")) {
      res.status(404).end();
      return;
    }
    try {
      const active = await currentSession(req);
      const headers = new Headers();
      for (const name of ["content-type", "if-match", "idempotency-key"]) {
        const value = req.get(name);
        if (value) headers.set(name, value);
      }
      if (active)
        headers.set("authorization", `Bearer ${active.tokens.access_token}`);
      const response = await api(req.url.replace(/^\//, ""), {
        method: req.method,
        headers,
        body: ["GET", "HEAD"].includes(req.method) ? undefined : req.body,
      });
      res.status(response.status);
      for (const name of ["content-type", "etag", "x-checksum-sha256"]) {
        const value = response.headers.get(name);
        if (value) res.setHeader(name, value);
      }
      // Streaming avoids holding downloaded archives in the web process.
      if (response.body) {
        const { Readable } = await import("node:stream");
        Readable.fromWeb(response.body as any).pipe(res);
      } else res.end();
    } catch (e) {
      fail(res, e);
    }
  },
);
if (production) {
  app.use(express.static(resolve("dist"), { index: false }));
  app.get("/{*path}", (_req, res) => res.sendFile(resolve("dist/index.html")));
} else {
  const { createServer } = await import("vite");
  const vite = await createServer({
    server: { middlewareMode: true },
    appType: "spa",
  });
  app.use(vite.middlewares);
}
app.listen(port, host, () => console.log(`Honeycomb ${site}: ${origin}`));
