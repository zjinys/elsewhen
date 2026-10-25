/**
 * relay v3 数据面契约测试。
 *
 * 每条断言对应方案 §3.1 的一条「关键性质」：
 *   1. 身份 = (blob_id, idx)  ⇒ 重复 PUT 不占第二份空间（4.4× 重复副本的结构性解法）
 *   2. `ack upto` 立即释放  ⇒ 峰值 = 客户端窗口，而不是「整份等 ack」
 *   3. `close` 精确清零       ⇒ 不靠 TTL 才能做到「close 后残留 0」
 *   4. 越权 / 非法 blob_id / 超限 都要在**落库前**被拒（400/403/413/507）
 *   5. blob 账与信令账分开     ⇒ 快照不可能把控制消息挤死
 */
import assert from 'node:assert/strict';
import { randomBytes } from 'node:crypto';
import test from 'node:test';
import { auth, hmac, key, mailbox, paired, registerBody, sendBody, sha256, startRelay } from '../test/helpers.mjs';

const chunkOf = (n = 4096) => randomBytes(n);
const blobId = () => randomBytes(24).toString('hex'); // 48 字符，落在 16..64 的合法区间

/** POST/GET 的 JSON 端点。 */
async function json(relay, path, body, get = false) {
  const base = await relay.runtime.ready;
  const url = get ? `${base}${path}?${new URLSearchParams(body)}` : `${base}${path}`;
  const response = await fetch(url, get ? {} : {
    method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify(body),
  });
  return { status: response.status, body: await response.json() };
}

/** `PUT /blob/chunk`：body 是裸二进制，鉴权参数走 query。 */
async function putChunk(relay, credential, mb, sender, id, index, payload) {
  const ts = Date.now();
  const nonce = randomBytes(16).toString('hex');
  const sig = signedPut(credential, sender, mb, id, index, payload, ts, nonce);
  const base = await relay.runtime.ready;
  const query = new URLSearchParams({
    version: '2', mailbox_key: mb, sender_mb: sender, blob_id: id,
    index: String(index), ts: String(ts), nonce, sig,
  });
  const response = await fetch(`${base}/blob/chunk?${query}`, { method: 'PUT', body: payload });
  return { status: response.status, body: await response.json() };
}

/** 裸二进制下载：返回原始分片与响应头。 */
async function getChunks(relay, credential, mb, id, from = 0, limit = 8) {
  const base = await relay.runtime.ready;
  const query = new URLSearchParams({
    version: '2', mailbox_key: mb, blob_id: id, from: String(from), limit: String(limit),
    ...auth(credential, 'blob-get', [mb, id, String(from), String(limit)]),
  });
  const response = await fetch(`${base}/blob/chunk?${query}`);
  const buffer = Buffer.from(await response.arrayBuffer());
  const headers = {
    from: Number(response.headers.get('x-blob-from')),
    count: Number(response.headers.get('x-blob-count')),
    next: Number(response.headers.get('x-blob-next')),
  };
  // 解析 [4 字节大端长度][密文] 帧
  const frames = [];
  for (let offset = 0; offset < buffer.length;) {
    const size = buffer.readUInt32BE(offset);
    frames.push(buffer.subarray(offset + 4, offset + 4 + size));
    offset += 4 + size;
  }
  return { status: response.status, headers, frames, raw: buffer };
}

/** PUT 的签名把「这一片的确切字节」钉在签名里（与 /send 的 sha256(ciphertext) 同构）。 */
function signedPut(credential, sender, mb, id, index, payload, ts, nonce) {
  return hmac(credential, ['relay-v2-blob-put', sender, mb, id, String(index), String(ts), nonce, sha256(payload)]);
}

