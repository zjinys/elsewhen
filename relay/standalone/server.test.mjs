/**
 * standalone 中继端到端冒烟：协议语义与 CF 版一致。
 * 覆盖：health、create→status→register→send→fetch→ack 生命周期、
 *       nonce 重放拒绝、坏签名拒绝、撤销后 send 拒绝、
 *       owner 授权 CAS 幂等回执、release 分发端点。
 */
import test from 'node:test';
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { mkdtempSync, mkdirSync, writeFileSync } from 'node:fs';
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
  const releasesDir = mkdtempSync(join(tmpdir(), 'relay-rel-'));
  const proc = spawn(process.execPath, [join(here, 'server.mjs')], {
    env: { ...process.env, PORT: '0', DATA_DIR: dataDir, RELEASES_DIR: releasesDir, ...extraEnv },
    stdio: ['ignore', 'pipe', 'inherit'],
  });
  const port = await new Promise((resolve, reject) => {
    proc.stdout.on('data', (chunk) => {
      const match = /listening on [^\n]*:(\d+)/.exec(String(chunk));
      if (match) resolve(Number(match[1]));
    });
    proc.on('exit', (code) => reject(new Error(`relay exited early: ${code}`)));
  });
  return { proc, port, releasesDir };
}

async function post(port, path, body) {
  const res = await fetch(`http://127.0.0.1:${port}${path}`, {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify(body),
  });
  return { status: res.status, body: await res.json() };
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

test('release endpoints: 404 before publish, manifest and apk after', async () => {
  const s = await startServer();
  try {
    assert.equal((await get(s.port, '/release/latest')).status, 404);
    mkdirSync(join(s.releasesDir, 'release'), { recursive: true });
    mkdirSync(join(s.releasesDir, 'apk'), { recursive: true });
    const manifest = { version: '1.2.3', hash: 'abcdef123', notes: 'test' };
    writeFileSync(join(s.releasesDir, 'release', 'manifest.json'), JSON.stringify(manifest));
    const latest = await get(s.port, '/release/latest');
    assert.equal(latest.status, 200);
    assert.deepEqual(latest.body, manifest);
    assert.match(latest.headers.get('content-type'), /application\/json/);
    // APK：hash 非法 400；合法但缺失 404；存在则流式返回
    assert.equal((await get(s.port, '/release/apk?h=**')).status, 400);
    assert.equal((await get(s.port, '/release/apk?h=abcdef123')).status, 404);
    writeFileSync(join(s.releasesDir, 'apk', 'abcdef123.apk'), Buffer.from('APKBYTES'));
    const apk = await get(s.port, '/release/apk?h=abcdef123');
    assert.equal(apk.status, 200);
    assert.equal(apk.body, 'APKBYTES');
    assert.equal(apk.headers.get('content-type'), 'application/vnd.android.package-archive');
  } finally { s.proc.kill(); }
});
