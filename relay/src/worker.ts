/**
 * Elsewhen 盲中继（blind relay）
 *
 * 设计文档：docs/notes/proposed/architecture/2026-09-30-multi-device-sync.md §6.3
 *
 * 威胁模型：中继是不可信传输介质（与网盘同级）。
 *  - 信封验签：HMAC-SHA256(relay_cred, ...) —— 中继只能验签，不能解密
 *  - 载荷：端到端密文 blob —— 中继看不到内容、表名、seq
 *  - 元数据：中继可见 sender/recipient 的 device_id 哈希、流量大小与时间
 *
 * relay_cred = HMAC(shared_key, "relay-auth")（绑定 §0.3 时双方各自派生）
 * mailbox_key = HMAC(relay_cred, "relay-mailbox") 的 hex —— 中继不存明文 device_id
 *
 * 注册凭证（relay-auth 申请）：
 *   POST /register  { sender_mb, recipient_mb, cred_hex, expires_at, sig }
 *   仅当本地无凭证时接受（防止陌生人覆盖受害者凭证）。
 *   凭证的注入走带外通道（扫码二维码内含 writer 的 cred），见文档 §0.3。
 *
 * HTTP 接口（v1，轮询式；WebSocket 升级留作后续）：
 *   POST   /send        上传加密信封到收件人邮箱（需 sender 签名）
 *   GET    /fetch?after=<cursor>&limit=<n>  取件（需收件人签名）
 *   POST   /ack         确认已处理，删除（需收件人签名）
 *   GET    /status      邮箱状态（字节数/条数/最早过期时间，需收件人签名）
 *   POST   /presence    心跳：上报本设备在线状态 + 端到端加密的端点信息（供发现）
 *   GET    /peers       发现：列出本设备可见的在线对端（含加密端点，中继不可读）
 *   GET    /health      存活探测（无鉴权）
 *
 * 签名规范（HMAC-SHA256，hex 小写）：
 *   send:  HMAC(cred, "relay-send" | sender_mb | recipient_mb | seq_hint |
 *                expires_at_ms | ts | nonce | SHA256(ciphertext))
 *   fetch: HMAC(cred, "relay-fetch" | recipient_mb | after | limit | ts | nonce)
 *   ack:   HMAC(cred, "relay-ack" | recipient_mb | id | ts | nonce)
 *   status:HMAC(cred, "relay-status" | recipient_mb | ts | nonce)
 *   register: HMAC(cred, "relay-register" | sender_mb | recipient_mb | expires_at)
 *   ts 与服务器时间差超过 5 分钟拒绝；nonce 在 TTL 内重放拒绝。
 */

export interface Env {
  MAILBOX: DurableObjectNamespace;
  MAIL_TTL_MS: string;
  MAILBOX_QUOTA_BYTES: string;
  MAX_ENVELOPE_BYTES: string;
}

const SKEW_MS = 5 * 60 * 1000; // 签名时间戳容差 ±5 分钟
const NONCE_TTL_MS = 10 * 60 * 1000; // nonce 防重放窗口 10 分钟

export default {
  async fetch(request: Request, env: Env): Promise<Response> {
    const url = new URL(request.url);

    if (url.pathname === "/health") {
      return json({ ok: true, service: "elsewhen-relay", version: 1 });
    }

    // 所有邮箱操作路由到对应 Mailbox Durable Object。
    // mailbox_key 取自 body 或 query，DO id 由它派生 —— 每设备一个实例。
    let mailboxKey: string | null = null;
    if (request.method === "POST") {
      const contentType = request.headers.get("content-type") ?? "";
      if (!contentType.includes("application/json")) {
        return err(415, "content-type must be application/json");
      }
      // 注意：body 只能读一次，这里读完转发给 DO
      const body = await request.text();
      try {
        const key = (JSON.parse(body) as { mailbox_key?: unknown }).mailbox_key;
        mailboxKey = typeof key === "string" ? key : null;
      } catch {
        return err(400, "invalid json body");
      }
      if (!mailboxKey || !/^[0-9a-f]{64}$/.test(mailboxKey)) {
        return err(400, "mailbox_key must be 64-char lowercase hex");
      }
      const id = env.MAILBOX.idFromName(mailboxKey);
      const stub = env.MAILBOX.get(id);
      return stub.fetch(new Request(new URL(url.pathname, url.origin), {
        method: "POST",
        headers: { "content-type": "application/json" },
        body,
      }));
    }

    if (request.method === "GET" &&
        (url.pathname === "/fetch" || url.pathname === "/status" || url.pathname === "/peers")) {
      mailboxKey = url.searchParams.get("mailbox_key");
      if (!mailboxKey || !/^[0-9a-f]{64}$/.test(mailboxKey)) {
        return err(400, "mailbox_key must be 64-char lowercase hex");
      }
      const id = env.MAILBOX.idFromName(mailboxKey);
      const stub = env.MAILBOX.get(id);
      return stub.fetch(new Request(url, request));
    }

    return err(404, "not found");
  },
};