// `open` 只开一张「传输支票」：片数/总字节/摘要在流式上传里发完才知道，relay
// 存一份既没用又多一个可见字段 —— 总量以端到端签名的 manifest 为准。
const openBody = (credential, mb, sender, id, overrides = {}) => ({
  version: 2, mailbox_key: mb, sender_mb: sender, blob_id: id,
  ...auth(credential, 'blob-open', [sender, mb, id], overrides),
});
// 开户 sender 也能查进度（窗口流控要用）；不带 sender_mb 就只有 owner 那一把能验。
const senderStateBody = (credential, mb, sender, id) => ({
  version: 2, mailbox_key: mb, sender_mb: sender, blob_id: id, ...auth(credential, 'blob-state', [mb, id]),
});
const stateBody = (owner, mb, id) => ({ version: 2, mailbox_key: mb, blob_id: id, ...auth(owner, 'blob-state', [mb, id]) });
const ackBody = (owner, mb, id, upto) => ({ version: 2, mailbox_key: mb, blob_id: id, upto, ...auth(owner, 'blob-ack', [mb, id, String(upto)]) });
const closeBody = (credential, mb, id, status, sender) => ({
  version: 2, mailbox_key: mb, blob_id: id, status, ...(sender ? { sender_mb: sender } : {}),
  ...auth(credential, 'blob-close', [mb, id, status]),
});

async function status(relay, owner, mb) {
  const result = await json(relay, '/status', { version: '2', mailbox_key: mb, ...auth(owner, 'status', [mb]) }, true);
  assert.equal(result.status, 200);
  return result.body;
}

test('同一 blob 重复 PUT 同一片只占一份空间；ack 立即释放；close 精确清零', async () => {
  const relay = await startRelay();
  try {
    const { owner, mb, sender, credential } = await paired(relay);
    const id = blobId();
    const chunks = [chunkOf(4096), chunkOf(4096), chunkOf(2048)];
    const bytes = chunks.reduce((sum, chunk) => sum + chunk.length, 0);

    const opened = await json(relay, '/blob/open', openBody(credential, mb, sender, id));
    assert.equal(opened.status, 200);
    assert.deepEqual(opened.body.have, []);

    for (const [index, chunk] of chunks.entries()) {
      const put = await putChunk(relay, credential, mb, sender, id, index, chunk);
      assert.equal(put.status, 200);
      assert.equal(put.body.have, index + 1);
      assert.equal(put.body.highest, index);
    }

    const full = await status(relay, owner, mb);
    assert.equal(full.blob_bytes, bytes);
    assert.equal(full.blobs, 1);

    // 同一片重投（内容不同但同长度）：主键 (blob_id, idx) 让它原地覆盖，账目不变。
    const replacement = chunkOf(4096);
    const again = await putChunk(relay, credential, mb, sender, id, 1, replacement);
    assert.equal(again.status, 200);
    assert.equal(again.body.have, 3);
    const afterReplay = await json(relay, '/blob/state', stateBody(owner, mb, id), true);
    assert.deepEqual(afterReplay.body.present, [0, 1, 2]);
    assert.equal(afterReplay.body.stored_bytes, bytes);
    assert.equal((await status(relay, owner, mb)).blob_bytes, bytes);
    chunks[1] = replacement;

    // 上传方也能查进度：没有这个信号，窗口只能靠撞 507 退避。
    const senderView = await json(relay, '/blob/state', senderStateBody(credential, mb, sender, id), true);
    assert.equal(senderView.status, 200);
    assert.deepEqual(senderView.body.present, [0, 1, 2]);

    // 重复 open 是幂等回执：不重置进度，只续期。
    const reopened = await json(relay, '/blob/open', openBody(credential, mb, sender, id));
    assert.equal(reopened.status, 200);
    assert.deepEqual(reopened.body.have, [0, 1, 2]);

    // 裸二进制下载：不套 JSON，帧自带长度前缀。
    const page = await getChunks(relay, owner, mb, id, 0, 8);
    assert.equal(page.status, 200);
    assert.deepEqual(page.headers, { from: 0, count: 3, next: 3 });
    assert.equal(page.frames.length, 3);
    assert.deepEqual(page.frames, chunks);

    // ack upto=1 立即删掉前两片，腾出空间（不是等整份 ack）。
    const acked = await json(relay, '/blob/ack', ackBody(owner, mb, id, 1));
    assert.equal(acked.status, 200);
    assert.equal(acked.body.deleted, 2);
    assert.equal(acked.body.remaining, 1);
    const released = await json(relay, '/blob/state', stateBody(owner, mb, id), true);
    assert.deepEqual(released.body.present, [2]);
    assert.equal(released.body.highest, 2);
    assert.equal(released.body.stored_bytes, chunks[2].length);
    assert.equal((await status(relay, owner, mb)).blob_bytes, chunks[2].length);

    // 缺口之后的下载从第一片存在的开始：缺口（已 ack 的）由 state.present 反推，
    // relay 不知道总量，也就没有「缺哪几片」这回事。
    const gap = await getChunks(relay, owner, mb, id, 0, 8);
    assert.deepEqual([gap.headers.from, gap.headers.count, gap.headers.next], [2, 1, 3]);

    const closed = await json(relay, '/blob/close', closeBody(owner, mb, id, 'done'));
    assert.equal(closed.status, 200);
    assert.equal(closed.body.freed, chunks[2].length);
    const gone = await status(relay, owner, mb);
    assert.equal(gone.blob_bytes, 0);
    assert.equal(gone.blobs, 0);
    // 幂等：已经关掉的 blob 再 close 也是成功。
    assert.equal((await json(relay, '/blob/close', closeBody(owner, mb, id, 'done'))).status, 200);
  } finally {
    relay.runtime.dispose();
  }
});

