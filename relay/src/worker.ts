/**
 * Relay protocol v2. Mailbox owners authorize narrowly scoped sender credentials.
 * HMAC authenticates transport access, not end-to-end content: the relay knows
 * these credentials. Callers must seal and authenticate payloads before sending.
 * Legacy v1 KV credentials are never read or promoted into v2 permissions.
 */
import { DurableObject } from "cloudflare:workers";

export interface Env {
  MAILBOX: DurableObjectNamespace<Mailbox>;
  MAIL_TTL_MS: string;
  MAILBOX_QUOTA_BYTES: string;
  MAX_ENVELOPE_BYTES: string;
  /** 应用升级分发桶（release 清单与 APK，见 /release/*）。 */
  RELEASES: R2Bucket;
}

const VERSION = 2;
const SKEW_MS = 5 * 60_000;
const NONCE_TTL_MS = 10 * 60_000;
const CONTROL_BYTES = 32 * 1024;
const MAX_ENVELOPE = 8 * 1024 * 1024;
const RESPONSE_BYTES = 16 * 1024 * 1024;
const CHUNK_CHARS = 64 * 1024;
const MAX_SENDERS = 256;
const MAX_MAILS = 10_000;
const MAX_NONCES = 20_000;
const POST_ROUTES = new Set(["/create", "/register", "/revoke", "/rotate", "/send", "/ack"]);
const GET_ROUTES = new Set(["/fetch", "/status", "/sender"]);
type Body = Record<string, unknown>;
type Stamp = { ts: number; nonce: string; sig: string };
type Owner = { cred_hex: string; generation: number };
type Sender = { cred_hex: string | null; expires_at: number; revision: number };
type Mail = { id: number; sender_mb: string; seq_hint: number; bytes: number; created_at: number; expires_at: number };

class HttpError extends Error {
  constructor(readonly status: number, message: string) { super(message); }
}
function require(condition: unknown, status: number, message: string): asserts condition {
  if (!condition) throw new HttpError(status, message);
}
function json(data: unknown, status = 200): Response {
  return Response.json(data, { status, headers: { "cache-control": "no-store" } });
}
function failure(error: unknown): Response {
  return error instanceof HttpError
    ? json({ error: error.message }, error.status)
    : json({ error: "relay internal error" }, 500);
}
function hex(value: unknown, length: number): value is string {
  return typeof value === "string" && new RegExp(`^[0-9a-f]{${length}}$`).test(value);
}
function integer(value: unknown, min = 0, max = Number.MAX_SAFE_INTEGER): value is number {
  return typeof value === "number" && Number.isSafeInteger(value) && value >= min && value <= max;
}
function configured(value: string, fallback: number, maximum: number): number {
  const parsed = Number(value);
  return integer(parsed, 1, maximum) ? parsed : fallback;
}
function envelopeLimit(env: Env): number { return configured(env.MAX_ENVELOPE_BYTES, MAX_ENVELOPE, MAX_ENVELOPE); }
function padded(id: number): string { return String(id).padStart(16, "0"); }
function cursor(value: unknown): number {
  require(typeof value === "string" && /^(?:0|[0-9]{16})$/.test(value), 400, "invalid mail cursor");
  const parsed = Number(value);
  require(integer(parsed), 400, "invalid mail cursor");
  return parsed;
}
function base64Size(value: unknown): number {
  require(typeof value === "string" && value.length > 0 && value.length % 4 === 0, 400, "ciphertext must be base64");
  require(!/[^A-Za-z0-9+/=]/.test(value), 400, "ciphertext must be base64");
  const padding = value.endsWith("==") ? 2 : value.endsWith("=") ? 1 : 0;
  require(value.indexOf("=") === -1 || value.indexOf("=") === value.length - padding, 400, "invalid base64 padding");
  return value.length / 4 * 3 - padding;
}
export function checkFreshness(ts: number, now: number): boolean {
  return integer(ts) && Math.abs(now - ts) <= SKEW_MS;
}
function stamp(body: Body): Stamp {
  require(integer(body.ts) && hex(body.nonce, 32) && hex(body.sig, 64), 400, "invalid request authentication");
  require(checkFreshness(body.ts, Date.now()), 403, "stale timestamp");
  return { ts: body.ts, nonce: body.nonce, sig: body.sig };
}
function signed(domain: string, parts: string[], auth: Stamp): string[] {
  return [`relay-v2-${domain}`, ...parts, String(auth.ts), auth.nonce];
}

