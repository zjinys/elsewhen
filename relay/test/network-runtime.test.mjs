import assert from 'node:assert/strict';
import { test } from 'node:test';
import { execFile } from 'node:child_process';
import { promisify } from 'node:util';
import { fileURLToPath } from 'node:url';
import { startRelay } from './helpers.mjs';

const run = promisify(execFile);

test('P3 steady-state network ticks move deltas after enrollment completes', { timeout: 240_000 }, async t => {
  const relay = await startRelay();
  t.after(() => relay.runtime.dispose());
  const upstream = await relay.runtime.ready;
  await run('cargo', [
    'test', '--lib',
    'sync::runtime::network::tests::tick_moves_deltas_between_joined_devices_without_new_pairing',
    '--', '--ignored', '--exact',
  ], {
    cwd: fileURLToPath(new URL('../../', import.meta.url)),
    env: {
      ...process.env, ELSEWHEN_RELAY_TEST_URL: upstream,
      NO_PROXY: 'localhost,127.0.0.1,::1', no_proxy: 'localhost,127.0.0.1,::1',
    },
    timeout: 220_000,
    maxBuffer: 2 * 1024 * 1024,
  });
  assert.ok(true);
});
