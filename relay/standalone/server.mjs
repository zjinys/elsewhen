/**
 * Elsewhen 盲中继 v2 —— 独立部署版（NAS / Docker / 任意 Node 主机）。
 *
 * 与 Cloudflare Worker 版（../src/worker.ts）协议逐条对齐：
 *   - 相同端点：/create /register /revoke /rotate /send /ack /fetch /status /sender /health
 *   - 相同认证：HMAC-SHA256，消息 = ["relay-v2-<domain>", ...parts, ts, nonce] 以 "|" 连接
 *   - 相同语义：nonce 防重放（10 分钟）、信件 TTL（默认 7 天）、配额上限、
 *     sender 授权 CAS（revision 递增）、owner generation、分块存储
 *
 * 与 CF 的差别只在存储：每个信箱一个 SQLite 文件（node:sqlite 内置，无原生依赖），
 * 位于 DATA_DIR/mailboxes/<mailbox_key>.db，懒创建。
 *
 * 环境变量：
 *   PORT          监听端口（默认 8787）
 *   DATA_DIR      信箱存储目录（默认 ./data）
 *   RELEASES_DIR  应用升级分发目录（默认 ./releases；manifest: release/manifest.json，
 *                 APK: apk/<hex>.apk；不存在该端点返 404，不影响配对）
 *   MAIL_TTL_MS / MAILBOX_QUOTA_BYTES / MAX_ENVELOPE_BYTES 与 worker 版同名同默认值
 *
 * Docker: 见 ../docker/Dockerfile
 */
import { createHash, createHmac, timingSafeEqual } from 'node:crypto';
import { createServer } from 'node:http';
import { DatabaseSync } from 'node:sqlite';
import { mkdirSync, createReadStream, statSync, readFileSync } from 'node:fs';
import { join } from 'node:path';

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
const POST_ROUTES = new Set(['/create', '/register', '/revoke', '/rotate', '/send', '/ack']);
const GET_ROUTES = new Set(['/fetch', '/status', '/sender']);
const RELEASE_PATTERN = /^[0-9a-f]{7,64}$/;

const PORT = Number(process.env.PORT ?? 8787);
const DATA_DIR = process.env.DATA_DIR ?? join(process.cwd(), 'data');
const RELEASES_DIR = process.env.RELEASES_DIR ?? join(process.cwd(), 'releases');
const MAIL_TTL_MS = configured(process.env.MAIL_TTL_MS, 7 * 24 * 60 * 60_000, 30 * 24 * 60 * 60_000);
const MAILBOX_QUOTA_BYTES = configured(process.env.MAILBOX_QUOTA_BYTES, 50 * 1024 * 1024, 512 * 1024 * 1024);
const MAX_ENVELOPE_BYTES = configured(process.env.MAX_ENVELOPE_BYTES, MAX_ENVELOPE, MAX_ENVELOPE);

function configured(value, fallback, maximum) {
  const parsed = Number(value);
  return Number.isSafeInteger(parsed) && parsed >= 1 && parsed <= maximum ? parsed : fallback;
}
function envelopeLimit() { return MAX_ENVELOPE_BYTES; }

