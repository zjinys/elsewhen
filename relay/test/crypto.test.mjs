// These tests bundle the production Worker and execute it in workerd with
// separate SQLite-backed Durable Objects. No mirrored mailbox implementation.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { mkdtemp, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import {
  key, mailbox, auth, startRelay, paired, createBody, registerBody,
  sendBody, fetchBody, ackBody, statusBody, revokeBody, rotateBody,
} from './helpers.mjs';

async function fixture(t, options) {
  const relay = await startRelay(options);
  t.after(() => relay.runtime.dispose());
  return relay;
}

test('owner proof binds mailbox; v1 registration and unapproved senders fail', async t => {
  const relay = await fixture(t);
  const owner = key(), other = key(), mb = mailbox(owner), sender = mailbox(other);
  const forged = createBody(other);
  forged.mailbox_key = mb;
  Object.assign(forged, auth(other, 'create', [mb]));
  assert.equal((await relay.call('/create', forged)).status, 403);
  assert.equal((await relay.call('/create', createBody(owner))).status, 200);
  assert.equal((await relay.call('/create', createBody(owner))).status, 200, 'safe owner retry');
  const ownRegistration = registerBody(other, mb, sender, key());
  assert.equal((await relay.call('/register', ownRegistration)).status, 403);
  const v1 = { ...ownRegistration, version: 1 };
  assert.equal((await relay.call('/register', v1)).status, 426);
  assert.equal((await relay.call('/send', sendBody(other, mb, sender))).status, 403);
});

test('sender credentials cannot read, delete, inspect, register or rotate the recipient mailbox', async t => {
  const relay = await fixture(t);
  const { owner, mb, sender, credential } = await paired(relay);
  const sent = await relay.call('/send', sendBody(credential, mb, sender));
  assert.equal(sent.status, 200);
  for (const recipient of [undefined, sender, mb]) {
    const request = fetchBody(credential, mb);
    if (recipient !== undefined) request.recipient_mb = recipient;
    assert.equal((await relay.call('/fetch', request, true)).status, 403);
  }
  assert.equal((await relay.call('/ack', ackBody(credential, mb, [sent.body.id]))).status, 403);
  assert.equal((await relay.call('/status', statusBody(credential, mb), true)).status, 403);
  assert.equal((await relay.call('/register', registerBody(credential, mb, mailbox(key()), key()))).status, 403);
  assert.equal((await relay.call('/rotate', rotateBody(credential, mb, key()))).status, 403);
  assert.equal((await relay.call('/status', statusBody(owner, mb), true)).body.count, 1);
});

test('owner access and sender grants are isolated across mailbox objects', async t => {
  const relay = await fixture(t);
  const a = await paired(relay), b = await paired(relay);
  assert.equal((await relay.call('/send', sendBody(a.credential, b.mb, a.sender))).status, 403);
  assert.equal((await relay.call('/fetch', fetchBody(a.owner, b.mb), true)).status, 403);
  assert.equal((await relay.call('/register', registerBody(a.owner, b.mb, a.sender, a.credential))).status, 403);
  assert.equal((await relay.call('/status', statusBody(b.owner, b.mb), true)).body.count, 0);
});

test('raw requested TTL is signed before clamping; pagination is exclusive including limit one', async t => {
  const relay = await fixture(t, { bindings: { MAIL_TTL_MS: '60000' } });
  const { owner, mb, sender, credential } = await paired(relay);
  const ids = [];
  for (let seq = 1; seq <= 3; seq++) {
    const sent = await relay.call('/send', sendBody(credential, mb, sender, { seq, expiry: Date.now() + 86_400_000 }));
    assert.equal(sent.status, 200);
    ids.push(sent.body.id);
  }
  let after = '0';
  for (let i = 0; i < 3; i++) {
    const page = await relay.call('/fetch', fetchBody(owner, mb, after, 1), true);
    assert.equal(page.status, 200);
    assert.deepEqual(page.body.envelopes.map(e => e.id), [ids[i]]);
    assert.equal(page.body.has_more, i !== 2);
    assert.equal(page.body.next_cursor, ids[i]);
    assert.ok(page.body.envelopes[0].expires_at <= Date.now() + 60_000);
    after = page.body.next_cursor;
  }
  const empty = await relay.call('/fetch', fetchBody(owner, mb, after, 1), true);
  assert.deepEqual(empty.body.envelopes, []);
  assert.equal(empty.body.has_more, false);
  assert.equal(empty.body.next_cursor, after);
});