test('越权、非法 blob_id、元数据不符、协议版本不符都在落库前被拒', async () => {
  const relay = await startRelay();
  try {
    const { owner, mb, sender, credential } = await paired(relay);
    const id = blobId();
    const chunk = chunkOf(1024);

    // blob_id 是 URL 上的身份：长度与字符集必须先卡住，含分隔符一律 400。
    for (const bad of ['short', 'a'.repeat(65), `${id}/x`, `${id}:x`, `${id} x`, '', `${id}.db`]) {
      const result = await json(relay, '/blob/open', openBody(credential, mb, sender, bad));
      assert.equal(result.status, 400, `blob_id ${JSON.stringify(bad)} must be rejected`);
    }
    // 协议版本仍是 2：不认识的版本要 426，而不是当成新协议静默接受。
    const wrongVersion = { ...openBody(credential, mb, sender, id), version: 1 };
    assert.equal((await json(relay, '/blob/open', wrongVersion)).status, 426);

    const opened = await json(relay, '/blob/open', openBody(credential, mb, sender, id));
    assert.equal(opened.status, 200);


    // 未注册的 sender 一律 403（复用 /send 的授权检查）。
    const strangerOwner = key(), stranger = mailbox(strangerOwner), strangerCredential = key();
    assert.equal((await putChunk(relay, strangerCredential, mb, stranger, id, 0, chunk)).status, 403);
    // owner 凭据冒充 sender 也不行。
    assert.equal((await putChunk(relay, owner, mb, sender, id, 0, chunk)).status, 403);
    // 索引边界：总量未知，所以只卡 0..MAX_BLOB_CHUNKS-1 这个结构上限。
    assert.equal((await putChunk(relay, credential, mb, sender, id, 4096, chunk)).status, 400);
    assert.equal((await putChunk(relay, credential, mb, sender, id, -1, chunk)).status, 400);
    // 空 body 不是合法分片。
    const base = await relay.runtime.ready;
    const ts = Date.now(), nonce = randomBytes(16).toString('hex');
    const emptyQuery = new URLSearchParams({
      version: '2', mailbox_key: mb, sender_mb: sender, blob_id: id, index: '0', ts: String(ts), nonce,
      sig: signedPut(credential, sender, mb, id, 0, Buffer.alloc(0), ts, nonce),
    });
    assert.equal((await fetch(`${base}/blob/chunk?${emptyQuery}`, { method: 'PUT', body: Buffer.alloc(0) })).status, 400);

    // 读侧只有 owner：sender 不能读、不能确认、不能关。
    assert.equal((await json(relay, '/blob/state', stateBody(credential, mb, id), true)).status, 403);
    assert.equal((await json(relay, '/blob/ack', ackBody(credential, mb, id, 0))).status, 403);
    assert.equal((await getChunks(relay, credential, mb, id)).status, 403);
    // 陌生人连 close 都签不出来。
    assert.equal((await json(relay, '/blob/close', closeBody(strangerCredential, mb, id, 'cancel', stranger))).status, 403);

    // 以上全部没有落库。
    assert.equal((await status(relay, owner, mb)).blob_bytes, 0);
    // 撤回是上传方的正当动作：sender 可以 close cancel。
    assert.equal((await json(relay, '/blob/close', closeBody(credential, mb, id, 'cancel', sender))).status, 200);
  } finally {
    relay.runtime.dispose();
  }
});

