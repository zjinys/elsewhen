// /release/* 升级分发路由：R2 托管清单与 APK，Worker 只做路由与守护。
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { startRelay } from './helpers.mjs';

test('release: 未发布清单时 404', async () => {
  const relay = await startRelay({ r2: true });
  try {
    const { status, body } = await relay.call('/release/latest', {}, true);
    assert.equal(status, 404);
    assert.match(body.error, /no release published/);
  } finally { await relay.runtime.dispose(); }
});

test('release: 已发布清单可读，非法 hash 的 apk 请求被拒', async () => {
  const relay = await startRelay({ r2: true });
  try {
    const bucket = await relay.runtime.getR2Bucket('RELEASES');
    const manifest = {
      version: '1.0.0', buildHash: 'abcd123', buildSeq: 42,
      changelog: 'test', apkSha256: 'e3b0c442', apkSize: 3, publishedAt: 1,
    };
    await bucket.put('release/manifest.json', JSON.stringify(manifest));

    const got = await relay.call('/release/latest', {}, true);
    assert.equal(got.status, 200);
    assert.equal(got.body.buildHash, 'abcd123');
    assert.equal(got.body.buildSeq, 42);

    for (const bad of ['../evil', 'ABCD123', 'a'.repeat(65), 'x y']) {
      const res = await relay.call('/release/apk', { h: bad }, true);
      assert.equal(res.status, 400, `hash ${bad} must be rejected`);
    }

    const missing = await relay.call('/release/apk', { h: 'abcd123' }, true);
    assert.equal(missing.status, 404);
  } finally { await relay.runtime.dispose(); }
});

test('release: APK 以正确类型与长度流式返回', async () => {
  const relay = await startRelay({ r2: true });
  try {
    const bucket = await relay.runtime.getR2Bucket('RELEASES');
    const payload = new Uint8Array([0x50, 0x4b, 0x03, 0x04, 1, 2, 3]);
    await bucket.put('apk/abcd123.apk', payload);

    const response = await relay.runtime.dispatchFetch('http://relay.test/release/apk?h=abcd123');
    assert.equal(response.status, 200);
    assert.equal(response.headers.get('content-type'), 'application/vnd.android.package-archive');
    assert.equal(Number(response.headers.get('content-length')), payload.length);
    const body = new Uint8Array(await response.arrayBuffer());
    assert.deepEqual([...body], [...payload]);
  } finally { await relay.runtime.dispose(); }
});
