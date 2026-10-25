import { createHash, createHmac, randomBytes } from 'node:crypto';
import { spawn } from 'node:child_process';
import { mkdtempSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

export const key = () => randomBytes(32).toString('hex');
export const hmac = (key, parts) => createHmac('sha256', Buffer.from(key, 'hex')).update(parts.join('|')).digest('hex');
export const sha256 = value => createHash('sha256').update(value).digest('hex');
export const mailbox = credential => hmac(credential, ['relay-mailbox']);
export function auth(credential, domain, parts, overrides = {}) {
  const ts = overrides.ts ?? Date.now();
  const nonce = overrides.nonce ?? randomBytes(16).toString('hex');
  return { ts, nonce, sig: hmac(credential, [`relay-v2-${domain}`, ...parts, String(ts), nonce]) };
}
export function createBody(credential) {
  const mb = mailbox(credential);
  return { version: 2, mailbox_key: mb, owner_cred_hex: credential, ...auth(credential, 'create', [mb]) };
}
export function registerBody(owner, mb, sender, credential, expected = 0, expiry = Date.now() + 60_000) {
  return {
    version: 2, mailbox_key: mb, sender_mb: sender, cred_hex: credential,
    expected_revision: expected, expires_at: expiry,
    ...auth(owner, 'register', [mb, sender, credential, String(expiry), String(expected)]),
  };
}
export function sendBody(credential, mb, sender, { content = 'c3ludGhldGlj', seq = 1, expiry = Date.now() + 60_000, ...overrides } = {}) {
  const authentication = auth(credential, 'send', [sender, mb, String(seq), String(expiry)], overrides);
  authentication.sig = hmac(credential, ['relay-v2-send', sender, mb, String(seq), String(expiry), String(authentication.ts), authentication.nonce, sha256(content)]);
  return { version: 2, mailbox_key: mb, sender_mb: sender, ciphertext: content, seq_hint: seq, expires_at: expiry, ...authentication };
}
export function fetchBody(owner, mb, after = '0', limit = 20) {
  return { version: 2, mailbox_key: mb, after, limit, ...auth(owner, 'fetch', [mb, after, String(limit)]) };
}
export function ackBody(owner, mb, ids) {
  return { version: 2, mailbox_key: mb, ids, ...auth(owner, 'ack', [mb, ids.join(',')]) };
}
export function statusBody(owner, mb) {
  return { version: 2, mailbox_key: mb, ...auth(owner, 'status', [mb]) };
}
export function revokeBody(owner, mb, sender, revision) {
  return { version: 2, mailbox_key: mb, sender_mb: sender, expected_revision: revision, ...auth(owner, 'revoke', [mb, sender, String(revision)]) };
}
export function rotateBody(owner, mb, replacement) {
  return { version: 2, mailbox_key: mb, new_cred_hex: replacement, ...auth(owner, 'rotate', [mb, replacement]) };
}

export async function startRelay({ bindings = {}, persist } = {}) {
  // 只支持 standalone 版（relay/standalone/server.mjs）：Cloudflare Worker 实现已移除。
  // 保留 runtime.ready / runtime.dispose() 两个形状，使调用方无需改动。
  const dataDir = persist ?? mkdtempSync(join(tmpdir(), 'relay-test-data-'));
  const server = join(dirname(fileURLToPath(import.meta.url)), '..', 'standalone', 'server.mjs');
  const proc = spawn(process.execPath, [server], {
    env: {
      ...process.env,
      PORT: '0',
      DATA_DIR: dataDir,
      MAIL_TTL_MS: '604800000',
      MAILBOX_QUOTA_BYTES: '52428800',
      MAX_ENVELOPE_BYTES: '8388608',
      ...bindings,
    },
    stdio: ['ignore', 'pipe', 'inherit'],
  });
  const ready = new Promise((resolve, reject) => {
    proc.stdout.on('data', chunk => {
      const match = /listening on [^\n]*:(\d+)/.exec(String(chunk));
      if (match) resolve(`http://127.0.0.1:${match[1]}`);
    });
    proc.on('exit', code => reject(new Error(`relay exited early: ${code}`)));
  });
  const runtime = {
    ready,
    dispose() { proc.kill('SIGKILL'); },
    // 兼容旧 Miniflare 形状：直接向服务进程发原始请求，返回 undici Response。
    async dispatchFetch(path, init = {}) {
      const base = await ready;
      return fetch(`${base}${path.startsWith('http') ? new URL(path).pathname + new URL(path).search : path}`, init);
    },
  };
  return {
    runtime,
    async call(path, body = {}, get = false) {
      const base = await ready;
      const url = get ? `${base}${path}?${new URLSearchParams(body)}` : `${base}${path}`;
      const response = await fetch(url, get ? {} : {
        method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify(body),
      });
      return { status: response.status, body: await response.json() };
    },
  };
}
export async function paired(relay) {
  const owner = key(), senderOwner = key(), credential = key();
  const mb = mailbox(owner), sender = mailbox(senderOwner);
  const created = await relay.call('/create', createBody(owner));
  if (created.status !== 200) throw new Error(`create failed: ${created.status}`);
  const registered = await relay.call('/register', registerBody(owner, mb, sender, credential));
  if (registered.status !== 200) throw new Error(`register failed: ${registered.status}`);
  return { owner, mb, sender, credential };
}
