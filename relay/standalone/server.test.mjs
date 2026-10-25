/**
 * standalone 中继端到端冒烟：协议语义与 CF 版一致。
 * 覆盖：health、create→status→register→send→fetch→ack 生命周期、
 *       nonce 重放拒绝、坏签名拒绝、撤销后 send 拒绝、
 *       owner 授权 CAS 幂等回执。
 */
import test from 'node:test';
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { mkdtempSync } from 'node:fs';
import { DatabaseSync } from 'node:sqlite';
import { tmpdir } from 'node:os';
import { join, dirname } from 'node:path';
import { createHash, createHmac, randomBytes } from 'node:crypto';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const key = () => randomBytes(32).toString('hex');
const hmac = (cred, parts) => createHmac('sha256', Buffer.from(cred, 'hex')).update(parts.join('|')).digest('hex');
const sha256 = (value) => createHash('sha256').update(value).digest('hex');
const mailboxKey = (credential) => hmac(credential, ['relay-mailbox']);
function authQuery(credential, domain, parts) {
  const a = auth(credential, domain, parts);
  return `ts=${a.ts}&nonce=${a.nonce}&sig=${a.sig}`;
}
function auth(credential, domain, parts) {
  const ts = Date.now();
  const nonce = randomBytes(16).toString('hex');
  return { ts, nonce, sig: hmac(credential, [`relay-v2-${domain}`, ...parts, String(ts), nonce]) };
}

async function startServer(extraEnv = {}) {
  const dataDir = mkdtempSync(join(tmpdir(), 'relay-data-'));
  const proc = spawn(process.execPath, [join(here, 'server.mjs')], {
    env: { ...process.env, PORT: '0', DATA_DIR: dataDir, ...extraEnv },
    stdio: ['ignore', 'pipe', 'inherit'],
  });
  const port = await new Promise((resolve, reject) => {
    proc.stdout.on('data', (chunk) => {
      const match = /listening on [^\n]*:(\d+)/.exec(String(chunk));
      if (match) resolve(Number(match[1]));
    });
    proc.on('exit', (code) => reject(new Error(`relay exited early: ${code}`)));
  });
  return { proc, port, dataDir };
}

async function post(port, path, body) {
  const res = await fetch(`http://127.0.0.1:${port}${path}`, {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify(body),
  });
  return { status: res.status, body: await res.json(), headers: res.headers };
}
async function get(port, path) {
  const res = await fetch(`http://127.0.0.1:${port}${path}`);
  const text = await res.text();
  let body;
  try { body = JSON.parse(text); } catch { body = text; }
  return { status: res.status, body, headers: res.headers };
}

async function seedMailbox(port, { grantSender = false, owner = key(), senderCred = key() } = {}) {
  const mb = mailboxKey(owner);
  const senderMb = mailboxKey(senderCred);
  const created = await post(port, '/create', { version: 2, mailbox_key: mb, owner_cred_hex: owner, ...auth(owner, 'create', [mb]) });
  assert.equal(created.status, 200);
  if (grantSender) {
    const expiry = Date.now() + 60_000;
    const registered = await post(port, '/register', {
      version: 2, mailbox_key: mb, sender_mb: senderMb, cred_hex: senderCred, expected_revision: 0, expires_at: expiry,
      ...auth(owner, 'register', [mb, senderMb, senderCred, String(expiry), '0']),
    });
    assert.equal(registered.status, 200);
    assert.equal(registered.body.revision, 1);
    return { owner, mb, senderCred, senderMb, expiry };
  }
  return { owner, mb, senderCred, senderMb };
}

async function sendEnvelope(port, { mb, senderMb, senderCred, expiry, seq = 1, content }) {
  const text = content ?? Buffer.from('synthetic').toString('base64');
  const a = auth(senderCred, 'send', [senderMb, mb, String(seq), String(expiry)]);
  return post(port, '/send', {
    version: 2, mailbox_key: mb, sender_mb: senderMb, ciphertext: text, seq_hint: seq, expires_at: expiry,
    ...a,
    sig: hmac(senderCred, ['relay-v2-send', senderMb, mb, String(seq), String(expiry), String(a.ts), a.nonce, sha256(text)]),
  });
}