/** Bound chunked requests too: Content-Length alone is not an input limit. */
async function boundedBody(request: Request, limit: number): Promise<string> {
  const declared = request.headers.get("content-length");
  if (declared !== null) require(/^\d+$/.test(declared) && Number(declared) <= limit, 413, "request too large");
  const reader = request.body?.getReader();
  require(reader, 400, "body required");
  const chunks: Uint8Array[] = [];
  let size = 0;
  for (;;) {
    const { value, done } = await reader.read();
    if (done) break;
    size += value.byteLength;
    if (size > limit) {
      await reader.cancel();
      throw new HttpError(413, "request too large");
    }
    chunks.push(value);
  }
  const bytes = new Uint8Array(size);
  let offset = 0;
  for (const chunk of chunks) { bytes.set(chunk, offset); offset += chunk.length; }
  try { return new TextDecoder("utf-8", { fatal: true }).decode(bytes); }
  catch { throw new HttpError(400, "invalid utf-8"); }
}
function parsedJson(text: string): Body {
  let value: unknown;
  try { value = JSON.parse(text); } catch { throw new HttpError(400, "invalid json"); }
  require(value !== null && typeof value === "object" && !Array.isArray(value), 400, "json object required");
  return value as Body;
}
function queryBody(url: URL): Body {
  const body: Body = {};
  for (const [key, value] of url.searchParams) {
    require(!(key in body), 400, "duplicate query parameter");
    body[key] = value;
  }
  for (const key of ["version", "ts", "limit"]) {
    if (body[key] !== undefined) {
      require(typeof body[key] === "string" && /^\d+$/.test(body[key] as string), 400, "invalid numeric parameter");
      body[key] = Number(body[key]);
    }
  }
  return body;
}
function validateEnvelope(body: Body): asserts body is Body & { mailbox_key: string } {
  require(body.version === VERSION, 426, "relay protocol v2 required");
  require(hex(body.mailbox_key, 64), 400, "invalid mailbox_key");
  // Legacy callers cannot select a sender record as a recipient credential.
  require(body.recipient_mb === undefined || body.recipient_mb === body.mailbox_key, 403, "mailbox owner required");
}

export default {
  async fetch(request: Request, env: Env): Promise<Response> {
    try {
      const url = new URL(request.url);
      if (url.pathname === "/health" && request.method === "GET") return json({ ok: true, service: "elsewhen-relay", version: VERSION });
      if (url.pathname === "/presence" || url.pathname === "/peers") throw new HttpError(410, "discovery is not supported in relay v2");
      if (url.pathname === "/release/latest" && request.method === "GET") {
        // 升级清单：发布脚本写入 R2 的小 JSON。Etag 即便客户端缓存了，
        // 结合短 edge cache 也能在发布后一分钟内看到新版本。
        const manifest = await env.RELEASES.get("release/manifest.json");
        if (!manifest) throw new HttpError(404, "no release published");
        return new Response(manifest.body, {
          headers: { "content-type": "application/json", "cache-control": "public, max-age=60" },
        });
      }
      if (url.pathname === "/release/apk" && request.method === "GET") {
        // APK 大文件：流式转发 R2 对象，响应体不过 Worker 内存。
        const h = url.searchParams.get("h") ?? "";
        require(/^[0-9a-f]{7,64}$/.test(h), 400, "invalid release hash");
        const obj = await env.RELEASES.get(`apk/${h}.apk`);
        if (!obj) throw new HttpError(404, "release apk not found");
        return new Response(obj.body, {
          headers: {
            "content-type": "application/vnd.android.package-archive",
            "content-length": String(obj.size),
            "cache-control": "public, max-age=31536000, immutable",
          },
        });
      }
      const post = POST_ROUTES.has(url.pathname);
      require(post || GET_ROUTES.has(url.pathname), 404, "not found");
      require(request.method === (post ? "POST" : "GET"), 405, "method not allowed");
      require(url.href.length <= CONTROL_BYTES, 414, "request URL too large");
      let body: Body;
      let text: string | undefined;
      if (post) {
        require((request.headers.get("content-type") ?? "").split(";")[0].trim() === "application/json", 415, "application/json required");
        const limit = url.pathname === "/send" ? Math.ceil(envelopeLimit(env) / 3) * 4 + CONTROL_BYTES : CONTROL_BYTES;
        text = await boundedBody(request, limit);
        body = parsedJson(text);
      } else { body = queryBody(url); }
      validateEnvelope(body);
      // SQLite-backed DO 仅在首次创建时受 locationHint 影响：把信箱钉在
      // 亚太（apac），避免跨洋往返造成的多秒级长尾延迟。
      const stub = env.MAILBOX.getByName(body.mailbox_key, { locationHint: "apac" });
      return stub.fetch(post
        ? new Request(new URL(url.pathname, url.origin), { method: "POST", headers: { "content-type": "application/json" }, body: text })
        : new Request(url));
    } catch (error) { return failure(error); }
  },
};

