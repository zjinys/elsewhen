#!/usr/bin/env node
// Explicit relay-v2 smoke test. Synthetic credentials only; never reads app data.
// Run against an isolated deployment. Credential rows remain after mail cleanup.
import assert from 'node:assert/strict';
import { createHmac, randomBytes } from 'node:crypto';
import { createHash } from 'node:crypto';

const base = process.argv[2]?.replace(/\/$/, '');
if (!base) {
  console.error('Usage: node test/e2e-smoke.mjs <isolated-relay-url>');
  process.exit(2);
}
const parsed = new URL(base);
assert.ok(parsed.protocol === 'https:' || (parsed.protocol === 'http:' && ['localhost', '127.0.0.1', '[::1]'].includes(parsed.hostname)));
assert.ok(!parsed.username && !parsed.password && !parsed.search && !parsed.hash);
const key = () => randomBytes(32).toString('hex');
const hmac = (key, parts) => createHmac('sha256', Buffer.from(key, 'hex')).update(parts.join('|')).digest('hex');
const mailbox = key => hmac(key, ['relay-mailbox']);
const auth = (key, domain, parts) => {
  const ts = Date.now(), nonce = randomBytes(16).toString('hex');
  return { ts, nonce, sig: hmac(key, [`relay-v2-${domain}`, ...parts, String(ts), nonce]) };
};
async function call(path, body, get = false) {
  const response = await fetch(base + path + (get ? '?' + new URLSearchParams(body) : ''), get ? { redirect: 'error' } : {
    method: 'POST', redirect: 'error', headers: { 'content-type': 'application/json' }, body: JSON.stringify(body),
  });
  return { status: response.status, body: await response.json() };
}
const owner = key(), senderOwner = key(), credential = key();
const mb = mailbox(owner), sender = mailbox(senderOwner);
const created = await call('/create', { version: 2, mailbox_key: mb, owner_cred_hex: owner, ...auth(owner, 'create', [mb]) });
assert.equal(created.status, 200);
const expires = Date.now() + 60_000;
const registered = await call('/register', {
  version: 2, mailbox_key: mb, sender_mb: sender, cred_hex: credential, expires_at: expires, expected_revision: 0,
  ...auth(owner, 'register', [mb, sender, credential, String(expires), '0']),
});
assert.equal(registered.status, 200);
const ciphertext = Buffer.from('synthetic opaque smoke payload; not personal data').toString('base64');
const stamp = auth(credential, 'send', [sender, mb, '1', String(expires)]);
stamp.sig = hmac(credential, ['relay-v2-send', sender, mb, '1', String(expires), String(stamp.ts), stamp.nonce, createHash('sha256').update(ciphertext).digest('hex')]);
const sent = await call('/send', { version: 2, mailbox_key: mb, sender_mb: sender, seq_hint: 1, ciphertext, expires_at: expires, ...stamp });
assert.equal(sent.status, 200);
const denied = await call('/fetch', { version: 2, mailbox_key: mb, after: '0', limit: 1, ...auth(credential, 'fetch', [mb, '0', '1']) }, true);
assert.equal(denied.status, 403);
const fetched = await call('/fetch', { version: 2, mailbox_key: mb, after: '0', limit: 1, ...auth(owner, 'fetch', [mb, '0', '1']) }, true);
assert.equal(fetched.status, 200);
assert.equal(fetched.body.envelopes[0].ciphertext, ciphertext);
const acked = await call('/ack', { version: 2, mailbox_key: mb, ids: [sent.body.id], ...auth(owner, 'ack', [mb, sent.body.id]) });
assert.equal(acked.body.deleted, 1);
const revoked = await call('/revoke', { version: 2, mailbox_key: mb, sender_mb: sender, expected_revision: 1, ...auth(owner, 'revoke', [mb, sender, '1']) });
assert.equal(revoked.status, 200);
console.log('Relay v2 synthetic owner/grant/send/fetch/ack/revoke smoke passed. This does not test pairing or database synchronization.');
