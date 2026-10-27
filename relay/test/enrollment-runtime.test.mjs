import assert from 'node:assert/strict';
import { test } from 'node:test';
import { createServer } from 'node:http';
import { execFile } from 'node:child_process';
import { promisify } from 'node:util';
import { fileURLToPath } from 'node:url';
import { startRelay } from './helpers.mjs';

const run = promisify(execFile);

test('P2 production runtimes recover committed grants and receipts after lost responses', { timeout: 240_000 }, async t => {
  const relay = await startRelay();
  t.after(() => relay.runtime.dispose());
  const upstream = await relay.runtime.ready;
  const requests = new Map();
  const lost = new Set();
  const failures = [];
  const counts = { grant: 0, revoke: 0, receipt: 0 };
  const server = createServer(async (request, response) => {
    try {
      const chunks = [];
      let length = 0;
      for await (const chunk of request) {
        length += chunk.length;
        assert.ok(length <= 16 * 1024 * 1024);
        chunks.push(chunk);
      }
      const bytes = Buffer.concat(chunks);
      const body = bytes.length ? JSON.parse(bytes.toString()) : null;
      const path = new URL(request.url, upstream).pathname;
      let kind;
      let identity;
      if (path === '/register' && body.expires_at > Date.now() + 86_400_000) {
        kind = 'grant';
        identity = `${path}:${body.mailbox_key}:${body.sender_mb}`;
      } else if (path === '/revoke') {
        kind = 'revoke';
        identity = `${path}:${body.mailbox_key}:${body.sender_mb}`;
      } else if (path === '/send') {
        try {
          const wire = JSON.parse(Buffer.from(body.ciphertext, 'base64').toString());
          if (wire.header?.message_id?.startsWith('envelope-receipt-')) {
            kind = 'receipt';
            identity = `${path}:${body.mailbox_key}:${wire.header.message_id}`;
          }
        } catch { /* P1 Noise frames are not enrollment receipt envelopes. */ }
      }
      if (kind === 'grant' || kind === 'revoke') {
        const original = {
          mailbox: body.mailbox_key, sender: body.sender_mb,
          revision: body.expected_revision, credential: body.cred_hex,
          expiry: body.expires_at,
        };
        const prior = requests.get(identity);
        if (prior) {
          assert.deepEqual(original, prior.original, 'retry must preserve the original grant/revoke intent');
          assert.ok(!prior.nonces.has(body.nonce), 'HTTP retry must use a fresh nonce');
          prior.nonces.add(body.nonce);
        } else {
          requests.set(identity, { original, nonces: new Set([body.nonce]) });
        }
      }
      const result = await fetch(new URL(request.url, upstream), {
        method: request.method,
        headers: { 'content-type': 'application/json' },
        ...(bytes.length ? { body: bytes } : {}),
      });
      const payload = Buffer.from(await result.arrayBuffer());
      // 头必须原样转发：blob 数据面的窗口游标就在 `x-blob-from/-count/-next` 上
      // （不走 base64 JSON 是为省 1.37× 膨胀）。丢掉这两颗头，接收端就会把
      // 「这一窗有 10 片」当成「relay 上什么都没有」，每拍空转到天荒。
      const headers = {};
      result.headers.forEach((value, name) => {
        if (['content-length', 'content-encoding', 'transfer-encoding'].includes(name)) return;
        headers[name] = value;
      });
      if (kind && result.ok && !lost.has(identity)) {
        lost.add(identity);
        counts[kind]++;
        // The actual Worker committed first; only its response is lost.
        response.writeHead(503, { 'content-type': 'application/json' });
        response.end('{"error":"synthetic lost response"}');
      } else {
        response.writeHead(result.status, headers);
        response.end(payload);
      }
    } catch (error) {
      failures.push(error);
      response.writeHead(500, { 'content-type': 'application/json' });
      response.end('{"error":"synthetic proxy failure"}');
    }
  });
  await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
  t.after(() => new Promise(resolve => {
    server.closeAllConnections();
    server.close(resolve);
  }));
  const url = `http://127.0.0.1:${server.address().port}`;
  await run('cargo', [
    'test', '--lib',
    'sync::transport_relay_smoke_tests::enrollment_ui::production_enrollment_recovers_grants_native_writes_and_receipts',
    '--', '--ignored', '--exact',
  ], {
    cwd: fileURLToPath(new URL('../../', import.meta.url)),
    env: {
      ...process.env, ELSEWHEN_RELAY_TEST_URL: url,
      NO_PROXY: 'localhost,127.0.0.1,::1', no_proxy: 'localhost,127.0.0.1,::1',
    },
    timeout: 220_000,
    maxBuffer: 2 * 1024 * 1024,
  });
  assert.deepEqual(failures, []);
  assert.equal(counts.grant, 2);
  assert.equal(counts.revoke, 1);
  assert.ok(counts.receipt >= 4, 'roster, selection, application and completion receipts must exercise lost responses');
});
