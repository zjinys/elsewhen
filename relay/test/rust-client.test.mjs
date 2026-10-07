import { test } from 'node:test';
import { execFile } from 'node:child_process';
import { promisify } from 'node:util';
import { fileURLToPath } from 'node:url';
import { startRelay } from './helpers.mjs';

const run = promisify(execFile);

test('Rust client interoperates with isolated production Worker v2', { timeout: 180_000 }, async t => {
  const relay = await startRelay();
  t.after(() => relay.runtime.dispose());
  const url = await relay.runtime.ready;
  await run('cargo', [
    'test', '--lib',
    'sync::transport_relay_smoke_tests::smoke::end_to_end_send_fetch_ack_against_real_worker',
    '--', '--ignored', '--exact',
  ], {
    cwd: fileURLToPath(new URL('../../', import.meta.url)),
    env: {
      ...process.env,
      ELSEWHEN_RELAY_TEST_URL: url.toString(),
      NO_PROXY: [process.env.NO_PROXY, 'localhost,127.0.0.1,::1'].filter(Boolean).join(','),
      no_proxy: [process.env.no_proxy, 'localhost,127.0.0.1,::1'].filter(Boolean).join(','),
    },
    timeout: 170_000,
    maxBuffer: 2 * 1024 * 1024,
  });
});