test('health responds with protocol version', async () => {
  const s = await startServer();
  try {
    const res = await get(s.port, '/health');
    assert.equal(res.status, 200);
    assert.deepEqual(res.body, { ok: true, service: 'elsewhen-relay', version: 2 });
    assert.equal((await get(s.port, '/presence')).status, 410);
  } finally { s.proc.kill(); }
});

test('mailbox lifecycle: create, status, register, send, fetch, ack', async () => {
  const s = await startServer();
  try {
    const { owner, mb, senderCred, senderMb } = await seedMailbox(s.port, { grantSender: true });
    const expiry = Date.now() + 60_000;
    // 空信箱 status
    const status = await get(s.port, `/status?version=2&mailbox_key=${mb}&${authQuery(owner, 'status', [mb])}`);
    assert.equal(status.status, 200);
    assert.equal(status.body.count, 0);
    assert.equal(status.body.quota, 50 * 1024 * 1024);
    // send
    const envelope = await sendEnvelope(s.port, { mb, senderMb, senderCred, expiry });
    assert.equal(envelope.status, 200);
    const mailId = envelope.body.id;
    assert.match(mailId, /^\d{16}$/);
    // fetch 回来内容一致
    const fetched = await get(s.port, `/fetch?version=2&mailbox_key=${mb}&after=0&limit=20&${authQuery(owner, 'fetch', [mb, '0', '20'])}`);
    assert.equal(fetched.status, 200);
    assert.equal(fetched.body.envelopes.length, 1);
    assert.equal(fetched.body.envelopes[0].ciphertext, Buffer.from('synthetic').toString('base64'));
    assert.equal(fetched.body.envelopes[0].sender_mb, senderMb);
    assert.equal(fetched.body.has_more, false);
    // ack 后再 fetch 为空
    const acked = await post(s.port, '/ack', { version: 2, mailbox_key: mb, ids: [mailId], ...auth(owner, 'ack', [mb, mailId]) });
    assert.equal(acked.status, 200);
    assert.equal(acked.body.deleted, 1);
    const refetched = await get(s.port, `/fetch?version=2&mailbox_key=${mb}&after=0&limit=20&${authQuery(owner, 'fetch', [mb, '0', '20'])}`);
    assert.equal(refetched.body.envelopes.length, 0);
    assert.equal(s.port > 0, true);
  } finally { s.proc.kill(); }
});

test('same (sender_mb, seq_hint) collapses to one mail; seq_hint=0 keeps both', async () => {
  const s = await startServer();
  try {
    const { owner, mb, senderCred, senderMb } = await seedMailbox(s.port, { grantSender: true });
    const expiry = Date.now() + 60_000;
    const first = Buffer.from('first delivery').toString('base64');
    const second = Buffer.from('second delivery').toString('base64');
    // 同一逻辑投递重发两次（丢响应 / 租约恢复的真实形态）：seq_hint 是客户端
    // 签名过的稳定序号，relay 必须把它当幂等键，不能累加副本。
    assert.equal((await sendEnvelope(s.port, { mb, senderMb, senderCred, expiry, seq: 7, content: first })).status, 200);
    assert.equal((await sendEnvelope(s.port, { mb, senderMb, senderCred, expiry, seq: 7, content: second })).status, 200);
    const status = await get(s.port, `/status?version=2&mailbox_key=${mb}&${authQuery(owner, 'status', [mb])}`);
    assert.equal(status.body.count, 1);
    assert.equal(status.body.bytes, Buffer.from(second, 'base64').length);
    // 生效的是最后一次：旧副本被替换而不是被遮蔽
    const fetched = await get(s.port, `/fetch?version=2&mailbox_key=${mb}&after=0&limit=20&${authQuery(owner, 'fetch', [mb, '0', '20'])}`);
    assert.equal(fetched.body.envelopes.length, 1);
    assert.equal(fetched.body.envelopes[0].ciphertext, second);
    // seq_hint=0 表示「本轮没有稳定序号」（配对帧等）：必须允许重复
    const zero = Buffer.from('no sequence').toString('base64');
    assert.equal((await sendEnvelope(s.port, { mb, senderMb, senderCred, expiry, seq: 0, content: zero })).status, 200);
    assert.equal((await sendEnvelope(s.port, { mb, senderMb, senderCred, expiry, seq: 0, content: zero })).status, 200);
    const after = await get(s.port, `/status?version=2&mailbox_key=${mb}&${authQuery(owner, 'status', [mb])}`);
    assert.equal(after.body.count, 3);
  } finally { s.proc.kill(); }
});