test('sender rotation and revocation are version-fenced; owner rotation retains mailbox identity', async t => {
  const relay = await fixture(t);
  const { owner, mb, sender, credential } = await paired(relay);
  const next = key();
  assert.equal((await relay.call('/register', registerBody(owner, mb, sender, next, 0))).status, 409);
  assert.equal((await relay.call('/register', registerBody(owner, mb, sender, next, 1))).body.revision, 2);
  assert.equal((await relay.call('/send', sendBody(credential, mb, sender))).status, 403);
  assert.equal((await relay.call('/send', sendBody(next, mb, sender))).status, 200);
  assert.equal((await relay.call('/revoke', revokeBody(owner, mb, sender, 2))).body.revision, 3);
  assert.equal((await relay.call('/send', sendBody(next, mb, sender))).status, 403);
  assert.equal((await relay.call('/register', registerBody(owner, mb, sender, credential, 0))).status, 409);
  const state = await relay.call('/sender', { version: 2, mailbox_key: mb, sender_mb: sender, ...auth(owner, 'sender', [mb, sender]) }, true);
  assert.deepEqual(state.body, { revision: 3, active: false, expires_at: 0 });
  const newOwner = key();
  assert.equal((await relay.call('/rotate', rotateBody(owner, mb, newOwner))).body.owner_generation, 2);
  assert.equal((await relay.call('/status', statusBody(owner, mb), true)).status, 403);
  assert.equal((await relay.call('/status', statusBody(newOwner, mb), true)).status, 200);
  assert.equal((await relay.call('/create', createBody(owner))).status, 409, 'old owner cannot reset rotated mailbox');
});

test('lost grant and revoke responses retry only the exact post-state across restart', async t => {
  const persist = await mkdtemp(join(tmpdir(), 'elsewhen-relay-retry-'));
  t.after(() => rm(persist, { recursive: true, force: true }));
  let relay = await startRelay({ persist });
  const owner = key(), mb = mailbox(owner), sender = mailbox(key()), credential = key();
  const expiry = Date.now() + 3_600_000;
  try {
    assert.equal((await relay.call('/create', createBody(owner))).status, 200);
    // Each retry is freshly signed: replaying the same HTTP nonce still fails.
    const committed = await Promise.all(Array.from({ length: 8 }, () =>
      relay.call('/register', registerBody(owner, mb, sender, credential, 0, expiry))));
    assert.ok(committed.every(r => r.status === 200 && r.body.revision === 1));
    await relay.runtime.dispose();
    relay = await startRelay({ persist });
    assert.equal((await relay.call('/register', registerBody(owner, mb, sender, credential, 0, expiry))).body.revision, 1);
    assert.equal((await relay.call('/register', registerBody(owner, mb, sender, key(), 0, expiry))).status, 409);
    assert.equal((await relay.call('/register', registerBody(owner, mb, sender, credential, 0, expiry + 1))).status, 409);
    const revoked = await Promise.all(Array.from({ length: 8 }, () =>
      relay.call('/revoke', revokeBody(owner, mb, sender, 1))));
    assert.ok(revoked.every(r => r.status === 200 && r.body.revision === 2));
    assert.equal((await relay.call('/register', registerBody(owner, mb, sender, credential, 0, expiry))).status, 409);
    assert.equal((await relay.call('/send', sendBody(credential, mb, sender))).status, 403);
    await relay.runtime.dispose();
    relay = await startRelay({ persist });
    assert.equal((await relay.call('/revoke', revokeBody(owner, mb, sender, 1))).body.revision, 2);
    const replacement = key();
    assert.equal((await relay.call('/register', registerBody(owner, mb, sender, replacement, 2, expiry))).body.revision, 3);
    assert.equal((await relay.call('/revoke', revokeBody(owner, mb, sender, 1))).status, 409);
    assert.equal((await relay.call('/register', registerBody(owner, mb, sender, credential, 0, expiry))).status, 409);
    assert.equal((await relay.call('/send', sendBody(replacement, mb, sender))).status, 200);
  } finally { await relay.runtime.dispose(); }
});