test('同信箱的另一个 sender 既不能写进别人的 blob，也不能替它撤回', async () => {
  const relay = await startRelay();
  try {
    const { owner, mb, sender, credential } = await paired(relay);
    // 第二个 sender：同信箱、授权合法，但不是这个 blob 的开户人。
    const otherOwner = key(), other = mailbox(otherOwner), otherCredential = key();
    const registered = await json(relay, '/register', registerBody(owner, mb, other, otherCredential));
    assert.equal(registered.status, 200);

    const id = blobId();
    const chunk = chunkOf(1024);
    assert.equal((await json(relay, '/blob/open', openBody(credential, mb, sender, id))).status, 200);

    assert.equal((await putChunk(relay, otherCredential, mb, other, id, 0, chunk)).status, 403);
    assert.equal((await json(relay, '/blob/close', closeBody(otherCredential, mb, id, 'cancel', other))).status, 403);
    // 开户人自己不受影响。
    assert.equal((await putChunk(relay, credential, mb, sender, id, 0, chunk)).status, 200);
    assert.equal((await status(relay, owner, mb)).blob_bytes, 1024);
  } finally {
    relay.runtime.dispose();
  }
});

test('单次 PUT 超上限 413，在途总量超 blob 配额 507，且不动信令账', async () => {
  const relay = await startRelay({ bindings: { BLOB_QUOTA_BYTES: '1000' } });
  try {
    const { owner, mb, sender, credential } = await paired(relay);
    const first = blobId(), second = blobId();
    const chunk = chunkOf(600);

    assert.equal((await json(relay, '/blob/open', openBody(credential, mb, sender, first))).status, 200);
    assert.equal((await putChunk(relay, credential, mb, sender, first, 0, chunk)).status, 200);

    // 配额按**已落盘字节**逐片判：open 不占空间，第二片写不下才 507。
    assert.equal((await json(relay, '/blob/open', openBody(credential, mb, sender, second))).status, 200);
    const overflow = await putChunk(relay, credential, mb, sender, second, 0, chunk);
    assert.equal(overflow.status, 507);
    // 同尺寸重投要扣掉自己原来的那份，不能把自己挤成 507。
    assert.equal((await putChunk(relay, credential, mb, sender, first, 0, chunk)).status, 200);

    // 单片超过 1 MiB 的 PUT 是 413，直接挡在请求体内，不落库。
    const big = randomBytes(1024 * 1024 + 1);
    assert.equal((await putChunk(relay, credential, mb, sender, first, 0, big)).status, 413);

    // 信令账照常工作，且与 blob 账互不占用：一封 5 字节的信不受 1000 字节 blob 配额影响。
    const sent = await json(relay, '/send', sendBody(credential, mb, sender, { content: 'c3ludGg=', seq: 7 }));
    assert.equal(sent.status, 200);

    const counters = await status(relay, owner, mb);
    assert.deepEqual(counters.capabilities, ['blob', 'drop']);
    // 只传了一片：第二个 blob 开了但没写片 ⇒ 占 0 字节（不再「声明即预扣」）。
    assert.equal(counters.blob_bytes, 600);
    assert.equal(counters.blobs, 2);
    assert.equal(counters.signal_bytes, 5);
    assert.equal(counters.bytes, 5);
    assert.equal(counters.quota, 52_428_800);
    assert.equal(counters.blob_quota, 1000);
  } finally {
    relay.runtime.dispose();
  }
});