test('replayed nonce is rejected with 409', async () => {
  const s = await startServer();
  try {
    const owner = key();
    const mb = mailboxKey(owner);
    const a = auth(owner, 'create', [mb]);
    const body = { version: 2, mailbox_key: mb, owner_cred_hex: owner, ...a };
    assert.equal((await post(s.port, '/create', body)).status, 200);
    const replay = await post(s.port, '/create', body);
    assert.equal(replay.status, 409);
    assert.equal(replay.body.error, 'nonce replay');
  } finally { s.proc.kill(); }
});

test('signature by a different credential is rejected', async () => {
  const s = await startServer();
  try {
    const owner = key(), stranger = key();
    const mb = mailboxKey(owner);
    const res = await post(s.port, '/create', { version: 2, mailbox_key: mb, owner_cred_hex: owner, ...auth(stranger, 'create', [mb]) });
    assert.equal(res.status, 403);
  } finally { s.proc.kill(); }
});

test('register CAS: wrong expected revision rejected, exact retry idempotent', async () => {
  const s = await startServer();
  try {
    const owner = key(), senderCred = key();
    const mb = mailboxKey(owner), senderMb = mailboxKey(senderCred);
    await post(s.port, '/create', { version: 2, mailbox_key: mb, owner_cred_hex: owner, ...auth(owner, 'create', [mb]) });
    const expiry = Date.now() + 60_000;
    const grant = (expected) => post(s.port, '/register', {
      version: 2, mailbox_key: mb, sender_mb: senderMb, cred_hex: senderCred, expected_revision: expected, expires_at: expiry,
      ...auth(owner, 'register', [mb, senderMb, senderCred, String(expiry), String(expected)]),
    });
    assert.equal((await grant(1)).status, 409); // 实际 revision 是 0
    const first = await grant(0);
    assert.equal(first.body.revision, 1);
    const retry = await grant(0); // 丢响应后重试：恰好落在 expected+1 → 幂等回执
    assert.equal(retry.status, 200);
    assert.equal(retry.body.revision, 1);
  } finally { s.proc.kill(); }
});

test('revoked sender cannot send', async () => {
  const s = await startServer();
  try {
    const grant = await seedMailbox(s.port, { grantSender: true });
    const revoked = await post(s.port, '/revoke', {
      version: 2, mailbox_key: grant.mb, sender_mb: grant.senderMb, expected_revision: 1,
      ...auth(grant.owner, 'revoke', [grant.mb, grant.senderMb, '1']),
    });
    assert.equal(revoked.body.revision, 2);
    const res = await sendEnvelope(s.port, { mb: grant.mb, senderMb: grant.senderMb, senderCred: grant.senderCred, expiry: grant.expiry });
    assert.equal(res.status, 403);
    assert.equal(res.body.error, 'sender not authorized');
  } finally { s.proc.kill(); }
});

test('wrong owner credential binding rejected on create', async () => {
  const s = await startServer();
  try {
    const owner = key();
    const wrongBinding = key();
    const mb = mailboxKey(wrongBinding); // cred 与 mailbox 不绑定
    const res = await post(s.port, '/create', { version: 2, mailbox_key: mb, owner_cred_hex: owner, ...auth(owner, 'create', [mb]) });
    assert.equal(res.status, 403);
    assert.equal(res.body.error, 'owner credential does not bind mailbox');
  } finally { s.proc.kill(); }
});