test('concurrent nonce replay accepts one request and quota remains exact', async t => {
  const relay = await fixture(t, { bindings: { MAILBOX_QUOTA_BYTES: '10' } });
  const { owner, mb, sender, credential } = await paired(relay);
  const request = sendBody(credential, mb, sender, { content: Buffer.alloc(6).toString('base64') });
  const replayed = await Promise.all(Array.from({ length: 10 }, () => relay.call('/send', request)));
  assert.equal(replayed.filter(r => r.status === 200).length, 1);
  assert.equal(replayed.filter(r => r.status === 409).length, 9);
    // seq 必须换新号：R2-1 之后同一 (sender,seq) 被视为同一封逻辑邮件、直接替换
  // 而不是新增——用 seq=1 重发会在配额账上走替换路径拿到 200，测不出 507。
  const overflow = await relay.call('/send', sendBody(credential, mb, sender, { seq: 2, content: Buffer.alloc(5).toString('base64') }));
  assert.equal(overflow.status, 507);
  const status = await relay.call('/status', statusBody(owner, mb), true);
  assert.equal(status.body.bytes, 6);
  assert.equal(status.body.count, 1);
  const id = replayed.find(r => r.status === 200).body.id;
  assert.equal((await relay.call('/ack', ackBody(owner, mb, [id, id]))).status, 400);
  assert.equal((await relay.call('/ack', ackBody(owner, mb, [id]))).body.deleted, 1);
  assert.equal((await relay.call('/ack', ackBody(owner, mb, [id]))).body.deleted, 0);
  assert.equal((await relay.call('/status', statusBody(owner, mb), true)).body.bytes, 0);
});

test('concurrent distinct sends retain unique IDs and exact aggregate accounting', async t => {
  const relay = await fixture(t);
  const { owner, mb, sender, credential } = await paired(relay);
  const sent = await Promise.all(Array.from({ length: 32 }, (_, seq) => relay.call('/send', sendBody(credential, mb, sender, { seq, content: 'YQ==' }))));
  assert.ok(sent.every(r => r.status === 200));
  assert.equal(new Set(sent.map(r => r.body.id)).size, 32);
  const status = await relay.call('/status', statusBody(owner, mb), true);
  assert.equal(status.body.bytes, 32);
  assert.equal(status.body.count, 32);
});

test('malformed numbers, cursors, payloads, signatures and request sizes fail safely', async t => {
  const relay = await fixture(t, { bindings: { MAX_ENVELOPE_BYTES: '32' } });
  const { owner, mb, sender, credential } = await paired(relay);
  for (const limit of [0, -1, 101, 1.5, 'NaN']) {
    assert.equal((await relay.call('/fetch', fetchBody(owner, mb, '0', limit), true)).status, 400);
  }
  for (const after of ['1', 'x', '9999999999999999', '1|2']) {
    assert.equal((await relay.call('/fetch', fetchBody(owner, mb, after), true)).status, 400);
  }
  for (const content of ['', 'x', 'a===', '=aaa', 'éééé']) {
    assert.equal((await relay.call('/send', sendBody(credential, mb, sender, { content }))).status, 400);
  }
  assert.equal((await relay.call('/send', sendBody(credential, mb, sender, { content: Buffer.alloc(33).toString('base64') }))).status, 413);
  assert.equal((await relay.call('/send', sendBody(credential, mb, sender, { ts: Date.now() - 301_000 }))).status, 403);
  assert.equal((await relay.call('/send', sendBody(credential, mb, sender, { expiry: Date.now() - 1 }))).status, 400);
  const tampered = sendBody(credential, mb, sender); tampered.seq_hint++;
  assert.equal((await relay.call('/send', tampered)).status, 403);
  const raw = await relay.runtime.dispatchFetch('http://relay.test/create', { method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify({ ...createBody(key()), junk: 'x'.repeat(33000) }) });
  assert.equal(raw.status, 413);
  const duplicate = await relay.runtime.dispatchFetch('http://relay.test/fetch?' + new URLSearchParams(fetchBody(owner, mb)) + '&limit=1');
  assert.equal(duplicate.status, 400);
});