test('无进展 TTL 到期后由 clean 整块回收，不需要 close', async () => {
  const relay = await startRelay({ bindings: { BLOB_TTL_MS: '200' } });
  try {
    const { owner, mb, sender, credential } = await paired(relay);
    const id = blobId();
    const chunk = chunkOf(1024);
    assert.equal((await json(relay, '/blob/open', openBody(credential, mb, sender, id))).status, 200);
    assert.equal((await putChunk(relay, credential, mb, sender, id, 0, chunk)).status, 200);
    assert.equal((await status(relay, owner, mb)).blob_bytes, 1024);

    // TTL=200ms：等它过期，下一次请求的 clean 就会把表和分片一起删掉。
    await new Promise((resolve) => setTimeout(resolve, 500));
    const expired = await json(relay, '/blob/state', stateBody(owner, mb, id), true);
    assert.equal(expired.status, 200);
    assert.deepEqual(expired.body.present, []);
    assert.equal(expired.body.highest, -1);
    assert.equal((await status(relay, owner, mb)).blob_bytes, 0);
    // 超时被收掉的 blob 再 close 不算错。
    assert.equal((await json(relay, '/blob/close', closeBody(owner, mb, id, 'cancel'))).status, 200);
  } finally {
    relay.runtime.dispose();
  }
});

test('一个信箱可并行多个 blob，互不干扰；删一个不影响另一个', async () => {
  const relay = await startRelay();
  try {
    const { owner, mb, sender, credential } = await paired(relay);
    const ids = [blobId(), blobId()];
    const chunks = [chunkOf(2048), chunkOf(3072)];
    for (const [index, id] of ids.entries()) {
      const opened = await json(relay, '/blob/open', openBody(credential, mb, sender, id));
      assert.equal(opened.status, 200);
      assert.equal((await putChunk(relay, credential, mb, sender, id, 0, chunks[index])).status, 200);
    }
    assert.equal((await status(relay, owner, mb)).blob_bytes, 2048 + 3072);
    assert.equal((await json(relay, '/blob/close', closeBody(owner, mb, ids[0], 'done'))).status, 200);
    assert.equal((await status(relay, owner, mb)).blob_bytes, 3072);
    const survivor = await json(relay, '/blob/state', stateBody(owner, mb, ids[1]), true);
    assert.deepEqual(survivor.body.present, [0]);
    assert.equal(survivor.body.stored_bytes, 3072);
  } finally {
    relay.runtime.dispose();
  }
});

test('旧客户端路径不受影响：/status 老字段仍在、/send+/fetch 照旧工作', async () => {
  const relay = await startRelay();
  try {
    const { owner, mb, sender, credential } = await paired(relay);
    const sent = await json(relay, '/send', sendBody(credential, mb, sender, { content: 'YQ==', seq: 3 }));
    assert.equal(sent.status, 200);
    const fetched = await json(relay, '/fetch', {
      version: '2', mailbox_key: mb, after: '0', limit: '20', ...auth(owner, 'fetch', [mb, '0', '20']),
    }, true);
    assert.equal(fetched.status, 200);
    assert.equal(fetched.body.envelopes.length, 1);
    const counters = await status(relay, owner, mb);
    // 老客户端读的就是这四个字段，新字段是纯追加。
    for (const field of ['ok', 'bytes', 'count', 'earliest_expiry', 'quota']) assert.ok(field in counters, field);
    assert.equal(counters.count, 1);
  } finally {
    relay.runtime.dispose();
  }
});

/**
 * 三条容量不变式（方案 §3.4 / roadmap R5-3）。它们不是「顺便测测」的行为，
 * 而是这个系统允许存在的**边界**：任何一条被破，事故就会以另一种形式复发
 * （信令被大文件挤死、数据面无限增长、传输残留变成永久占用）。
 */