class HttpError extends Error {
  constructor(status, message) { super(message); this.status = status; }
}
function require(condition, status, message) {
  if (!condition) throw new HttpError(status, message);
}
function json(data, status = 200) {
  return { status, body: JSON.stringify(data), headers: { 'content-type': 'application/json', 'cache-control': 'no-store' } };
}
function failure(error) {
  return error instanceof HttpError
    ? json({ error: error.message }, error.status)
    : (console.error(error), json({ error: 'relay internal error' }, 500));
}
function hex(value, length) {
  return typeof value === 'string' && new RegExp(`^[0-9a-f]{${length}}$`).test(value);
}
function integer(value, min = 0, max = Number.MAX_SAFE_INTEGER) {
  return typeof value === 'number' && Number.isSafeInteger(value) && value >= min && value <= max;
}
function padded(id) { return String(id).padStart(16, '0'); }
function cursor(value) {
  require(typeof value === 'string' && /^(?:0|[0-9]{16})$/.test(value), 400, 'invalid mail cursor');
  const parsed = Number(value);
  require(integer(parsed), 400, 'invalid mail cursor');
  return parsed;
}
function base64Size(value) {
  require(!/[^A-Za-z0-9+/=]/.test(value), 400, 'ciphertext must be base64');
  const padding = value.endsWith('==') ? 2 : value.endsWith('=') ? 1 : 0;
  require(value.indexOf('=') === -1 || value.indexOf('=') === value.length - padding, 400, 'invalid base64 padding');
  return value.length / 4 * 3 - padding;
}
function checkFreshness(ts, now) {
  return integer(ts) && Math.abs(now - ts) <= SKEW_MS;
}
function hmacHex(credHex, parts) {
  return createHmac('sha256', Buffer.from(credHex, 'hex')).update(parts.join('|')).digest('hex');
}
function sha256Hex(value) { return createHash('sha256').update(value).digest('hex'); }
/** 常数时间比较，防时序侧信道。 */
function verifyHmac(credHex, parts, signature) {
  if (!hex(credHex, 64) || !hex(signature, 64)) return false;
  const expected = Buffer.from(hmacHex(credHex, parts), 'hex');
  const actual = Buffer.from(signature, 'hex');
  return expected.length === actual.length && timingSafeEqual(expected, actual);
}
function signed(domain, parts, auth) {
  return [`relay-v2-${domain}`, ...parts, String(auth.ts), auth.nonce];
}
function stamp(body) {
  require(integer(body.ts) && hex(body.nonce, 32) && hex(body.sig, 64), 400, 'invalid request authentication');
  require(checkFreshness(body.ts, Date.now()), 403, 'stale timestamp');
  return { ts: body.ts, nonce: body.nonce, sig: body.sig };
}
function validateEnvelope(body) {
  require(body.version === VERSION, 426, 'relay protocol v2 required');
  require(hex(body.mailbox_key, 64), 400, 'invalid mailbox_key');
  // 旧客户端不能把 sender 记录当收件凭据用。
  require(body.recipient_mb === undefined || body.recipient_mb === body.mailbox_key, 403, 'mailbox owner required');
}
function queryBody(url) {
  const body = {};
  for (const [key, value] of url.searchParams) {
    require(!(key in body), 400, 'duplicate query parameter');
    body[key] = value;
  }
  for (const key of ['version', 'ts', 'limit']) {
    if (body[key] !== undefined) {
      require(typeof body[key] === 'string' && /^\d+$/.test(body[key]), 400, 'invalid numeric parameter');
      body[key] = Number(body[key]);
    }
  }
  return body;
}

/** 信箱存储：一个信箱一个 SQLite 文件，表结构与 CF DO 版一致。 */
const mailboxes = new Map();
function openMailbox(mailboxKey) {
  let db = mailboxes.get(mailboxKey);
  if (db) return db;
  const dir = join(DATA_DIR, 'mailboxes');
  mkdirSync(dir, { recursive: true });
  db = new DatabaseSync(join(dir, `${mailboxKey}.db`));
  db.exec(`
    PRAGMA journal_mode = WAL;
    CREATE TABLE IF NOT EXISTS relay_v2_owner (singleton INTEGER PRIMARY KEY CHECK(singleton=1), mailbox_key TEXT NOT NULL, cred_hex TEXT NOT NULL, generation INTEGER NOT NULL);
    CREATE TABLE IF NOT EXISTS relay_v2_senders (sender_mb TEXT PRIMARY KEY, cred_hex TEXT, expires_at INTEGER NOT NULL, revision INTEGER NOT NULL);
    CREATE TABLE IF NOT EXISTS relay_v2_nonces (nonce TEXT PRIMARY KEY, expires_at INTEGER NOT NULL);
    CREATE INDEX IF NOT EXISTS relay_v2_nonce_expiry ON relay_v2_nonces(expires_at);
    CREATE TABLE IF NOT EXISTS relay_v2_mail (id INTEGER PRIMARY KEY AUTOINCREMENT, sender_mb TEXT NOT NULL, seq_hint INTEGER NOT NULL, bytes INTEGER NOT NULL, created_at INTEGER NOT NULL, expires_at INTEGER NOT NULL);
    CREATE INDEX IF NOT EXISTS relay_v2_mail_expiry ON relay_v2_mail(expires_at);
    CREATE TABLE IF NOT EXISTS relay_v2_chunks (mail_id INTEGER NOT NULL, part INTEGER NOT NULL, ciphertext TEXT NOT NULL, PRIMARY KEY(mail_id,part));
  `);
  mailboxes.set(mailboxKey, db);
  return db;
}
function sqlGet(db, query, ...params) {
  return db.prepare(query).get(...params);
}
function sqlAll(db, query, ...params) {
  return db.prepare(query).all(...params);
}
/** node:sqlite 的 exec 不支持绑定参数；带参写入一律走 prepare().run()。 */
function sqlRun(db, query, ...params) {
  return db.prepare(query).run(...params);
}
function withTx(db, apply) {
  db.exec('BEGIN IMMEDIATE');
  try {
    const result = apply();
    db.exec('COMMIT');
    return result;
  } catch (error) {
    try { db.exec('ROLLBACK'); } catch { /* 已回滚 */ }
    throw error;
  }
}