/** 校验请求签名时间戳在容差窗口内。 */
export function checkFreshness(ts: number, now: number): boolean {
  return Number.isFinite(ts) && Math.abs(now - ts) <= SKEW_MS;
}

function json(data: unknown, status = 200): Response {
  return new Response(JSON.stringify(data), {
    status,
    headers: { "content-type": "application/json" },
  });
}

function err(status: number, message: string): Response {
  return json({ error: message }, status);
}

// ---------------------------------------------------------------------------
// Mailbox Durable Object：单设备邮箱
// ---------------------------------------------------------------------------

interface StoredEnvelope {
  id: string;            // DO 内单调递增序号（取件游标）
  sender_mb: string;     // 发件人 mailbox_key（hex）
  seq_hint: number;      // 发件方本地序号提示（乱序/丢包检测用；密文外的最小元数据）
  ciphertext: string;    // base64 端到端密文 blob（中继不可读）
  size: number;          // 密文字节数（配额计账）
  created_at: number;    // ms epoch
  expires_at: number;    // ms epoch
}

interface CredRecord {
  cred_hex: string;      // 验签凭证（relay_cred），只能验签不能解密
  expires_at: number;    // 凭证过期时间
}

interface PresenceRecord {
  endpoint_blob: string; // 端到端加密的端点信息（addr/port/device_id 等），中继不可读
  expires_at: number;    // 心跳过期时间（TTL，通常 2 分钟）
  updated_at: number;    // 最近一次心跳
}

export class Mailbox {
  private state: DurableObjectState;
  private env: Env;

  constructor(state: DurableObjectState, env: Env) {
    this.state = state;
    this.env = env;
  }

  async fetch(request: Request): Promise<Response> {
    const url = new URL(request.url);
    const now = Date.now();

    // 惰性过期清理（每次请求时顺手清，无需 alarm 常驻）
    await this.sweepExpired(now);

    // /peers 是 GET，其余发现相关是 POST，路由在 fetch 入口已按 mailbox_key 分到本 DO

    try {
      switch (url.pathname) {
        case "/register": return await this.handleRegister(request, now);
        case "/send": return await this.handleSend(request, now);
        case "/fetch": return await this.handleFetch(url, now);
        case "/ack": return await this.handleAck(request, now);
        case "/status": return await this.handleStatus(url, now);
        case "/presence": return await this.handlePresence(request, now);
        case "/peers": return await this.handlePeers(url, now);
        default: return err(404, "unknown mailbox route");
      }
    } catch (e) {
      return err(500, `internal: ${(e as Error).message}`);
    }
  }

  // -- 凭证管理 ---------------------------------------------------------------

  /** 读取 (sender_mb → cred) 凭证记录。 */
  private async getCred(senderMb: string): Promise<CredRecord | null> {
    const rec = await this.state.storage.get<CredRecord>(`cred:${senderMb}`);
    if (!rec) return null;
    if (rec.expires_at <= Date.now()) return null;
    return rec;
  }

