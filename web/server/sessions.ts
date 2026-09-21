import { DatabaseSync } from "node:sqlite";
import { randomBytes, createCipheriv, createDecipheriv } from "node:crypto";

type Tokens = {
  access_token: string;
  refresh_token?: string;
  expires_at: number;
  pending_refresh?: { key: string; started_at: number };
  [key: string]: unknown;
};
type Session = { id: string; tokens: Tokens; expires: number; serialized: string };
type Refresh = (token: string, key: string) => Promise<Response>;

// Both website processes keep their existing encrypted database and key across deploys.
// A saved refresh operation also survives a process exit after IAM rotates its token.
export class Sessions {
  private refreshing = new Map<string, Promise<Session | undefined>>();
  constructor(private db: DatabaseSync, private key: Buffer, private refresh: Refresh) {
    db.exec("PRAGMA journal_mode=WAL; PRAGMA busy_timeout=5000; CREATE TABLE IF NOT EXISTS sessions (id TEXT PRIMARY KEY, tokens TEXT NOT NULL, expires INTEGER NOT NULL)");
  }
  private encrypt(tokens: Tokens) {
    const nonce = randomBytes(12);
    const cipher = createCipheriv("aes-256-gcm", this.key, nonce);
    const bytes = Buffer.concat([cipher.update(JSON.stringify(tokens), "utf8"), cipher.final()]);
    return Buffer.concat([nonce, cipher.getAuthTag(), bytes]).toString("base64");
  }
  private decrypt(value: string): Tokens {
    const bytes = Buffer.from(value, "base64");
    const decipher = createDecipheriv("aes-256-gcm", this.key, bytes.subarray(0, 12));
    decipher.setAuthTag(bytes.subarray(12, 28));
    return JSON.parse(Buffer.concat([decipher.update(bytes.subarray(28)), decipher.final()]).toString());
  }
  load(id: string): Session | undefined {
    const row = this.db.prepare("SELECT tokens,expires FROM sessions WHERE id=?").get(id) as { tokens: string; expires: number } | undefined;
    if (!row) return;
    if (row.expires <= Date.now()) { this.remove(id); return; }
    try { return { id, tokens: this.decrypt(row.tokens), expires: row.expires, serialized: row.tokens }; }
    catch { throw new Error("The saved session could not be read. Retry after the session service is restored."); }
  }
  remove(id: string) { this.db.prepare("DELETE FROM sessions WHERE id=?").run(id); }
  create(id: string, body: unknown, startedAt: number, expires: number) {
    const tokens = this.tokens(body, startedAt);
    this.db.prepare("INSERT INTO sessions(id,tokens,expires) VALUES(?,?,?)").run(id, this.encrypt(tokens), expires);
  }
  private tokens(body: unknown, startedAt: number): Tokens {
    const value = body as Record<string, unknown>;
    if (!value || typeof value.access_token !== "string" || !value.access_token ||
        typeof value.expires_in !== "number" || !Number.isSafeInteger(value.expires_in) || value.expires_in <= 0 ||
        !Number.isSafeInteger(startedAt + value.expires_in * 1000))
      throw new Error("IAM returned an invalid session response. Retry to recover the saved session.");
    if (typeof value.refresh_token !== "string" || !value.refresh_token)
      throw new Error("IAM returned no refresh token. Retry to recover the saved session.");
    const refresh = value.refresh_token;
    return { ...value, access_token: value.access_token, refresh_token: refresh, expires_at: startedAt + value.expires_in * 1000, pending_refresh: undefined };
  }
  async current(id: string, rejectedToken?: string): Promise<Session | undefined> {
    let pending = this.refreshing.get(id);
    if (!pending) {
      pending = this.renew(id, rejectedToken);
      this.refreshing.set(id, pending);
    }
    try { return await pending; }
    finally { if (this.refreshing.get(id) === pending) this.refreshing.delete(id); }
  }
  private async renew(id: string, rejectedToken?: string): Promise<Session | undefined> {
    for (let pass = 0; pass < 2; pass++) {
      // Only the short read/update transaction is locked; network IO happens afterwards.
      this.db.exec("BEGIN IMMEDIATE");
      let active: Session | undefined;
      try {
        active = this.load(id);
        if (!active) { this.db.exec("COMMIT"); return; }
        if (!active.tokens.pending_refresh && active.tokens.expires_at > Date.now() + 30000 && active.tokens.access_token !== rejectedToken) {
          this.db.exec("COMMIT"); return active;
        }
        if (!active.tokens.refresh_token) {
          this.remove(id); this.db.exec("COMMIT"); return;
        }
        if (!active.tokens.pending_refresh) {
          active.tokens.pending_refresh = { key: randomBytes(16).toString("hex"), started_at: Date.now() };
          active.serialized = this.encrypt(active.tokens);
          this.db.prepare("UPDATE sessions SET tokens=? WHERE id=?").run(active.serialized, id);
        }
        this.db.exec("COMMIT");
      } catch (e) { this.db.exec("ROLLBACK"); throw e; }
      const attempt = active.tokens.pending_refresh!;
      const response = await this.refresh(active.tokens.refresh_token!, attempt.key);
      if (!response.ok) {
        // A late response must not delete a successor saved by another process.
        const latest = this.load(id);
        if (!latest || latest.serialized !== active.serialized) return latest;
        const body = await response.json().catch(() => undefined) as any;
        // App credential outages, rate limits and generic 400/401/403 errors do not
        // establish that the user's refresh family was revoked.
        if ([400, 401].includes(response.status) && body?.error?.code === "invalid_grant") {
          this.db.prepare("DELETE FROM sessions WHERE id=? AND tokens=?").run(id, active.serialized);
          return this.load(id);
        }
        throw new Error("IAM could not renew this session. Please retry.");
      }
      const tokens = this.tokens(await response.json(), attempt.started_at);
      // CAS prevents an older response overwriting a later rotation or recreating logout.
      this.db.prepare("UPDATE sessions SET tokens=? WHERE id=? AND tokens=?")
        .run(this.encrypt(tokens), id, active.serialized);
      const latest = this.load(id);
      if (!latest || latest.tokens.expires_at > Date.now()) return latest;
      rejectedToken = undefined;
      // A response replayed after a long outage can already be expired. Its successor
      // refresh token is safely saved, so one additional rotation can recover it.
    }
    throw new Error("IAM returned an expired access token. Retry to renew the saved session.");
  }
}
