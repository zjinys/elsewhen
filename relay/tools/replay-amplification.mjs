// R2 验收：把 1.7 GB 归档里 37306d0c 信箱的真实投递形态重放一遍。
// 形态文件由归档实测得出（默认 /tmp/loadshape.txt，可用 argv[2] 指定）：175 个不同载荷、共投递 801 次、
// 合计 534.8 MB —— 即 4.41× 磁盘放大。
// 对比两遍：seq=0（R2 之前的客户端）与 seq=每载荷固定序号（R2 之后）。
import { readFileSync } from 'node:fs';
import { startRelay, key, mailbox, createBody, registerBody, sendBody, auth } from '../test/helpers.mjs';

const shape = readFileSync(process.argv[2] || '/tmp/loadshape.txt', 'utf8').trim().split('\n')
  .map((line) => line.split('|').map(Number));

async function replay(label, seqFor) {
  const relay = await startRelay({ bindings: { MAILBOX_QUOTA_BYTES: '536870912' } });
  try {
    const owner = key(), mb = mailbox(owner), senderCred = key(), senderMb = mailbox(senderCred);
    const created = await relay.call('/create', createBody(owner));
    if (created.status !== 200) throw new Error(`create failed: ${JSON.stringify(created)}`);
    const registered = await relay.call('/register', registerBody(owner, mb, senderMb, senderCred));
    if (registered.status !== 200) throw new Error(`register failed: ${JSON.stringify(registered)}`);
    const expiry = Date.now() + 24 * 60 * 60_000;
    let offered = 0, rejected = 0, failedBytes = 0;
    for (const [index, [bytes, count]] of shape.entries()) {
      // 同一载荷的多份副本共用同一串密文（内容相同、序号相同）：这正是客户端
      // 每拍重发同一条 outbox 记录的真实形态。
      const content = Buffer.alloc(bytes, (index % 251) + 1).toString('base64');
      for (let copy = 0; copy < count; copy++) {
        offered += bytes;
        const sent = await relay.call('/send', sendBody(senderCred, mb, senderMb, { content, seq: seqFor(index), expiry }));
        if (sent.status !== 200) { rejected += 1; failedBytes += bytes; }
      }
    }
    const a = auth(owner, 'status', [mb]);
    const status = await relay.call('/status', { version: 2, mailbox_key: mb, ts: a.ts, nonce: a.nonce, sig: a.sig }, true);
    return { label, offered, rejected, failedBytes, stored: status.body.bytes, count: status.body.count };
  } finally {
    relay.runtime.dispose();
  }
}

const before = await replay('R2 之前（seq=0：每次重发都落一封新邮件）', () => 0);
const after = await replay('R2 之后（同一载荷同一个 seq_hint）', (index) => index + 1);
const mb = (n) => `${(n / 1048576).toFixed(1)} MB`;
for (const r of [before, after]) {
  console.log(`${r.label}\n  客户端投递=${mb(r.offered)}（被 507 拒绝 ${r.rejected} 次，${mb(r.failedBytes)} 未落地）\n  邮箱实存=${mb(r.stored)} / ${r.count} 封`);
}
console.log(`\n放大倍数：${(before.stored / after.stored).toFixed(2)}× → 1.00×，实存 ${mb(before.stored)} → ${mb(after.stored)}（省 ${mb(before.stored - after.stored)}）`);
