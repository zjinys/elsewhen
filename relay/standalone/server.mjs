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
 *   MAIL_TTL_MS / MAILBOX_QUOTA_BYTES / MAX_ENVELOPE_BYTES 与 worker 版同名同默认值
 *
 * Docker: 见 ../docker/Dockerfile
 */
import { createHash, createHmac, timingSafeEqual } from 'node:crypto';
import { createServer } from 'node:http';
import { mkdirSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { Worker } from 'node:worker_threads';
import { BLOB_QUOTA_BYTES_DEFAULT, BLOB_TTL_MS_DEFAULT, handleBlobRequest } from './blob.mjs';
import { openMailboxDb, purgeExpired } from './schema.mjs';

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
const POST_ROUTES = new Set(['/create', '/register', '/revoke', '/rotate', '/send', '/ack', '/drop']);
const GET_ROUTES = new Set(['/fetch', '/status', '/sender']);

const PORT = Number(process.env.PORT ?? 8787);
const DATA_DIR = process.env.DATA_DIR ?? join(process.cwd(), 'data');
const MAIL_TTL_MS = configured(process.env.MAIL_TTL_MS, 7 * 24 * 60 * 60_000, 30 * 24 * 60 * 60_000);
const MAILBOX_QUOTA_BYTES = configured(process.env.MAILBOX_QUOTA_BYTES, 50 * 1024 * 1024, 512 * 1024 * 1024);
const MAX_ENVELOPE_BYTES = configured(process.env.MAX_ENVELOPE_BYTES, MAX_ENVELOPE, MAX_ENVELOPE);
// 数据面独立配额：blob 只跟 blob 抢空间，永远不会挤掉信令（事故根因之一）。
const BLOB_QUOTA_BYTES = configured(process.env.BLOB_QUOTA_BYTES, BLOB_QUOTA_BYTES_DEFAULT, 512 * 1024 * 1024);
const BLOB_TTL_MS = configured(process.env.BLOB_TTL_MS, BLOB_TTL_MS_DEFAULT, 24 * 60 * 60_000);

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
/**
 * `SQLITE_BUSY`/`SQLITE_LOCKED` 不是「服务器出错」，是「这一拍正好有人在写同一封信箱」。
 *
 * `errcode` 的低 8 位是主错误码（5 = BUSY、6 = LOCKED），高位是不同的扩展变体
 * （BUSY_SNAPSHOT 517、BUSY_RECOVERY 261……）—— 按低 8 位判，所有变体一起认。
 * 原来这类错误落到 500：客户端会把「等一下就好」当成「中继坏了」，一边退避一边
 * 上报故障，运维看到的也是 500 而不是可自愈的争用。
 * `Retry-After` 按秒给出建议重试间隔，HTTP 语义上 503 就该带它。
 */
function isBusy(error) {
  const primary = Number(error?.errcode) & 0xff;
  return primary === 5 || primary === 6;
}
function failure(error) {
  if (error instanceof HttpError) return json({ error: error.message }, error.status);
  if (isBusy(error)) {
    const busy = json({ error: 'relay_busy' }, 503);
    busy.headers['retry-after'] = '1';
    return busy;
  }
  console.error(error);
  return json({ error: 'relay internal error' }, 500);
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
  // 与协议层保持同一条契约：非字符串、空体、长度非 4 的倍数一律 400。
  // 缺这三条时，空 payload 会被当成 0 字节邮件入库、非字符串会在 endsWith 上
  // 抛 TypeError（500）、长度不合法会算出小数大小——都不会报 400。
  require(typeof value === 'string' && value.length > 0 && value.length % 4 === 0, 400, 'ciphertext must be base64');
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
// 句柄上限：信箱数没有上限（每个配对设备一个），句柄却不能无限涨。
// 所有库操作在本文件里都是同步的，所以淘汰只会发生在两个请求之间，
// 绝不会关掉正在使用的句柄。
const HANDLE_LIMIT = configured(process.env.HANDLE_LIMIT, 128, 8192);
const mailboxes = new Map();
function openMailbox(mailboxKey) {
  const cached = mailboxes.get(mailboxKey);
  if (cached) {
    // 触碰即提到队尾：Map 的迭代顺序就是 LRU 顺序。
    mailboxes.delete(mailboxKey);
    mailboxes.set(mailboxKey, cached);
    return cached;
  }
  const dir = join(DATA_DIR, 'mailboxes');
  mkdirSync(dir, { recursive: true });
  const db = openMailboxDb(join(dir, `${mailboxKey}.db`));
  mailboxes.set(mailboxKey, db);
  while (mailboxes.size > HANDLE_LIMIT) {
    const oldest = mailboxes.keys().next().value;
    const stale = mailboxes.get(oldest);
    mailboxes.delete(oldest);
    try { stale.close(); } catch { /* 已关闭 */ }
  }
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
    return purgeExpired(this.db, now);
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
      // 幂等投递：同一 (sender_mb, seq_hint) 的重发只保留一份，而不是再插一行。
      // 客户端在丢响应 / 租约恢复后会把同一封信重复投，旧实现每次都新增一行，
      // 于是 relay 上留下 4.4× 的重复副本并反复撞配额。
      //
      // **必须原地替换、保住原 id**：接收方按 id 递增的游标翻页（`id>after`），
      // 若重发把行删掉再插（新 id 落在末尾），每拍重发的整批会把接收方永远卡在
      // 「最新一批的前几封」上 —— 本机实测 40 拍只翻到 10 封里的前 3 封，快照的
      // snapshot-end 帧永远够不到，导入永不完成。重发内容本就与旧行完全相同
      // （outbox 里存的是同一串密文），所以绝大多数重发是纯粹的账目 no-op。
      if (seq > 0) {
        const existing = sqlGet(this.db, 'SELECT id,bytes FROM relay_v2_mail WHERE sender_mb=? AND seq_hint=?', sender, seq);
        if (existing) {
          const stored = sqlAll(this.db, 'SELECT ciphertext FROM relay_v2_chunks WHERE mail_id=? ORDER BY part', existing.id)
            .map((c) => c.ciphertext).join('');
          if (stored === ciphertext) {
            // 同一封信的重发：不写任何行，配额账目也不变。
            return { ok: true, id: padded(Number(existing.id)) };
          }
          // 内容变化（协议升级 / 重新封装）：原地换掉，id 与排队位置都不动。
          // 先量后写，但把被替换行的字节从账上扣掉：否则替换一份同尺寸的旧信
          // 会在信箱刚好满时误报 507（旧行已不在账上，新行还没计入）。
          const usedForReplace = sqlGet(this.db, 'SELECT COALESCE(SUM(bytes),0) AS bytes,COUNT(*) AS n FROM relay_v2_mail');
          require(usedForReplace.bytes - Number(existing.bytes) + size <= MAILBOX_QUOTA_BYTES, 507, 'mailbox quota exceeded');
          sqlRun(this.db, 'DELETE FROM relay_v2_chunks WHERE mail_id=?', existing.id);
          sqlRun(this.db, 'UPDATE relay_v2_mail SET bytes=?,created_at=?,expires_at=? WHERE id=?',
            size, now, Math.min(expiry, now + MAIL_TTL_MS), existing.id);
          for (let offset = 0, part = 0; offset < ciphertext.length; offset += CHUNK_CHARS, part++) {
            sqlRun(this.db, 'INSERT INTO relay_v2_chunks (mail_id,part,ciphertext) VALUES (?,?,?)', existing.id, part, ciphertext.slice(offset, offset + CHUNK_CHARS));
          }
          return { ok: true, id: padded(Number(existing.id)) };
        }
      }
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
  /**
   * 放弃：`ack` 是「这封信我消费了」，`drop` 是「这封信我永远不会消费」。
   * relay 上动作相同（删除），但**语义与审计不同** —— 客户端必须能分开表态，
   * 否则「已处理」和「已丢弃」写成同一个词，死信排查就只剩猜。
   * 鉴权、nonce、id 校验与 `ack` 完全同形，签名域单独一个 `drop`（域分离是
   * 「拿 ack 的签名去 drop」这种跨操作重放的结构性防线）。
   */
  drop(body) {
    require(Array.isArray(body.ids) && body.ids.length >= 1 && body.ids.length <= 100, 400, 'ids must contain 1..100 mail ids');
    const ids = body.ids;
    const parsed = ids.map((id) => { const n = cursor(id); require(n > 0, 400, 'invalid mail id'); return n; });
    require(new Set(parsed).size === parsed.length, 400, 'duplicate mail ids');
    return this.ownerOperation(body, 'drop', [body.mailbox_key, ids.join(',')],
      () => {
        let deleted = 0;
        for (const id of parsed) {
          sqlRun(this.db, 'DELETE FROM relay_v2_chunks WHERE mail_id=?', id);
          deleted += this.db.prepare('DELETE FROM relay_v2_mail WHERE id=?').run(id).changes;
        }
        // 只报「真的删掉了几封」：重复 drop 同一封得 0，不算错误（幂等）。
        return { ok: true, deleted };
      });
  }
  statusFor(body) {
    return this.ownerOperation(body, 'status', [body.mailbox_key], () => statusPayload(this.db));
  }
}
function statusPayload(db) {
  const row = sqlGet(db, 'SELECT COALESCE(SUM(bytes),0) AS bytes,COUNT(*) AS count,MIN(expires_at) AS earliest_expiry FROM relay_v2_mail');
  // 两本账：信令（信）与数据（blob）各自独立计量，互不占用配额。
  // 这是事故的直接根因之一（快照把控制消息挤死），所以必须在 /status 上分开看。
  // 旧字段 `bytes`/`quota` 保留原义（= 信令账），老客户端不需要改。
  // blob_bytes 是**已落盘**字节（真实占盘，回收指标看这个）；配额按 open 时声明的
  // bytes 预扣，所以一个刚 open 还没传片的 blob 占配额但不占盘。
  const blobs = sqlGet(db, 'SELECT COALESCE(SUM(LENGTH(ciphertext)),0) AS bytes FROM relay_v3_blob_chunk');
  const openBlobs = sqlGet(db, 'SELECT COUNT(*) AS count FROM relay_v3_blob');
  const signalBytes = Number(row.bytes);
  return {
    ok: true,
    version: VERSION,
    capabilities: ['blob', 'drop'],
    bytes: signalBytes,
    count: Number(row.count),
    earliest_expiry: row.earliest_expiry === null ? null : Number(row.earliest_expiry),
    quota: MAILBOX_QUOTA_BYTES,
    signal_bytes: signalBytes,
    blob_bytes: Number(blobs.bytes),
    blobs: Number(openBlobs.count),
    blob_quota: BLOB_QUOTA_BYTES,
    blob_ttl_ms: BLOB_TTL_MS,
  };
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
/** 裸二进制请求体（`PUT /blob/chunk`）：与 `boundedBody` 同一套长度契约，只是不解码 UTF-8。 */
async function boundedBinary(req, limit) {
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
  require(size > 0, 400, 'body required');
  return Buffer.concat(chunks);
}

/** `/blob/*` 需要的上下文：全部指向 `server.mjs` 现成的实现，不另写一套鉴权/事务。 */
const blobContext = {
  version: VERSION,
  require, json, failure, send, queryBody, boundedBody, boundedBinary, parsedJson,
  validateEnvelope, openMailbox, Mailbox,
  hex, integer, sha256Hex, stamp, signed, verifyHmac, withTx,
  isHex: hex,
  sqlGet, sqlAll, sqlRun,
  blobLimits: { quota: BLOB_QUOTA_BYTES, ttl: BLOB_TTL_MS },
};

async function handle(req, res) {
  const url = new URL(req.url, `http://${req.headers.host ?? 'localhost'}`);
  try {
    if (url.pathname === '/health' && req.method === 'GET') return send(res, json({ ok: true, service: 'elsewhen-relay', version: VERSION }));
    if (url.pathname === '/presence' || url.pathname === '/peers') throw new HttpError(410, 'discovery is not supported in relay v2');
    // 数据面（v3 blob）单独分流：`PUT /blob/chunk` 的 body 是裸二进制，不能走 JSON 解析。
    if (url.pathname.startsWith('/blob/')) return await handleBlobRequest(req, res, url, blobContext);
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
      case '/drop': return send(res, json(await mailbox.drop(body)));
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
  console.log(`elsewhen-relay v${VERSION} listening on :${actual} (data=${DATA_DIR})`);
});

/** 周期磁盘回收：删行 → WAL 截断 → freelist 占比过高时 VACUUM。
 *
 * 放 `worker_threads` 里跑：`node:sqlite` 是同步 API，在主线程重写一个 690 MB 的
 * 信箱库会把所有请求卡住。扫的是**目录**而不是已打开句柄：设备退役后再无请求的
 * 信箱也能被收掉（原来那个只遍历 `mailboxes` 的 `setInterval` 结构上做不到）。
 * worker 启动即跑一轮，所以 relay 重启后磁盘占用立刻下降，不用等第一个周期。
 */
const SWEEP_INTERVAL_MS = configured(process.env.SWEEP_INTERVAL_MS, 10 * 60_000, 24 * 60 * 60_000);
if (process.env.ELSEWHEN_DISABLE_SWEEP !== '1') {
  const sweeper = new Worker(join(dirname(fileURLToPath(import.meta.url)), 'sweeper.mjs'), {
    workerData: { sweep: true, dataDir: DATA_DIR, intervalMs: SWEEP_INTERVAL_MS },
  });
  sweeper.on('message', (result) => {
    if (process.env.ELSEWHEN_DEBUG && (result.freed > 0 || result.failed > 0)) {
      console.log(`sweep swept=${result.swept} failed=${result.failed} freed=${result.freed}`);
    }
  });
  sweeper.on('error', (error) => console.error('sweeper failed', error));
  // 回收器不该让进程活着：服务停了就一起退。
  sweeper.unref();
}