test('容量不变式：两本账各自设上限、互不挤占，close 后立即归零', async () => {
  const relay = await startRelay({ bindings: { MAILBOX_QUOTA_BYTES: '4096', BLOB_QUOTA_BYTES: '2048' } });
  try {
    const { owner, mb, sender, credential } = await paired(relay);

    // 不变式 1：signal_bytes ≤ 信令配额。写信写到被拒为止，账面永不越界。
    let accepted = 0;
    let rejection = null;
    for (const seq of [1, 2, 3, 4, 5, 6]) {
      const sent = await json(relay, '/send', sendBody(credential, mb, sender, { content: chunkOf(1024).toString('base64'), seq }));
      if (sent.status === 200) { accepted += 1; continue; }
      rejection = sent;
      break;
    }
    assert.equal(accepted, 4, '4096 字节配额最多容 4 封 1 KiB 的信');
    assert.equal(rejection.status, 507, '越界的信必须 507，而不是悄悄收下再撑爆盘');
    const filled = await status(relay, owner, mb);
    assert.ok(filled.signal_bytes <= filled.quota, `信令账越界：${filled.signal_bytes} > ${filled.quota}`);
    // 三分账自洽：老字段 `bytes` 就是信令账，`count` 与之一致。
    assert.equal(filled.bytes, filled.signal_bytes);
    assert.equal(filled.count, accepted);
    assert.equal(filled.blob_bytes, 0);

    // 不变式 2：blob_bytes ≤ blob 配额，且**信令已满也不影响 blob**（两本账独立）。
    // 这正是事故里最致命的一环：快照把控制消息挤死 ⇒ 配对永远收不了尾。
    const id = blobId();
    assert.equal((await json(relay, '/blob/open', openBody(credential, mb, sender, id))).status, 200);
    assert.equal((await putChunk(relay, credential, mb, sender, id, 0, chunkOf(1024))).status, 200);
    assert.equal((await putChunk(relay, credential, mb, sender, id, 1, chunkOf(1024))).status, 200);
    const overflow = await putChunk(relay, credential, mb, sender, id, 2, chunkOf(1024));
    assert.equal(overflow.status, 507, '数据面越界也必须 507');
    // 被拒的那一片不落库，已落盘的两片一片不少（不能「回滚式」连坐）。
    const state = await json(relay, '/blob/state', stateBody(owner, mb, id), true);
    assert.deepEqual(state.body.present, [0, 1]);
    assert.equal(state.body.stored_bytes, 2048);
    const both = await status(relay, owner, mb);
    assert.ok(both.blob_bytes <= both.blob_quota, `数据面越界：${both.blob_bytes} > ${both.blob_quota}`);
    assert.equal(both.signal_bytes, filled.signal_bytes, 'blob 的增长不该动信令账');
    // 信令已满的状态下再写一封信仍然按配额拒绝（不是被 blob 挤的，是被自己的账拒的）。
    const again = await json(relay, '/send', sendBody(credential, mb, sender, { content: chunkOf(1024).toString('base64'), seq: 99 }));
    assert.equal(again.status, 507);

    // 不变式 3（无进展 TTL 由 clean 整块回收）由下一条用例覆盖；这里锁「close 立即为 0」，
    // 因为「等 TTL」本身就是事故里残留 7 天的根因。
    assert.equal((await json(relay, '/blob/close', closeBody(owner, mb, id, 'cancel'))).status, 200);
    const cleared = await status(relay, owner, mb);
    assert.equal(cleared.blob_bytes, 0, 'close 之后数据面残留必须是 0');
    assert.equal(cleared.blobs, 0);
    assert.equal(cleared.signal_bytes, filled.signal_bytes, '关闭数据面不该碰信令');
  } finally {
    relay.runtime.dispose();
  }
});

test('PUT 也走统一的 validateEnvelope：拿 sender 记录当收件凭据被拒', async () => {
  const relay = await startRelay();
  try {
    const { owner, mb, sender, credential } = await paired(relay);
    const id = blobId();
    const chunk = chunkOf(1024);
    assert.equal((await json(relay, '/blob/open', openBody(credential, mb, sender, id))).status, 200);
    const base = await relay.runtime.ready;
    // `recipient_mb` 这条契约原本只有 JSON 端点走，PUT 手抄了两条校验、漏了它：
    // 老客户端因此可以拿 sender 记录当「收件凭据」蒙过这条路径。
    const putWith = async (extra) => {
      const ts = Date.now(), nonce = randomBytes(16).toString('hex');
      const query = new URLSearchParams({
        version: '2', mailbox_key: mb, sender_mb: sender, blob_id: id, index: '0',
        ts: String(ts), nonce, sig: signedPut(credential, sender, mb, id, 0, chunk, ts, nonce), ...extra,
      });
      return (await fetch(`${base}/blob/chunk?${query}`, { method: 'PUT', body: chunk })).status;
    };
    assert.equal(await putWith({ recipient_mb: mailbox(key()) }), 403, '别人的信箱不能当收件凭据');
    assert.equal(await putWith({ recipient_mb: mb }), 200, '等于自己的 mailbox_key 时照旧放行');
    // 版本仍然是统一的 426（不是 400/500）。
    assert.equal(await putWith({ version: '1', recipient_mb: mb }), 426);
    // 校验失败的那次没有落库：只有正对照那一片 1024 字节。
    assert.equal((await status(relay, owner, mb)).blob_bytes, 1024);
  } finally {
    relay.runtime.dispose();
  }
});