  /**
   * 注册验签凭证：仅当 (sender,recipient) 对尚无有效凭证时接受。
   * 这保证陌生人无法覆盖受害者凭证（凭证的首次注入走扫码带外通道）。
   */
  private async handleRegister(request: Request, now: number): Promise<Response> {
    const b = await request.json() as {
      mailbox_key: string; sender_mb: string; cred_hex: string;
      expires_at: number; sig: string;
    };
    if (!isHex(b.sender_mb, 64) || !isHex(b.cred_hex, 64) || !isHex(b.sig, 64)) {
      return err(400, "sender_mb, cred_hex, sig must be 64-char hex");
    }
    if (!Number.isFinite(b.expires_at) || b.expires_at <= now) {
      return err(400, "expires_at must be a future ms timestamp");
    }

    const existing = await this.getCred(b.sender_mb);
    if (existing) {
      return err(409, "credential already registered for this sender; rotate out-of-band");
    }

    // 自证签名：HMAC(申报的 cred, "relay-register"|sender|recipient|expires)
    const expect = await hmacHex(b.cred_hex,
      ["relay-register", b.sender_mb, b.mailbox_key, String(b.expires_at)]);
    if (expect !== b.sig) {
      return err(403, "bad register signature");
    }

    await this.state.storage.put(`cred:${b.sender_mb}`, {
      cred_hex: b.cred_hex, expires_at: b.expires_at,
    } satisfies CredRecord);
    return json({ ok: true });
  }

  // -- 收发 -------------------------------------------------------------------

  private async handleSend(request: Request, now: number): Promise<Response> {
    const b = await request.json() as {
      mailbox_key: string; sender_mb: string; seq_hint: number;
      ciphertext: string; expires_at: number; ts: number; nonce: string; sig: string;
    };

    if (!isHex(b.sender_mb, 64) || !isHex(b.sig, 64) || !isHex(b.nonce, 32)) {
      return err(400, "sender_mb/sig 64-hex, nonce 32-hex required");
    }
    if (!checkFreshness(b.ts, now)) return err(403, "stale timestamp");
    if (typeof b.ciphertext !== "string" || b.ciphertext.length === 0) {
      return err(400, "ciphertext required (base64)");
    }
    const size = Math.floor(b.ciphertext.length * 3 / 4); // base64 → 字节估计
    const maxEnv = Number(this.env.MAX_ENVELOPE_BYTES);
    if (size > maxEnv) return err(413, `envelope exceeds ${maxEnv} bytes`);

    const ttl = Number(this.env.MAIL_TTL_MS);
    const exp = Math.min(
      Number.isFinite(b.expires_at) ? b.expires_at : now + ttl,
      now + ttl,
    );

    const cred = await this.getCred(b.sender_mb);
    if (!cred) return err(403, "no credential for sender; register first (out-of-band)");

    // 验签
    const payloadHash = await sha256Hex(b.ciphertext);
    const expect = await hmacHex(cred.cred_hex, [
      "relay-send", b.sender_mb, b.mailbox_key, String(b.seq_hint ?? 0),
      String(exp), String(b.ts), b.nonce, payloadHash,
    ]);
    if (expect !== b.sig) return err(403, "bad signature");

    // 防重放 nonce
    if (!(await this.claimNonce(b.nonce, now))) {
      return err(409, "nonce replay");
    }

    // 配额检查
    const used = await this.usedBytes();
    const quota = Number(this.env.MAILBOX_QUOTA_BYTES);
    if (used + size > quota) {
      return err(507, `mailbox quota exceeded (${used + size}/${quota} bytes)`);
    }

    const id = await this.nextId();
    const stored: StoredEnvelope = {
      id, sender_mb: b.sender_mb, seq_hint: b.seq_hint ?? 0,
      ciphertext: b.ciphertext, size, created_at: now, expires_at: exp,
    };
    await this.state.storage.put(`mail:${id}`, stored);
    await this.addBytes(size);
    return json({ ok: true, id });
  }

