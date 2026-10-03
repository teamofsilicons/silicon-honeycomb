import { createHmac, timingSafeEqual } from "node:crypto";
import { loginReturnPath } from "./login-return.ts";

export type IdentityKind = "carbon" | "silicon";
export type LoginContext = { state: string; kind?: IdentityKind; next: string; nonce?: string; expires: number };
export function identityKind(value: unknown): IdentityKind | undefined {
  return value === "carbon" || value === "silicon" ? value : undefined;
}
export function signLoginContext(context: LoginContext, key: Buffer): string {
  const payload = Buffer.from(JSON.stringify(context)).toString("base64url");
  return `${payload}.${createHmac("sha256", key).update(payload).digest("base64url")}`;
}
export function readLoginContext(value: string, state: string, key: Buffer): LoginContext | undefined {
  const [payload, signature, extra] = value.split(".");
  if (!payload || !signature || extra) return;
  const expected = createHmac("sha256", key).update(payload).digest();
  const supplied = Buffer.from(signature, "base64url");
  if (supplied.length !== expected.length || !timingSafeEqual(supplied, expected)) return;
  try {
    const context = JSON.parse(Buffer.from(payload, "base64url").toString()) as LoginContext;
    if (context.state !== state || !Number.isFinite(context.expires) || context.expires < Date.now()
      || (context.kind !== undefined && !identityKind(context.kind))
      || (context.nonce !== undefined && !/^[a-f0-9-]{36}$/.test(context.nonce))) return;
    return { ...context, next: loginReturnPath(context.next) };
  } catch { return; }
}