test('六个签名域两两交叉重放：只有本域签名能过，其余 25 组全 403', async () => {
  const relay = await startRelay();
  try {
    const { owner, mb, sender, credential } = await paired(relay);
    const id = blobId();
    const chunk = chunkOf(1024);
    const base = await relay.runtime.ready;
    const domains = ['blob-open', 'blob-put', 'blob-state', 'blob-get', 'blob-ack', 'blob-close'];
    // 每个端点都构造成「除了签名域之外完全合法」的请求：唯一变量就是域。
    const call = {
      'blob-open': (domain) => json(relay, '/blob/open', {
        version: 2, mailbox_key: mb, sender_mb: sender, blob_id: id, ...auth(credential, domain, [sender, mb, id]),
      }),
      'blob-put': async (domain) => {
        const ts = Date.now(), nonce = randomBytes(16).toString('hex');
        const query = new URLSearchParams({
          version: '2', mailbox_key: mb, sender_mb: sender, blob_id: id, index: '0',
          ts: String(ts), nonce,
          sig: hmac(credential, [`relay-v2-${domain}`, sender, mb, id, '0', String(ts), nonce, sha256(chunk)]),
        });
        const response = await fetch(`${base}/blob/chunk?${query}`, { method: 'PUT', body: chunk });
        return { status: response.status, body: await response.json() };
      },
      'blob-state': (domain) => json(relay, '/blob/state', {
        version: '2', mailbox_key: mb, blob_id: id, ...auth(owner, domain, [mb, id]),
      }, true),
      'blob-get': async (domain) => {
        const query = new URLSearchParams({
          version: '2', mailbox_key: mb, blob_id: id, from: '0', limit: '8',
          ...auth(owner, domain, [mb, id, '0', '8']),
        });
        return { status: (await fetch(`${base}/blob/chunk?${query}`)).status };
      },
      'blob-ack': (domain) => json(relay, '/blob/ack', {
        version: 2, mailbox_key: mb, blob_id: id, upto: 0, ...auth(owner, domain, [mb, id, '0']),
      }),
      'blob-close': (domain) => json(relay, '/blob/close', {
        version: 2, mailbox_key: mb, blob_id: id, status: 'done', ...auth(owner, domain, [mb, id, 'done']),
      }),
    };

    // 正对照：每个端点用自己的域都能过。没有这一段，下面「全 403」也可能只是
    // 别的地方坏了（比如 blob 没开成功、body 形状不对），测试就成了空断言。
    const lifecycle = async () => {
      assert.equal((await call['blob-open']('blob-open')).status, 200);
      assert.equal((await call['blob-put']('blob-put')).status, 200);
      assert.equal((await call['blob-state']('blob-state')).status, 200);
      assert.equal((await call['blob-get']('blob-get')).status, 200);
      assert.equal((await call['blob-ack']('blob-ack')).status, 200);
      assert.equal((await call['blob-close']('blob-close')).status, 200);
    };
    await lifecycle();

    // 交叉重放：6 × 5 = 30 组，一律 403（签名域是钉子，只有本域算数）。
    for (const target of domains) {
      for (const forged of domains.filter((domain) => domain !== target)) {
        const result = await call[target](forged);
        assert.equal(result.status, 403, `${forged} 域签名不能打 ${target}`);
      }
    }
    // 全程没有一封「冒充成功」的写入，也没有关掉任何 blob：再跑一遍生命周期仍是通的。
    await lifecycle();
    assert.equal((await status(relay, owner, mb)).blob_bytes, 0);
  } finally {
    relay.runtime.dispose();
  }
});
