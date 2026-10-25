#!/usr/bin/env node
/**
 * relay 契约冒烟：对着**已经在线上跑的** relay 打一遍关键路径，确认它不只是
 * 「进程活着」，而是「协议还对」。
 *
 * 覆盖（这些都是曾经真出过事的地方）：
 *   /health        → version 仍为 2（能力靠 /status.capabilities 协商，不靠抬版本号）
 *   /create /register /send /fetch → 信箱主链路（幂等键 seq_hint 原样回来）
 *   /drop          → **签名域分离**：拿 ack 的签名去 drop 必须 403
 *   /status        → capabilities 必须同时含 blob 与 drop
 *
 * 不覆盖：blob 数据面。它由仓库的 `relay && npm test` 在本地对同一份代码跑，
 * 在线上跑会往真实信箱里写分片，收益不抵风险。
 *
 * 用法：node smoke.mjs [baseUrl]
 *   默认 baseUrl 取 $RELAY_BASE，再默认 http://127.0.0.1:8787
 *
 * 副作用：创建一个临时信箱并发一封测试信，随后 drop 掉。空信箱文件会留在
 * 服务器的 data/mailboxes 下（几 KB），sweeper 会回收其过期行。
 */
import { key, mailbox, auth, createBody, registerBody, sendBody, fetchBody, ackBody, statusBody }
  from '../../../relay/test/helpers.mjs';

const BASE = (process.argv[2] || process.env.RELAY_BASE || 'http://127.0.0.1:8787').replace(/\/$/, '');

let failed = 0;
const check = (name, ok, detail = '') => {
  if (!ok) failed += 1;
  console.log(`${ok ? '  ok  ' : ' FAIL '} ${name}${detail ? `  ${detail}` : ''}`);
};

// POST 路由：JSON body。POST_ROUTES = create/register/revoke/rotate/send/ack/drop
const post = async (path, body) => {
  const response = await fetch(BASE + path, {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify(body),
  });
  const text = await response.text();
  let parsed = null;
  try { parsed = JSON.parse(text); } catch { /* 非 JSON 就留原文 */ }
  return { status: response.status, body: parsed, text };
};

// GET 路由：同一份签名材料走 query string。GET_ROUTES = fetch/status/sender
// （签名覆盖 ts/nonce/sig，所以 GET 也必须带齐这几个参数，不是「裸 GET 只读」。）
const get = async (path, query) => {
  const response = await fetch(`${BASE}${path}?${new URLSearchParams(query)}`);
  const text = await response.text();
  let parsed = null;
  try { parsed = JSON.parse(text); } catch { /* 非 JSON 就留原文 */ }
  return { status: response.status, body: parsed, text };
};

const owner = key();
const sender = key();
const cred = key();
const mb = mailbox(owner);
const senderMb = mailbox(sender);
const expiry = Date.now() + 600_000;

// 1. 健康：版本号必须还是 2。
{
  const r = await get('/health', {});
  check('health 200', r.status === 200, `status=${r.status}`);
  check('health version==2', r.body?.version === 2, `version=${r.body?.version}`);
  check('health service 名', r.body?.service === 'elsewhen-relay', String(r.body?.service));
}

// 2. 两个信箱。sender 也要有自己的信箱，否则没有 sender 凭据可授权。
{
  const a = await post('/create', createBody(owner));
  const b = await post('/create', createBody(sender));
  check('create owner', a.status === 200, `status=${a.status} ${a.text.slice(0, 80)}`);
  check('create sender', b.status === 200, `status=${b.status} ${b.text.slice(0, 80)}`);
}

// 3. 授权 sender 往 owner 信箱投递。
{
  const r = await post('/register', registerBody(owner, mb, senderMb, cred, 0, expiry));
  check('register (grant)', r.status === 200, `status=${r.status} ${r.text.slice(0, 80)}`);
}

// 4. 投递一封带 seq_hint 的信：seq_hint 是唯一明文的业务字段，也是投递幂等键。
let mailId = null;
{
  const r = await post('/send', sendBody(cred, mb, senderMb, { content: 'aGVsbG8tcmVsYXk=', seq: 7, expiry }));
  mailId = r.body?.id ?? null;
  check('send', r.status === 200 && mailId !== null, `status=${r.status} id=${mailId}`);
}

// 5. 取回：seq_hint 必须原样带回来（R2 的幂等键就靠它）。
{
  const r = await get('/fetch', fetchBody(owner, mb));
  const envelopes = r.body?.envelopes ?? [];
  check('fetch 拿到 1 封', envelopes.length === 1, `count=${envelopes.length} status=${r.status}`);
  check('fetch seq_hint==7', envelopes[0]?.seq_hint === 7, `seq_hint=${envelopes[0]?.seq_hint}`);
}

// 6. 域分离：ack 的签名拿去 drop 必须失败。这是 R5 的结构性防线，
//    一旦有人把两个动词合并成「一个删除端点」，这条会立刻红。
{
  const ids = [mailId];
  const forged = { version: 2, mailbox_key: mb, ids, ...auth(owner, 'ack', [mb, ids.join(',')]) };
  const r = await post('/drop', forged);
  check('drop 拒绝 ack 域的签名', r.status === 403, `status=${r.status} ${r.text.slice(0, 80)}`);
}

// 7. 正确的 drop 域应当成功。
{
  const ids = [mailId];
  const r = await post('/drop', { version: 2, mailbox_key: mb, ids, ...auth(owner, 'drop', [mb, ids.join(',')]) });
  check('drop (drop 域)', r.status === 200, `status=${r.status} ${r.text.slice(0, 80)}`);
}

// 8. /status 是 owner 签名操作，不是裸 GET；能力在这里协商。
{
  const r = await get('/status', statusBody(owner, mb));
  const caps = r.body?.capabilities ?? [];
  check('status 200', r.status === 200, `status=${r.status}`);
  check('capabilities 含 blob', caps.includes('blob'), JSON.stringify(caps));
  check('capabilities 含 drop', caps.includes('drop'), JSON.stringify(caps));
  check('drop 之后 count==0', r.body?.count === 0, `count=${r.body?.count}`);
}

console.log(failed === 0
  ? `\n契约冒烟通过（${BASE}）`
  : `\n契约冒烟 FAIL ${failed} 项（${BASE}）`);
console.log(`临时信箱（留在服务器上，几 KB 空库）：${mb} / ${senderMb}`);
process.exit(failed === 0 ? 0 : 1);