/** SQLite transactions recheck credentials after asynchronous signature verification. */
export class Mailbox extends DurableObject<Env> {
  constructor(ctx: DurableObjectState, env: Env) {
    super(ctx, env);
    ctx.storage.sql.exec(`
      CREATE TABLE IF NOT EXISTS relay_v2_owner (singleton INTEGER PRIMARY KEY CHECK(singleton=1), mailbox_key TEXT NOT NULL, cred_hex TEXT NOT NULL, generation INTEGER NOT NULL);
      CREATE TABLE IF NOT EXISTS relay_v2_senders (sender_mb TEXT PRIMARY KEY, cred_hex TEXT, expires_at INTEGER NOT NULL, revision INTEGER NOT NULL);
      CREATE TABLE IF NOT EXISTS relay_v2_nonces (nonce TEXT PRIMARY KEY, expires_at INTEGER NOT NULL);
      CREATE INDEX IF NOT EXISTS relay_v2_nonce_expiry ON relay_v2_nonces(expires_at);
      CREATE TABLE IF NOT EXISTS relay_v2_mail (id INTEGER PRIMARY KEY AUTOINCREMENT, sender_mb TEXT NOT NULL, seq_hint INTEGER NOT NULL, bytes INTEGER NOT NULL, created_at INTEGER NOT NULL, expires_at INTEGER NOT NULL);
      CREATE INDEX IF NOT EXISTS relay_v2_mail_expiry ON relay_v2_mail(expires_at);
      CREATE TABLE IF NOT EXISTS relay_v2_chunks (mail_id INTEGER NOT NULL, part INTEGER NOT NULL, ciphertext TEXT NOT NULL, PRIMARY KEY(mail_id,part));
    `);
  }

  private owner(mailbox: string): Owner {
    const rows = this.ctx.storage.sql.exec<Owner & { mailbox_key: string }>("SELECT * FROM relay_v2_owner WHERE singleton=1").toArray();
    require(rows.length === 1 && rows[0].mailbox_key === mailbox, 403, "mailbox owner not registered");
    return rows[0];
  }
  private sender(sender: string): Sender | undefined {
    return this.ctx.storage.sql.exec<Sender>("SELECT cred_hex,expires_at,revision FROM relay_v2_senders WHERE sender_mb=?", sender).toArray()[0];
  }
  private clean(now: number): void {
    this.ctx.storage.sql.exec("DELETE FROM relay_v2_chunks WHERE mail_id IN (SELECT id FROM relay_v2_mail WHERE expires_at<=?)", now);
    this.ctx.storage.sql.exec("DELETE FROM relay_v2_mail WHERE expires_at<=?", now);
    this.ctx.storage.sql.exec("DELETE FROM relay_v2_nonces WHERE expires_at<=?", now);
  }
  private claim(auth: Stamp, now: number): void {
    require(checkFreshness(auth.ts, now), 403, "stale timestamp");
    require(this.ctx.storage.sql.exec("SELECT 1 FROM relay_v2_nonces WHERE nonce=?", auth.nonce).toArray().length === 0, 409, "nonce replay");
    const count = this.ctx.storage.sql.exec<{ n: number }>("SELECT COUNT(*) AS n FROM relay_v2_nonces").one().n;
    require(count < MAX_NONCES, 429, "mailbox request window full");
    this.ctx.storage.sql.exec("INSERT INTO relay_v2_nonces VALUES (?,?)", auth.nonce, now + NONCE_TTL_MS);
  }
  private async ownerOperation<T>(body: Body & { mailbox_key: string }, domain: string, parts: string[], apply: (now: number, owner: Owner) => T): Promise<T> {
    const auth = stamp(body);
    const owner = this.owner(body.mailbox_key);
    require(await verifyHmac(owner.cred_hex, signed(domain, parts, auth), auth.sig), 403, "bad owner signature");
    return this.ctx.storage.transactionSync(() => {
      const now = Date.now();
      require(this.owner(body.mailbox_key).generation === owner.generation, 409, "owner credential changed");
      this.clean(now);
      this.claim(auth, now);
      return apply(now, owner);
    });
  }