class Mailbox {
  constructor(db) { this.db = db; }

  owner(mailbox) {
    const row = sqlGet(this.db, 'SELECT * FROM relay_v2_owner WHERE singleton=1');
    require(row && row.mailbox_key === mailbox, 403, 'mailbox owner not registered');
    return row;
  }
  sender(sender) {
    return sqlGet(this.db, 'SELECT cred_hex,expires_at,revision FROM relay_v2_senders WHERE sender_mb=?', sender);
  }
  clean(now) {
    sqlRun(this.db, 'DELETE FROM relay_v2_chunks WHERE mail_id IN (SELECT id FROM relay_v2_mail WHERE expires_at<=?)', now);
    sqlRun(this.db, 'DELETE FROM relay_v2_mail WHERE expires_at<=?', now);
    sqlRun(this.db, 'DELETE FROM relay_v2_nonces WHERE expires_at<=?', now);
  }
  claim(auth, now) {
    require(checkFreshness(auth.ts, now), 403, 'stale timestamp');
    require(!sqlGet(this.db, 'SELECT 1 FROM relay_v2_nonces WHERE nonce=?', auth.nonce), 409, 'nonce replay');
    const { n } = sqlGet(this.db, 'SELECT COUNT(*) AS n FROM relay_v2_nonces');
    require(n < MAX_NONCES, 429, 'mailbox request window full');
    sqlRun(this.db, 'INSERT INTO relay_v2_nonces (nonce,expires_at) VALUES (?,?)', auth.nonce, now + NONCE_TTL_MS);
  }
  ownerOperation(body, domain, parts, apply) {
    const auth = stamp(body);
    const owner = this.owner(body.mailbox_key);
    require(verifyHmac(owner.cred_hex, signed(domain, parts, auth), auth.sig), 403, 'bad owner signature');
    return withTx(this.db, () => {
      const now = Date.now();
      require(this.owner(body.mailbox_key).generation === owner.generation, 409, 'owner credential changed');
      this.clean(now);
      this.claim(auth, now);
      return apply(now, owner);
    });
  }