  private async handleFetch(url: URL, now: number): Promise<Response> {
    const p = url.searchParams;
    const mailboxKey = p.get("mailbox_key")!;
    const recipientMb = p.get("recipient_mb") ?? "";
    const after = p.get("after") ?? "0";
    const limit = Math.min(Number(p.get("limit") ?? 20), 100);
    const ts = Number(p.get("ts") ?? 0);
    const nonce = p.get("nonce") ?? "";
    const sig = p.get("sig") ?? "";

    if (!isHex(recipientMb, 64) || !isHex(sig, 64) || !isHex(nonce, 32)) {
      return err(400, "recipient_mb/sig 64-hex, nonce 32-hex required");
    }
    if (!checkFreshness(ts, now)) return err(403, "stale timestamp");

    // 取件者用“自己→自己”的凭证自证（注册时 recipient=self 的凭证）
    const cred = await this.getCred(recipientMb);
    if (!cred) return err(403, "no credential for recipient");

    const expect = await hmacHex(cred.cred_hex,
      ["relay-fetch", mailboxKey, after, String(limit), String(ts), nonce]);
    if (expect !== sig) return err(403, "bad signature");
    if (!(await this.claimNonce(nonce, now))) return err(409, "nonce replay");

    // 游标取件：mail:<id> 按 id 字典序（零填充）单调
    const start = `mail:${padId(after)}`;
    const items = await this.state.storage.list<StoredEnvelope>({
      start, limit,
    });
    const envelopes = [...items.values()]
      .filter((e) => e.id > after)
      .map((e) => ({
        id: e.id, sender_mb: e.sender_mb, seq_hint: e.seq_hint,
        ciphertext: e.ciphertext, created_at: e.created_at, expires_at: e.expires_at,
      }));
    return json({ ok: true, envelopes });
  }

  private async handleAck(request: Request, now: number): Promise<Response> {
    const b = await request.json() as {
      mailbox_key: string; recipient_mb: string; ids: string[];
      ts: number; nonce: string; sig: string;
    };
    if (!isHex(b.recipient_mb, 64) || !isHex(b.sig, 64) || !isHex(b.nonce, 32)) {
      return err(400, "recipient_mb/sig 64-hex, nonce 32-hex required");
    }
    if (!Array.isArray(b.ids) || b.ids.length === 0 || b.ids.length > 100) {
      return err(400, "ids must be 1..100 items");
    }
    if (!checkFreshness(b.ts, now)) return err(403, "stale timestamp");

    const cred = await this.getCred(b.recipient_mb);
    if (!cred) return err(403, "no credential for recipient");

    const idsJoined = b.ids.join(",");
    const expect = await hmacHex(cred.cred_hex,
      ["relay-ack", b.mailbox_key, idsJoined, String(b.ts), b.nonce]);
    if (expect !== b.sig) return err(403, "bad signature");
    if (!(await this.claimNonce(b.nonce, now))) return err(409, "nonce replay");

    let deleted = 0;
    for (const id of b.ids) {
      const key = `mail:${id}`;
      const env = await this.state.storage.get<StoredEnvelope>(key);
      if (env) {
        await this.state.storage.delete(key);
        await this.addBytes(-env.size);
        deleted++;
      }
    }
    return json({ ok: true, deleted });
  }

  private async handleStatus(url: URL, now: number): Promise<Response> {
    const p = url.searchParams;
    const mailboxKey = p.get("mailbox_key")!;
    const recipientMb = p.get("recipient_mb") ?? "";
    const ts = Number(p.get("ts") ?? 0);
    const nonce = p.get("nonce") ?? "";
    const sig = p.get("sig") ?? "";

    if (!isHex(recipientMb, 64) || !isHex(sig, 64) || !isHex(nonce, 32)) {
      return err(400, "recipient_mb/sig 64-hex, nonce 32-hex required");
    }
    if (!checkFreshness(ts, now)) return err(403, "stale timestamp");

    const cred = await this.getCred(recipientMb);
    if (!cred) return err(403, "no credential");

    const expect = await hmacHex(cred.cred_hex,
      ["relay-status", mailboxKey, String(ts), nonce]);
    if (expect !== sig) return err(403, "bad signature");
    if (!(await this.claimNonce(nonce, now))) return err(409, "nonce replay");

    const used = await this.usedBytes();
    const items = await this.state.storage.list<StoredEnvelope>({ prefix: "mail:" });
    let earliestExpiry: number | null = null;
    for (const e of items.values()) {
      if (earliestExpiry === null || e.expires_at < earliestExpiry) {
        earliestExpiry = e.expires_at;
      }
    }
    return json({
      ok: true, bytes: used, count: items.size,
      quota: Number(this.env.MAILBOX_QUOTA_BYTES),
      earliest_expiry: earliestExpiry,
    });
  }