  async fetch(request: Request): Promise<Response> {
    try {
      const url = new URL(request.url);
      const post = POST_ROUTES.has(url.pathname);
      require(post || GET_ROUTES.has(url.pathname), 404, "not found");
      require(request.method === (post ? "POST" : "GET"), 405, "method not allowed");
      const body = post ? parsedJson(await boundedBody(request, url.pathname === "/send" ? Math.ceil(envelopeLimit(this.env) / 3) * 4 + CONTROL_BYTES : CONTROL_BYTES)) : queryBody(url);
      validateEnvelope(body);
      switch (url.pathname) {
        case "/create": return json(await this.create(body));
        case "/register": return json(await this.register(body));
        case "/revoke": return json(await this.revoke(body));
        case "/rotate": return json(await this.rotate(body));
        case "/sender": return json(await this.getSender(body));
        case "/send": return json(await this.send(body));
        case "/fetch": return json(await this.receive(body));
        case "/ack": return json(await this.ack(body));
        case "/status": return json(await this.status(body));
      }
      throw new HttpError(404, "not found");
    } catch (error) { return failure(error); }
  }

  private async create(body: Body & { mailbox_key: string }) {
    const auth = stamp(body);
    require(hex(body.owner_cred_hex, 64), 400, "invalid owner credential");
    const credential = body.owner_cred_hex;
    require(await verifyHmac(credential, ["relay-mailbox"], body.mailbox_key), 403, "owner credential does not bind mailbox");
    require(await verifyHmac(credential, signed("create", [body.mailbox_key], auth), auth.sig), 403, "bad owner proof");
    return this.ctx.storage.transactionSync(() => {
      const old = this.ctx.storage.sql.exec<Owner>("SELECT cred_hex,generation FROM relay_v2_owner WHERE singleton=1").toArray()[0];
      require(!old || (old.generation === 1 && old.cred_hex === credential), 409, "mailbox already initialized");
      const now = Date.now();
      this.clean(now);
      this.claim(auth, now);
      if (!old) this.ctx.storage.sql.exec("INSERT INTO relay_v2_owner VALUES (1,?,?,1)", body.mailbox_key, credential);
      return { ok: true, version: VERSION, owner_generation: 1 };
    });
  }

  private async register(body: Body & { mailbox_key: string }) {
    require(hex(body.sender_mb, 64) && body.sender_mb !== body.mailbox_key && hex(body.cred_hex, 64), 400, "invalid sender credential");
    require(integer(body.expires_at, 1) && integer(body.expected_revision), 400, "invalid grant version or expiry");
    const sender = body.sender_mb, credential = body.cred_hex, expiry = body.expires_at, expected = body.expected_revision;
    return this.ownerOperation(body, "register", [body.mailbox_key, sender, credential, String(expiry), String(expected)], (now, owner) => {
      require(expiry > now && expiry <= now + 365 * 24 * 60 * 60_000, 400, "grant expiry must be within one year");
      require(credential !== owner.cred_hex, 400, "sender and owner credentials must differ");
      const old = this.sender(sender);
      // A fresh request nonce may retry a committed grant after a lost response.
      // Only the exact post-state at expected+1 is a receipt; never adopt a newer
      // revision or revive a revoked/changed grant by silently refreshing its CAS.
      if (old?.revision === expected + 1 && old.cred_hex === credential && old.expires_at === expiry) {
        return { ok: true, revision: old.revision };
      }
      require((old?.revision ?? 0) === expected, 409, "sender revision changed");
      if (!old) require(this.ctx.storage.sql.exec<{ n: number }>("SELECT COUNT(*) AS n FROM relay_v2_senders").one().n < MAX_SENDERS, 409, "sender limit reached");
      const revision = expected + 1;
      require(integer(revision, 1), 409, "sender revision exhausted");
      this.ctx.storage.sql.exec("INSERT INTO relay_v2_senders VALUES (?,?,?,?) ON CONFLICT(sender_mb) DO UPDATE SET cred_hex=excluded.cred_hex,expires_at=excluded.expires_at,revision=excluded.revision", sender, credential, expiry, revision);
      return { ok: true, revision };
    });
  }
  private async revoke(body: Body & { mailbox_key: string }) {
    require(hex(body.sender_mb, 64) && integer(body.expected_revision, 1), 400, "invalid sender revision");
    const sender = body.sender_mb, expected = body.expected_revision;
    return this.ownerOperation(body, "revoke", [body.mailbox_key, sender, String(expected)], () => {
      const old = this.sender(sender);
      if (old?.revision === expected + 1 && old.cred_hex === null && old.expires_at === 0) {
        return { ok: true, revision: old.revision };
      }
      require(old?.revision === expected, 409, "sender revision changed");
      require(integer(expected + 1, 1), 409, "sender revision exhausted");
      this.ctx.storage.sql.exec("UPDATE relay_v2_senders SET cred_hex=NULL,expires_at=0,revision=? WHERE sender_mb=?", expected + 1, sender);
      return { ok: true, revision: expected + 1 };
    });
  }
  private async rotate(body: Body & { mailbox_key: string }) {
    require(hex(body.new_cred_hex, 64), 400, "invalid new owner credential");
    const credential = body.new_cred_hex;
    return this.ownerOperation(body, "rotate", [body.mailbox_key, credential], (_, owner) => {
      require(credential !== owner.cred_hex, 400, "new owner credential must differ");
      require(integer(owner.generation + 1, 1), 409, "owner generation exhausted");
      require(this.ctx.storage.sql.exec("SELECT 1 FROM relay_v2_senders WHERE cred_hex=?", credential).toArray().length === 0, 400, "owner credential cannot be a sender credential");
      this.ctx.storage.sql.exec("UPDATE relay_v2_owner SET cred_hex=?,generation=? WHERE singleton=1", credential, owner.generation + 1);
      return { ok: true, owner_generation: owner.generation + 1 };
    });
  }
  private async getSender(body: Body & { mailbox_key: string }) {
    require(hex(body.sender_mb, 64), 400, "invalid sender_mb");
    const sender = body.sender_mb;
    return this.ownerOperation(body, "sender", [body.mailbox_key, sender], (now) => {
      const found = this.sender(sender);
      return { revision: found?.revision ?? 0, active: !!found?.cred_hex && found.expires_at > now, expires_at: found?.expires_at ?? 0 };
    });
  }

