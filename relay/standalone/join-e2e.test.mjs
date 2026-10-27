/**
 * 加入流程端到端集成测试：standalone 中继 × 真实 Rust runtime。
 *
 * 与 relay/test/*.test.mjs 的 miniflare 驱动器完全同构——只是被测对象换成
 * standalone server.mjs（NAS 部署的同款代码）。跑的是 Rust 侧 #[ignore]
 * 全流程大测试：
 *   1. smoke：owner/joiner 双 runtime 完整加入过程（邀请→审批→安装→
 *      enrollment→membership→数据增量收发），外加 owner/joiner/membership
 *      的 UI 操作 roundtrip。
 *   2. enrollment：丢响应后授权/回执恢复（fault proxy 注入）。
 *   3. network tick：配对完成后稳态增量同步。
 *
 * 需要本机可 cargo test（首次编译阀值较长）。
 */
import test from 'node:test';
import assert from 'node:assert/strict';
import { execFile, spawn } from 'node:child_process';
import { createServer } from 'node:http';
import { promisify } from 'node:util';
import { mkdtempSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const repoRoot = join(here, '..', '..');
const run = promisify(execFile);

async function startServer() {
  const dataDir = mkdtempSync(join(tmpdir(), 'relay-e2e-data-'));
  const proc = spawn(process.execPath, [join(here, 'server.mjs')], {
    env: { ...process.env, PORT: '0', DATA_DIR: dataDir, RELEASES_DIR: join(dataDir, 'releases') },
    stdio: ['ignore', 'pipe', 'inherit'],
  });
  const url = await new Promise((resolve, reject) => {
    proc.stdout.on('data', (chunk) => {
      const match = /listening on [^\n]*:(\d+)/.exec(String(chunk));
      if (match) resolve(`http://127.0.0.1:${match[1]}`);
    });
    proc.on('exit', (code) => reject(new Error(`relay exited early: ${code}`)));
  });
  return { proc, url };
}

/**
 * 丢包代理：完整转发到 relay，但对 grant/revoke/receipt 的**首次成功响应**
 * 伪造 503（服务端实际已提交），驱动「丢响应后恢复」路径。
 * 断言与 miniflare 驱动器一致：grant 丢 2、revoke 丢 1、receipt 丢 >=4。
 */
async function withFaultProxy(relayUrl, apply) {
  const requests = new Map();
  const lost = new Set();
  const counts = { grant: 0, revoke: 0, receipt: 0 };
  const failures = [];
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
      const path = new URL(request.url, relayUrl).pathname;
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
      console.error(`[proxy] ${request.method} ${path} body=${body ? Object.keys(body).join(',') : '-'} loss=${kind || '-'}`);
      const result = await fetch(new URL(request.url, relayUrl), {
        method: request.method,
        headers: { 'content-type': 'application/json' },
        ...(bytes.length ? { body: bytes } : {}),
      });
      const payload = Buffer.from(await result.arrayBuffer());
      // 头必须原样转发：blob 数据面的窗口游标在 `x-blob-from/-count/-next` 上，
      // 丢掉它接收端会把「这一窗有 10 片」当成「什么都没有」。
      const headers = {};
      result.headers.forEach((value, name) => {
        if (['content-length', 'content-encoding', 'transfer-encoding'].includes(name)) return;
        headers[name] = value;
      });
      if (kind && result.ok && !lost.has(identity)) {
        lost.add(identity);
        counts[kind]++;
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
  await new Promise((resolve) => server.listen(0, '127.0.0.1', resolve));
  const proxyUrl = `http://127.0.0.1:${server.address().port}`;
  try {
    await apply(proxyUrl);
    assert.deepEqual(failures, [], 'fault proxy must not error');
    assert.equal(counts.grant, 2);
    assert.equal(counts.revoke, 1);
    assert.ok(counts.receipt >= 4, 'roster, selection, application and completion receipts must exercise lost responses');
  } finally {
    await new Promise((resolve) => {
      server.closeAllConnections();
      server.close(resolve);
    });
  }
}

async function cargoTest(testName, relayUrl) {
  const env = {
    ...process.env,
    ELSEWHEN_RELAY_TEST_URL: relayUrl,
    NO_PROXY: 'localhost,127.0.0.1,::1',
    no_proxy: 'localhost,127.0.0.1,::1',
  };
  await run('cargo', ['test', '--lib', testName, '--', '--ignored', '--exact'], {
    cwd: repoRoot,
    env,
    timeout: 600_000,
    maxBuffer: 8 * 1024 * 1024,
  });
}

test('full join flow against standalone relay (double runtime pairing + enrollment)', { timeout: 900_000 }, async (t) => {
  const server = await startServer();
  t.after(() => server.proc.kill());
  const steps = [
    // 完整加入过程：邀请、审批、安装确认、初始同步与角色/榜单流程。
    'sync::transport_relay_smoke_tests::smoke::end_to_end_send_fetch_ack_against_real_worker',
    // 丢响应恢复：授权与回执在网络故障注入后重投收敛（走丢包代理）。
    'sync::transport_relay_smoke_tests::enrollment_ui::production_enrollment_recovers_grants_native_writes_and_receipts',
    // 稳态：配对完成后网络 tick 在设备间搬运增量，不再走配对信箱。
    'sync::runtime::network::tests::tick_moves_deltas_between_joined_devices_without_new_pairing',
  ];
  for (const name of steps) {
    await t.test(name, async () => {
      if (name.includes('enrollment_ui')) {
        // 该测试要求代理层注入丢响应；其余直连。
        await withFaultProxy(server.url, (proxyUrl) => cargoTest(name, proxyUrl));
      } else {
        await cargoTest(name, server.url);
      }
    });
  }
});