  // -- 设备发现（presence / peers）---------------------------------------------

  /**
   * 心跳：设备周期性上报"我在线"+ 端到端加密的端点信息。
   * endpoint_blob 是用共享密钥加密的 {addr, port, device_id, ...}，中继不可读，
   * 只作为盲索引存着，等待对端来取。发现与邮箱共用同一套 cred 鉴权。
   *
   * 签名：HMAC(cred, "relay-presence"|mailbox_key|endpoint_hash|ttl_ms|ts|nonce)
   */
  private async handlePresence(request: Request, now: number): Promise<Response> {
    const b = await request.json() as {
      mailbox_key: string; endpoint_blob: string; ttl_ms: number;
      ts: number; nonce: string; sig: string;
    };
    if (!isHex(b.sig, 64) || !isHex(b.nonce, 32)) {
      return err(400, "sig 64-hex, nonce 32-hex required");
    }
    if (typeof b.endpoint_blob !== "string" || b.endpoint_blob.length === 0) {
      return err(400, "endpoint_blob required (base64 ciphertext)");
    }
    if (!checkFreshness(b.ts, now)) return err(403, "stale timestamp");

    const cred = await this.getCred(b.mailbox_key);
    if (!cred) return err(403, "no credential");

    const ttl = Math.min(Number.isFinite(b.ttl_ms) ? b.ttl_ms : 120_000, 300_000); // 硬上限 5 分钟
    const endpointHash = await sha256Hex(b.endpoint_blob);
    const expect = await hmacHex(cred.cred_hex,
      ["relay-presence", b.mailbox_key, endpointHash, String(ttl), String(b.ts), b.nonce]);
    if (expect !== b.sig) return err(403, "bad signature");
    if (!(await this.claimNonce(b.nonce, now))) return err(409, "nonce replay");

    // 存为 peer 可见的索引：别的设备来 peers 时按 mailbox_key 查
    await this.state.storage.put(`presence:peer:${b.mailbox_key}`, {
      endpoint_blob: b.endpoint_blob,
      expires_at: now + ttl,
      updated_at: now,
    } satisfies PresenceRecord);
    return json({ ok: true, ttl_ms: ttl });
  }

  /**
   * 发现：返回本设备可见的对端 presence（含加密端点，中继不可读）。
   * 可见性 = 对端 mailbox_key 已在本邮箱注册过 cred（即双方绑定过）。
   * 签名：HMAC(cred, "relay-peers"|mailbox_key|ts|nonce)
   */
  private async handlePeers(url: URL, now: number): Promise<Response> {
    const p = url.searchParams;
    const mailboxKey = p.get("mailbox_key")!;
    const ts = Number(p.get("ts") ?? 0);
    const nonce = p.get("nonce") ?? "";
    const sig = p.get("sig") ?? "";
    // 对端 mailbox_key 列表（调用方想知道哪些对端在线；空则返回全部已绑定的）
    const want = p.get("want")?.split(",").filter(Boolean) ?? [];

    if (!isHex(sig, 64) || !isHex(nonce, 32)) {
      return err(400, "sig 64-hex, nonce 32-hex required");
    }
    if (!checkFreshness(ts, now)) return err(403, "stale timestamp");

    const cred = await this.getCred(mailboxKey);
    if (!cred) return err(403, "no credential");

    const expect = await hmacHex(cred.cred_hex,
      ["relay-peers", mailboxKey, String(ts), nonce]);
    if (expect !== sig) return err(403, "bad signature");
    if (!(await this.claimNonce(nonce, now))) return err(409, "nonce replay");

    // 收集所有已注册 cred 的对端（sender_mb 列表），过滤出在线的
    const credEntries = await this.state.storage.list<CredRecord>({ prefix: "cred:" });
    const peers: Array<{ mailbox_key: string; endpoint_blob: string; updated_at: number }> = [];
    for (const [key, rec] of credEntries) {
      const peerMb = key.slice("cred:".length);
      if (peerMb === mailboxKey) continue; // 自己不算对端
      if (rec.expires_at <= now) continue;
      if (want.length > 0 && !want.includes(peerMb)) continue;
      const presence = await this.state.storage.get<PresenceRecord>(`presence:peer:${peerMb}`);
      if (presence && presence.expires_at > now) {
        peers.push({
          mailbox_key: peerMb,
          endpoint_blob: presence.endpoint_blob,
          updated_at: presence.updated_at,
        });
      }
    }
    return json({ ok: true, peers });
  }