  private async send(body: Body & { mailbox_key: string }) {
    require(hex(body.sender_mb, 64) && integer(body.seq_hint) && integer(body.expires_at, 1), 400, "invalid envelope metadata");
    const auth = stamp(body);
    const size = base64Size(body.ciphertext);
    require(size <= envelopeLimit(this.env), 413, "envelope too large");
    const ciphertext = body.ciphertext as string, sender = body.sender_mb, seq = body.seq_hint, expiry = body.expires_at;
    this.owner(body.mailbox_key);
    const credential = this.sender(sender);
    require(credential?.cred_hex && credential.expires_at > Date.now(), 403, "sender not authorized");
    const hash = await sha256Hex(ciphertext);
    // Authenticate exactly the requested expiry, then clamp it after verification.
    require(await verifyHmac(credential.cred_hex, [...signed("send", [sender, body.mailbox_key, String(seq), String(expiry)], auth), hash], auth.sig), 403, "bad sender signature");
    return this.ctx.storage.transactionSync(() => {
      const now = Date.now(), current = this.sender(sender);
      require(current?.cred_hex && current.revision === credential.revision && current.expires_at > now, 403, "sender authorization changed");
      require(expiry > now, 400, "envelope expiry must be in the future");
      this.clean(now);
      this.claim(auth, now);
      const used = this.ctx.storage.sql.exec<{ bytes: number; n: number }>("SELECT COALESCE(SUM(bytes),0) AS bytes,COUNT(*) AS n FROM relay_v2_mail").one();
      const quota = configured(this.env.MAILBOX_QUOTA_BYTES, 50 * 1024 * 1024, 512 * 1024 * 1024);
      require(used.n < MAX_MAILS && used.bytes + size <= quota, 507, "mailbox quota exceeded");
      const ttl = configured(this.env.MAIL_TTL_MS, 7 * 24 * 60 * 60_000, 30 * 24 * 60 * 60_000);
      const id = this.ctx.storage.sql.exec<{ id: number }>("INSERT INTO relay_v2_mail(sender_mb,seq_hint,bytes,created_at,expires_at) VALUES (?,?,?,?,?) RETURNING id", sender, seq, size, now, Math.min(expiry, now + ttl)).one().id;
      require(integer(id, 1), 507, "mailbox sequence exhausted");
      for (let offset = 0, part = 0; offset < ciphertext.length; offset += CHUNK_CHARS, part++) {
        this.ctx.storage.sql.exec("INSERT INTO relay_v2_chunks VALUES (?,?,?)", id, part, ciphertext.slice(offset, offset + CHUNK_CHARS));
      }
      return { ok: true, id: padded(id) };
    });
  }
  private async receive(body: Body & { mailbox_key: string }) {
    const afterText = body.after ?? "0", after = cursor(afterText), limit = body.limit ?? 20;
    require(integer(limit, 1, 100), 400, "limit must be 1..100");
    return this.ownerOperation(body, "fetch", [body.mailbox_key, String(afterText), String(limit)], () => {
      const rows = this.ctx.storage.sql.exec<Mail>("SELECT * FROM relay_v2_mail WHERE id>? ORDER BY id LIMIT ?", after, limit + 1).toArray();
      const envelopes = [];
      let total = 0;
      for (const row of rows.slice(0, limit)) {
        const encoded = Math.ceil(row.bytes / 3) * 4;
        if (total + encoded + 1024 > RESPONSE_BYTES) break;
        const ciphertext = this.ctx.storage.sql.exec<{ ciphertext: string }>("SELECT ciphertext FROM relay_v2_chunks WHERE mail_id=? ORDER BY part", row.id).toArray().map((c) => c.ciphertext).join("");
        envelopes.push({ id: padded(row.id), sender_mb: row.sender_mb, seq_hint: row.seq_hint, ciphertext, created_at: row.created_at, expires_at: row.expires_at });
        total += encoded + 1024;
      }
      return { ok: true, envelopes, next_cursor: envelopes.at(-1)?.id ?? String(afterText), has_more: envelopes.length < rows.length };
    });
  }
  private async ack(body: Body & { mailbox_key: string }) {
    require(Array.isArray(body.ids) && body.ids.length >= 1 && body.ids.length <= 100, 400, "ids must contain 1..100 mail ids");
    const ids = body.ids as unknown[];
    const parsed = ids.map((id) => { const n = cursor(id); require(n > 0, 400, "invalid mail id"); return n; });
    require(new Set(parsed).size === parsed.length, 400, "duplicate mail ids");
    return this.ownerOperation(body, "ack", [body.mailbox_key, ids.join(",")], () => {
      let deleted = 0;
      for (const id of parsed) {
        this.ctx.storage.sql.exec("DELETE FROM relay_v2_chunks WHERE mail_id=?", id);
        deleted += this.ctx.storage.sql.exec("DELETE FROM relay_v2_mail WHERE id=? RETURNING id", id).toArray().length;
      }
      return { ok: true, deleted };
    });
  }
  private async status(body: Body & { mailbox_key: string }) {
    return this.ownerOperation(body, "status", [body.mailbox_key], () => ({
      ok: true,
      ...this.ctx.storage.sql.exec<{ bytes: number; count: number; earliest_expiry: number | null }>("SELECT COALESCE(SUM(bytes),0) AS bytes,COUNT(*) AS count,MIN(expires_at) AS earliest_expiry FROM relay_v2_mail").one(),
      quota: configured(this.env.MAILBOX_QUOTA_BYTES, 50 * 1024 * 1024, 512 * 1024 * 1024),
    }));
  }
}