  create(body) {
    const auth = stamp(body);
    require(hex(body.owner_cred_hex, 64), 400, 'invalid owner credential');
    const credential = body.owner_cred_hex;
    require(verifyHmac(credential, ['relay-mailbox'], body.mailbox_key), 403, 'owner credential does not bind mailbox');
    require(verifyHmac(credential, signed('create', [body.mailbox_key], auth), auth.sig), 403, 'bad owner proof');
    return withTx(this.db, () => {
      const old = sqlGet(this.db, 'SELECT cred_hex,generation FROM relay_v2_owner WHERE singleton=1');
      require(!old || (old.generation === 1 && old.cred_hex === credential), 409, 'mailbox already initialized');
      const now = Date.now();
      this.clean(now);
      this.claim(auth, now);
      if (!old) sqlRun(this.db, 'INSERT INTO relay_v2_owner (singleton,mailbox_key,cred_hex,generation) VALUES (1,?,?,1)', body.mailbox_key, credential);
      return { ok: true, version: VERSION, owner_generation: 1 };
    });
  }
  register(body) {
    require(hex(body.sender_mb, 64) && body.sender_mb !== body.mailbox_key && hex(body.cred_hex, 64), 400, 'invalid sender credential');
    require(integer(body.expires_at, 1) && integer(body.expected_revision), 400, 'invalid grant version or expiry');
    const sender = body.sender_mb, credential = body.cred_hex, expiry = body.expires_at, expected = body.expected_revision;
    return this.ownerOperation(body, 'register', [body.mailbox_key, sender, credential, String(expiry), String(expected)], (now, owner) => {
      require(expiry > now && expiry <= now + 365 * 24 * 60 * 60_000, 400, 'grant expiry must be within one year');
      require(credential !== owner.cred_hex, 400, 'sender and owner credentials must differ');
      const old = this.sender(sender);
      // 小概率丢响应时的幂等回执：只有恰好落在 expected+1 的完整后状态才算回执，
      // 绝不静默采纳更新版本或复活已撤销/已变更的授权。
      if (old?.revision === expected + 1 && old.cred_hex === credential && old.expires_at === expiry) {
        return { ok: true, revision: old.revision };
      }
      require((old?.revision ?? 0) === expected, 409, 'sender revision changed');
      if (!old) {
        const { n } = sqlGet(this.db, 'SELECT COUNT(*) AS n FROM relay_v2_senders');
        require(n < MAX_SENDERS, 409, 'sender limit reached');
      }
      const revision = expected + 1;
      require(integer(revision, 1), 409, 'sender revision exhausted');
      sqlRun(
        this.db,
        'INSERT INTO relay_v2_senders (sender_mb,cred_hex,expires_at,revision) VALUES (?,?,?,?) ' +
        'ON CONFLICT(sender_mb) DO UPDATE SET cred_hex=excluded.cred_hex,expires_at=excluded.expires_at,revision=excluded.revision',
        sender, credential, expiry, revision,
      );
      return { ok: true, revision };
    });
  }
  revoke(body) {
    require(hex(body.sender_mb, 64) && integer(body.expected_revision, 1), 400, 'invalid sender revision');
    const sender = body.sender_mb, expected = body.expected_revision;
    return this.ownerOperation(body, 'revoke', [body.mailbox_key, sender, String(expected)], () => {
      const old = this.sender(sender);
      if (old?.revision === expected + 1 && old.cred_hex === null && old.expires_at === 0) {
        return { ok: true, revision: old.revision };
      }
      require(old?.revision === expected, 409, 'sender revision changed');
      require(integer(expected + 1, 1), 409, 'sender revision exhausted');
      sqlRun(this.db, 'UPDATE relay_v2_senders SET cred_hex=NULL,expires_at=0,revision=? WHERE sender_mb=?', expected + 1, sender);
      return { ok: true, revision: expected + 1 };
    });
  }
  rotate(body) {
    require(hex(body.new_cred_hex, 64), 400, 'invalid new owner credential');
    const credential = body.new_cred_hex;
    return this.ownerOperation(body, 'rotate', [body.mailbox_key, credential], (_, owner) => {
      require(credential !== owner.cred_hex, 400, 'new owner credential must differ');
      require(integer(owner.generation + 1, 1), 409, 'owner generation exhausted');
      require(!sqlGet(this.db, 'SELECT 1 FROM relay_v2_senders WHERE cred_hex=?', credential), 400, 'owner credential cannot be a sender credential');
      sqlRun(this.db, 'UPDATE relay_v2_owner SET cred_hex=?,generation=? WHERE singleton=1', credential, owner.generation + 1);
      return { ok: true, owner_generation: owner.generation + 1 };
    });
  }
  getSender(body) {
    require(hex(body.sender_mb, 64), 400, 'invalid sender_mb');
    const sender = body.sender_mb;
    return this.ownerOperation(body, 'sender', [body.mailbox_key, sender], (now) => {
      const found = this.sender(sender);
      return { revision: found?.revision ?? 0, active: !!found?.cred_hex && found.expires_at > now, expires_at: found?.expires_at ?? 0 };
    });
  }