test('drop: owner-signed abandonment deletes only the listed mails, distinct from ack', async () => {
  const s = await startServer();
  try {
    const { owner, mb, senderCred, senderMb } = await seedMailbox(s.port, { grantSender: true });
    const expiry = Date.now() + 60_000;
    const ids = [];
    for (const seq of [1, 2, 3]) {
      const sent = await sendEnvelope(s.port, { mb, senderMb, senderCred, expiry, seq });
      assert.equal(sent.status, 200);
      ids.push(sent.body.id);
    }
    const drop = (body) => post(s.port, '/drop', body);
    // 只删列举的那封，其余原样留着（drop 不是「清空信箱」）。
    const first = await drop({ version: 2, mailbox_key: mb, ids: [ids[1]], ...auth(owner, 'drop', [mb, ids[1]]) });
    assert.equal(first.status, 200);
    assert.equal(first.body.deleted, 1);
    // 幂等：同一封再 drop 一次得 0，不是错误。
    const again = await drop({ version: 2, mailbox_key: mb, ids: [ids[1]], ...auth(owner, 'drop', [mb, ids[1]]) });
    assert.equal(again.status, 200);
    assert.equal(again.body.deleted, 0);
    const status = await get(s.port, `/status?version=2&mailbox_key=${mb}&${authQuery(owner, 'status', [mb])}`);
    assert.equal(status.body.count, 2);
    assert.deepEqual(status.body.capabilities, ['blob', 'drop']);
    // 签名域分离：拿 ack 域的签名去 drop 必须被拒（跨操作重放防线）。
    const crossed = await drop({ version: 2, mailbox_key: mb, ids: [ids[0]], ...auth(owner, 'ack', [mb, ids[0]]) });
    assert.equal(crossed.status, 403);
    // 非 owner 凭据（sender）不能替 owner 放弃。
    const impostor = await drop({ version: 2, mailbox_key: mb, ids: [ids[0]], ...auth(senderCred, 'drop', [mb, ids[0]]) });
    assert.equal(impostor.status, 403);
    // 参数校验与 ack 同形：空、重复、非数字都 400。
    assert.equal((await drop({ version: 2, mailbox_key: mb, ids: [], ...auth(owner, 'drop', [mb, '']) })).status, 400);
    assert.equal((await drop({ version: 2, mailbox_key: mb, ids: [ids[0], ids[0]], ...auth(owner, 'drop', [mb, `${ids[0]},${ids[0]}`]) })).status, 400);
    assert.equal((await drop({ version: 2, mailbox_key: mb, ids: ['not-a-mail'], ...auth(owner, 'drop', [mb, 'not-a-mail']) })).status, 400);
    // 剩下的两封照旧能被 fetch 到 —— drop 没有副作用到扫描游标。
    const fetched = await get(s.port, `/fetch?version=2&mailbox_key=${mb}&after=0&limit=20&${authQuery(owner, 'fetch', [mb, '0', '20'])}`);
    assert.deepEqual(fetched.body.envelopes.map((e) => e.id), [ids[0], ids[2]]);
  } finally { s.proc.kill(); }
});

test('写锁被占住时返回 503 relay_busy + Retry-After，而不是 500', async () => {
  // busy_timeout 调到 50 ms：这条路径要能被确定性地触发，不能真等 5 s。
  const s = await startServer({ SQLITE_BUSY_TIMEOUT_MS: '50' });
  let holder;
  try {
    const { owner, mb, senderCred, senderMb } = await seedMailbox(s.port, { grantSender: true });
    const expiry = Date.now() + 60_000;
    // 另开一个连接抢住写锁（sweeper 的 VACUUM / 另一个请求就是这种形态）。
    holder = new DatabaseSync(join(s.dataDir, 'mailboxes', `${mb}.db`));
    holder.exec('PRAGMA busy_timeout = 50');
    holder.exec('BEGIN IMMEDIATE');
    assert.equal(holder.prepare('SELECT COUNT(*) AS n FROM relay_v2_mail').get().n, 0);

    const blocked = await sendEnvelope(s.port, { mb, senderMb, senderCred, expiry });
    assert.equal(blocked.status, 503, '争用是「等一下再来」，不是「服务器出错」');
    assert.equal(blocked.body.error, 'relay_busy');
    assert.equal(blocked.headers.get('retry-after'), '1');

    // 锁一放就恢复：证明 503 没有副作用，重试就能成功。
    holder.exec('ROLLBACK');
    const sent = await sendEnvelope(s.port, { mb, senderMb, senderCred, expiry });
    assert.equal(sent.status, 200);
    const status = await get(s.port, `/status?version=2&mailbox_key=${mb}&${authQuery(owner, 'status', [mb])}`);
    assert.equal(status.body.count, 1);
  } finally {
    try { holder?.exec('ROLLBACK'); } catch { /* 已回滚 */ }
    try { holder?.close(); } catch { /* 已关闭 */ }
    s.proc.kill();
  }
});