test('large envelopes use chunk rows and bounded response pages, not oversized KV values', async t => {
  const relay = await fixture(t);
  const { owner, mb, sender, credential } = await paired(relay);
  const content = Buffer.alloc(8 * 1024 * 1024, 7).toString('base64');
  for (let seq = 0; seq < 2; seq++) {
    assert.equal((await relay.call('/send', sendBody(credential, mb, sender, { seq, content }))).status, 200);
  }
  const first = await relay.call('/fetch', fetchBody(owner, mb, '0', 100), true);
  assert.equal(first.body.envelopes.length, 1);
  assert.equal(first.body.envelopes[0].ciphertext, content);
  assert.equal(first.body.has_more, true);
  const second = await relay.call('/fetch', fetchBody(owner, mb, first.body.next_cursor, 100), true);
  assert.equal(second.body.envelopes.length, 1);
  assert.equal(second.body.has_more, false);
});

test('expired mail releases chunks and quota; expired sender remains unauthorized', async t => {
  const relay = await fixture(t, { bindings: { MAIL_TTL_MS: '20', MAILBOX_QUOTA_BYTES: '9' } });
  const { owner, mb, sender, credential } = await paired(relay);
  assert.equal((await relay.call('/send', sendBody(credential, mb, sender))).status, 200);
  await new Promise(resolve => setTimeout(resolve, 30));
  assert.deepEqual((await relay.call('/fetch', fetchBody(owner, mb), true)).body.envelopes, []);
  assert.equal((await relay.call('/status', statusBody(owner, mb), true)).body.bytes, 0);
  assert.equal((await relay.call('/register', registerBody(owner, mb, sender, credential, 1, Date.now() + 80))).status, 200);
  await new Promise(resolve => setTimeout(resolve, 100));
  assert.equal((await relay.call('/send', sendBody(credential, mb, sender))).status, 403);
});

test('state survives runtime restart: rotated owner, revoked grant and mail remain consistent', async t => {
  const persist = await mkdtemp(join(tmpdir(), 'elsewhen-relay-v2-'));
  t.after(() => rm(persist, { recursive: true, force: true }));
  let relay = await startRelay({ persist });
  const { owner, mb, sender, credential } = await paired(relay);
  const sent = await relay.call('/send', sendBody(credential, mb, sender));
  const newOwner = key();
  assert.equal((await relay.call('/revoke', revokeBody(owner, mb, sender, 1))).status, 200);
  assert.equal((await relay.call('/rotate', rotateBody(owner, mb, newOwner))).status, 200);
  await relay.runtime.dispose();
  relay = await startRelay({ persist });
  try {
    assert.equal((await relay.call('/status', statusBody(owner, mb), true)).status, 403);
    assert.equal((await relay.call('/send', sendBody(credential, mb, sender))).status, 403);
    const page = await relay.call('/fetch', fetchBody(newOwner, mb), true);
    assert.deepEqual(page.body.envelopes.map(e => e.id), [sent.body.id]);
    assert.equal((await relay.call('/ack', ackBody(newOwner, mb, [sent.body.id]))).body.deleted, 1);
  } finally { await relay.runtime.dispose(); }
});

test('unsupported discovery fails explicitly rather than claiming no peers are online', async t => {
  const relay = await fixture(t);
  assert.equal((await relay.call('/peers', {}, true)).status, 410);
  assert.equal((await relay.call('/presence', {})).status, 410);
});