  send(body) {
    require(hex(body.sender_mb, 64) && integer(body.seq_hint) && integer(body.expires_at, 1), 400, 'invalid envelope metadata');
    const auth = stamp(body);
    const size = base64Size(body.ciphertext);
    require(size <= envelopeLimit(), 413, 'envelope too large');
    const ciphertext = body.ciphertext, sender = body.sender_mb, seq = body.seq_hint, expiry = body.expires_at;
    this.owner(body.mailbox_key);
    const credential = this.sender(sender);
    require(credential?.cred_hex && credential.expires_at > Date.now(), 403, 'sender not authorized');
    const hash = sha256Hex(ciphertext);
    // 先认证调用者请求的确切过期时间，再在校验后钳制。
    require(
      verifyHmac(
        credential.cred_hex,
        [...signed('send', [sender, body.mailbox_key, String(seq), String(expiry)], auth), hash],
        auth.sig,
      ),
      403, 'bad sender signature',
    );
    return withTx(this.db, () => {
      const now = Date.now(), current = this.sender(sender);
      require(current?.cred_hex && current.revision === credential.revision && current.expires_at > now, 403, 'sender authorization changed');
      require(expiry > now, 400, 'envelope expiry must be in the future');
      this.clean(now);
      this.claim(auth, now);
      const used = sqlGet(this.db, 'SELECT COALESCE(SUM(bytes),0) AS bytes,COUNT(*) AS n FROM relay_v2_mail');
      require(used.n < MAX_MAILS && used.bytes + size <= MAILBOX_QUOTA_BYTES, 507, 'mailbox quota exceeded');
      const result = this.db.prepare(
        'INSERT INTO relay_v2_mail (sender_mb,seq_hint,bytes,created_at,expires_at) VALUES (?,?,?,?,?) RETURNING id',
      ).get(sender, seq, size, now, Math.min(expiry, now + MAIL_TTL_MS));
      const id = Number(result.id);
      require(integer(id, 1), 507, 'mailbox sequence exhausted');
      for (let offset = 0, part = 0; offset < ciphertext.length; offset += CHUNK_CHARS, part++) {
        sqlRun(this.db, 'INSERT INTO relay_v2_chunks (mail_id,part,ciphertext) VALUES (?,?,?)', id, part, ciphertext.slice(offset, offset + CHUNK_CHARS));
      }
      return { ok: true, id: padded(id) };
    });
  }
  receive(body) {
    const afterText = body.after ?? '0', after = cursor(afterText), limit = body.limit ?? 20;
    require(integer(limit, 1, 100), 400, 'limit must be 1..100');
    return this.ownerOperation(body, 'fetch', [body.mailbox_key, String(afterText), String(limit)], () => {
      const rows = sqlAll(this.db, 'SELECT * FROM relay_v2_mail WHERE id>? ORDER BY id LIMIT ?', after, limit + 1);
      const envelopes = [];
      let total = 0;
      for (const row of rows.slice(0, limit)) {
        const encoded = Math.ceil(row.bytes / 3) * 4;
        if (total + encoded + 1024 > RESPONSE_BYTES) break;
        const ciphertext = sqlAll(this.db, 'SELECT ciphertext FROM relay_v2_chunks WHERE mail_id=? ORDER BY part', row.id)
          .map((c) => c.ciphertext).join('');
        envelopes.push({ id: padded(Number(row.id)), sender_mb: row.sender_mb, seq_hint: row.seq_hint, ciphertext, created_at: row.created_at, expires_at: row.expires_at });
        total += encoded + 1024;
      }
      return { ok: true, envelopes, next_cursor: envelopes.at(-1)?.id ?? String(afterText), has_more: envelopes.length < rows.length };
    });
  }
  ack(body) {
    require(Array.isArray(body.ids) && body.ids.length >= 1 && body.ids.length <= 100, 400, 'ids must contain 1..100 mail ids');
    const ids = body.ids;
    const parsed = ids.map((id) => { const n = cursor(id); require(n > 0, 400, 'invalid mail id'); return n; });
    require(new Set(parsed).size === parsed.length, 400, 'duplicate mail ids');
    return this.ownerOperation(body, 'ack', [body.mailbox_key, ids.join(',')],
      () => {
        let deleted = 0;
        for (const id of parsed) {
          sqlRun(this.db, 'DELETE FROM relay_v2_chunks WHERE mail_id=?', id);
          deleted += this.db.prepare('DELETE FROM relay_v2_mail WHERE id=?').run(id).changes;
        }
        return { ok: true, deleted };
      });
  }
  statusFor(body) {
    return this.ownerOperation(body, 'status', [body.mailbox_key], () => statusPayload(this.db));
  }
}
function statusPayload(db) {
  const row = sqlGet(db, 'SELECT COALESCE(SUM(bytes),0) AS bytes,COUNT(*) AS count,MIN(expires_at) AS earliest_expiry FROM relay_v2_mail');
  return { ok: true, bytes: Number(row.bytes), count: Number(row.count), earliest_expiry: row.earliest_expiry === null ? null : Number(row.earliest_expiry), quota: MAILBOX_QUOTA_BYTES };
}
/** 请求体读取：Content-Length 预检 + 实际字节复核，UTF-8 严格解码。 */
async function boundedBody(req, limit) {
  const declared = req.headers['content-length'];
  if (declared !== undefined) require(/^\d+$/.test(declared) && Number(declared) <= limit, 413, 'request too large');
  const chunks = [];
  let size = 0;
  for await (const chunk of req) {
    size += chunk.length;
    if (size > limit) {
      req.destroy();
      throw new HttpError(413, 'request too large');
    }
    chunks.push(chunk);
  }
  if (size === 0) throw new HttpError(400, 'body required');
  try {
    return new TextDecoder('utf-8', { fatal: true }).decode(Buffer.concat(chunks));
  } catch {
    throw new HttpError(400, 'invalid utf-8');
  }
}
function parsedJson(text) {
  let value;
  try { value = JSON.parse(text); } catch { throw new HttpError(400, 'invalid json'); }
  require(value !== null && typeof value === 'object' && !Array.isArray(value), 400, 'json object required');
  return value;
}