function hexToBytes(value: string): Uint8Array {
  return Uint8Array.from(value.match(/../g)!, (byte) => parseInt(byte, 16));
}
async function hmacKey(value: string, usage: KeyUsage): Promise<CryptoKey> {
  return crypto.subtle.importKey("raw", hexToBytes(value), { name: "HMAC", hash: "SHA-256" }, false, [usage]);
}
export async function verifyHmac(key: string, parts: string[], signature: string): Promise<boolean> {
  if (!hex(key, 64) || !hex(signature, 64)) return false;
  return crypto.subtle.verify("HMAC", await hmacKey(key, "verify"), hexToBytes(signature), new TextEncoder().encode(parts.join("|")));
}
export async function hmacHex(key: string, parts: string[]): Promise<string> {
  const result = await crypto.subtle.sign("HMAC", await hmacKey(key, "sign"), new TextEncoder().encode(parts.join("|")));
  return bytesToHex(new Uint8Array(result));
}
export async function sha256Hex(value: string): Promise<string> {
  return bytesToHex(new Uint8Array(await crypto.subtle.digest("SHA-256", new TextEncoder().encode(value))));
}
function bytesToHex(value: Uint8Array): string {
  return [...value].map((byte) => byte.toString(16).padStart(2, "0")).join("");
}