  // -- 内部工具 ---------------------------------------------------------------

  /** nonce 一次性申领：已存在且未过期 → false（重放）。 */
  private async claimNonce(nonce: string, now: number): Promise<boolean> {
    const key = `nonce:${nonce}`;
    const exp = await this.state.storage.get<number>(key);
    if (exp && exp > now) return false;
    await this.state.storage.put(key, now + NONCE_TTL_MS);
    return true;
  }

  private async nextId(): Promise<string> {
    const n = (await this.state.storage.get<number>("meta:counter")) ?? 0;
    const next = n + 1;
    await this.state.storage.put("meta:counter", next);
    return padId(String(next));
  }

  private async usedBytes(): Promise<number> {
    return (await this.state.storage.get<number>("meta:bytes")) ?? 0;
  }

  private async addBytes(delta: number): Promise<void> {
    const cur = await this.usedBytes();
    await this.state.storage.put("meta:bytes", Math.max(0, cur + delta));
  }

  /** 惰性清理过期邮件与 nonce。 */
  private async sweepExpired(now: number): Promise<void> {
    const last = (await this.state.storage.get<number>("meta:lastSweep")) ?? 0;
    if (now - last < 60_000) return; // 最多每分钟扫一次
    await this.state.storage.put("meta:lastSweep", now);

    const mails = await this.state.storage.list<StoredEnvelope>({ prefix: "mail:" });
    let freed = 0;
    for (const [key, e] of mails) {
      if (e.expires_at <= now) {
        await this.state.storage.delete(key);
        freed += e.size;
      }
    }
    if (freed > 0) await this.addBytes(-freed);

    const nonces = await this.state.storage.list<number>({ prefix: "nonce:" });
    for (const [key, exp] of nonces) {
      if (exp <= now) await this.state.storage.delete(key);
    }

    // 过期 presence（心跳停止的设备从发现列表消失）
    const presences = await this.state.storage.list<PresenceRecord>({ prefix: "presence:peer:" });
    for (const [key, p] of presences) {
      if (p.expires_at <= now) await this.state.storage.delete(key);
    }
  }
}

// ---------------------------------------------------------------------------
// 加密原语（WebCrypto）：中继只做 HMAC 验签与 SHA256，从不接触明文
// ---------------------------------------------------------------------------

async function importHmacKey(hexKey: string): Promise<CryptoKey> {
  const raw = hexToBytes(hexKey);
  return crypto.subtle.importKey(
    "raw", raw, { name: "HMAC", hash: "SHA-256" }, false, ["sign"],
  );
}

/** HMAC-SHA256(key, parts.join("|"))，输出 hex。parts 需不含 "|"（均为 hex/数字/固定串）。 */
export async function hmacHex(hexKey: string, parts: string[]): Promise<string> {
  const key = await importHmacKey(hexKey);
  const msg = new TextEncoder().encode(parts.join("|"));
  const sig = await crypto.subtle.sign("HMAC", key, msg);
  return bytesToHex(new Uint8Array(sig));
}

export async function sha256Hex(data: string): Promise<string> {
  const digest = await crypto.subtle.digest("SHA-256", new TextEncoder().encode(data));
  return bytesToHex(new Uint8Array(digest));
}

function isHex(s: string, len: number): boolean {
  return typeof s === "string" && new RegExp(`^[0-9a-f]{${len}}$`).test(s);
}

function hexToBytes(hex: string): Uint8Array {
  const out = new Uint8Array(hex.length / 2);
  for (let i = 0; i < out.length; i++) {
    out[i] = parseInt(hex.slice(i * 2, i * 2 + 2), 16);
  }
  return out;
}

function bytesToHex(bytes: Uint8Array): string {
  return [...bytes].map((b) => b.toString(16).padStart(2, "0")).join("");
}

function padId(id: string): string {
  return id.padStart(16, "0");
}