function releaseLatest() {
  const path = join(RELEASES_DIR, 'release', 'manifest.json');
  try {
    const content = readFileSync(path);
    return { status: 200, body: content, headers: { 'content-type': 'application/json', 'cache-control': 'public, max-age=60' } };
  } catch {
    throw new HttpError(404, 'no release published');
  }
}
function releaseApk(url) {
  const h = url.searchParams.get('h') ?? '';
  require(RELEASE_PATTERN.test(h), 400, 'invalid release hash');
  const path = join(RELEASES_DIR, 'apk', `${h}.apk`);
  let stat;
  try { stat = statSync(path); } catch { throw new HttpError(404, 'release apk not found'); }
  return {
    status: 200,
    stream: createReadStream(path),
    headers: {
      'content-type': 'application/vnd.android.package-archive',
      'content-length': String(stat.size),
      'cache-control': 'public, max-age=31536000, immutable',
    },
  };
}
async function handle(req, res) {
  const url = new URL(req.url, `http://${req.headers.host ?? 'localhost'}`);
  try {
    if (url.pathname === '/health' && req.method === 'GET') return send(res, json({ ok: true, service: 'elsewhen-relay', version: VERSION }));
    if (url.pathname === '/presence' || url.pathname === '/peers') throw new HttpError(410, 'discovery is not supported in relay v2');
    if (url.pathname === '/release/latest' && req.method === 'GET') return send(res, releaseLatest());
    if (url.pathname === '/release/apk' && req.method === 'GET') return send(res, releaseApk(url));
    const post = POST_ROUTES.has(url.pathname);
    require(post || GET_ROUTES.has(url.pathname), 404, 'not found');
    require(req.method === (post ? 'POST' : 'GET'), 405, 'method not allowed');
    const limit = url.pathname === '/send' ? Math.ceil(envelopeLimit() / 3) * 4 + CONTROL_BYTES : CONTROL_BYTES;
    const body = post ? parsedJson(await boundedBody(req, limit)) : queryBody(url);
    validateEnvelope(body);
    const mailbox = new Mailbox(openMailbox(body.mailbox_key));
    switch (url.pathname) {
      case '/create': return send(res, json(await mailbox.create(body)));
      case '/register': return send(res, json(await mailbox.register(body)));
      case '/revoke': return send(res, json(await mailbox.revoke(body)));
      case '/rotate': return send(res, json(await mailbox.rotate(body)));
      case '/sender': return send(res, json(await mailbox.getSender(body)));
      case '/send': return send(res, json(await mailbox.send(body)));
      case '/fetch': return send(res, json(await mailbox.receive(body)));
      case '/ack': return send(res, json(await mailbox.ack(body)));
      case '/status': return send(res, json(await mailbox.statusFor(body)));
      default: throw new HttpError(404, 'not found');
    }
  } catch (error) {
    return send(res, failure(error));
  }
}
function send(res, message) {
  res.writeHead(message.status, message.headers);
  if (message.stream) message.stream.pipe(res);
  else res.end(message.body);
}

const server = createServer((req, res) => {
  handle(req, res).catch((error) => {
    console.error(error);
    try { send(res, failure(error)); } catch { res.destroy(); }
  });
});
// Node 默认 5 分钟请求头超时即可；防火墙/健康检查友好。
server.requestTimeout = 30_000;
server.headersTimeout = 15_000;
server.listen(PORT, () => {
  const actual = server.address().port;
  console.log(`elsewhen-relay v${VERSION} listening on :${actual} (data=${DATA_DIR} releases=${RELEASES_DIR})`);
});

/** 周期清理跨信箱过期文件（非请求路径的兜底；请求路径仍各自 clean）。 */
const GC_INTERVAL_MS = 60 * 60_000;
setInterval(() => {
  for (const [key, db] of mailboxes) {
    try { withTx(db, () => new Mailbox(db).clean(Date.now())); }
    catch (error) { console.error('gc failed for mailbox', key, error); }
  }
}, GC_INTERVAL_MS).unref();
