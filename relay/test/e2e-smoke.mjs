#!/usr/bin/env node
// 对已部署的 relay Worker 做端到端冒烟：模拟两台设备（A 发 / B 收）跑完整协议。
// 只依赖 Node 18+（fetch + node:crypto），不装任何包。
//
// 用法：
//   node test/e2e-smoke.mjs <worker-url>
//   node test/e2e-smoke.mjs https://elsewhen-relay.<sub>.workers.dev
//
// 注意：会在你的邮箱里留下测试凭证（mailbox_key 由随机 cred 派生，每次运行都是
// 新邮箱，互不干扰）；邮件会 ack 删掉，nonce 10 分钟后自动过期。

import { createHmac, createHash, randomBytes } from "node:crypto";

const BASE = process.argv[2]?.replace(/\/$/, "");
if (!BASE) {
  console.error("用法: node test/e2e-smoke.mjs <worker-url>");
  process.exit(2);
}

const hmac = (keyHex, parts) =>
  createHmac("sha256", Buffer.from(keyHex, "hex")).update(parts.join("|")).digest("hex");
const sha256 = (s) => createHash("sha256").update(s, "utf8").digest("hex");
const hex = (n) => randomBytes(n).toString("hex");
const mailboxKey = (credHex) => hmac(credHex, ["relay-mailbox"]);

let failures = 0;
function check(name, cond, detail = "") {
  const ok = !!cond;
  console.log(`${ok ? "✅" : "❌"} ${name}${ok ? "" : `  ← ${detail}`}`);
  if (!ok) failures++;
}

async function post(path, body) {
  const r = await fetch(BASE + path, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(body),
  });
  return { status: r.status, body: await r.json().catch(() => ({})) };
}
async function get(path, params) {
  const r = await fetch(BASE + path + "?" + new URLSearchParams(params));
  return { status: r.status, body: await r.json().catch(() => ({})) };
}

// --- 两台"设备"：各自有 relay_cred（真实场景由扫码绑定派生），mailbox_key 是哈希 ---
const credA = hex(32), credB = hex(32);
const mbA = mailboxKey(credA); // A 的邮箱
const mbB = mailboxKey(credB); // B 的邮箱（A 往这里发）

console.log(`\n目标: ${BASE}`);
console.log(`设备A 邮箱: ${mbA.slice(0, 12)}...  设备B 邮箱: ${mbB.slice(0, 12)}...\n`);

// 0. health
{
  const r = await get("/health", {});
  check("GET /health 存活", r.status === 200 && r.body.ok === true, JSON.stringify(r));
}

// 1. 注册：A 作为发件人，在 B 的邮箱注册 A 的 cred；B 在自己邮箱注册 self→self（取件自证）
const credExp = Date.now() + 3600_000;
{
  const r1 = await post("/register", {
    mailbox_key: mbB, sender_mb: mbA, cred_hex: credA, expires_at: credExp,
    sig: hmac(credA, ["relay-register", mbA, mbB, String(credExp)]),
  });
  check("register: A→B 邮箱注册发件凭证", r1.status === 200 && r1.body.ok, JSON.stringify(r1));

  const r2 = await post("/register", {
    mailbox_key: mbB, sender_mb: mbB, cred_hex: credB, expires_at: credExp,
    sig: hmac(credB, ["relay-register", mbB, mbB, String(credExp)]),
  });
  check("register: B 注册 self→self 取件凭证", r2.status === 200 && r2.body.ok, JSON.stringify(r2));

  const r3 = await post("/register", {
    mailbox_key: mbB, sender_mb: mbA, cred_hex: credA, expires_at: credExp,
    sig: hmac(credA, ["relay-register", mbA, mbB, String(credExp)]),
  });
  check("register: 重复注册被拒绝 (409)", r3.status === 409, `got ${r3.status}`);
}

// 2. A 发一封"端到端密文"（这里是伪密文，中继和本脚本都不解密）
const ciphertext = Buffer.from(`e2e-smoke-${Date.now()}`).toString("base64");
const exp = Date.now() + 7 * 24 * 3600_000; // 会被服务端 clamp 到 now+MAIL_TTL_MS，一致
let mailId;
{
  const ts = Date.now(), nonce = hex(16);
  const sig = hmac(credA, ["relay-send", mbA, mbB, "1", String(exp), String(ts), nonce, sha256(ciphertext)]);
  const r = await post("/send", {
    mailbox_key: mbB, sender_mb: mbA, seq_hint: 1,
    ciphertext, expires_at: exp, ts, nonce, sig,
  });
  mailId = r.body.id;
  check("send: 合法签名投递成功", r.status === 200 && !!mailId, JSON.stringify(r));

  const r2 = await post("/send", {
    mailbox_key: mbB, sender_mb: mbA, seq_hint: 1,
    ciphertext, expires_at: exp, ts, nonce, sig,
  });
  check("send: 同 nonce 重放被拒绝 (409)", r2.status === 409, `got ${r2.status}`);

  const ts3 = Date.now(), nonce3 = hex(16);
  const badSig = hmac(credB, ["relay-send", mbA, mbB, "1", String(exp), String(ts3), nonce3, sha256(ciphertext)]); // 错的 cred
  const r3 = await post("/send", {
    mailbox_key: mbB, sender_mb: mbA, seq_hint: 1,
    ciphertext, expires_at: exp, ts: ts3, nonce: nonce3, sig: badSig,
  });
  check("send: 错误凭证签名被拒绝 (403)", r3.status === 403, `got ${r3.status}`);
}

// 3. B 查状态 + 取件
{
  const ts = Date.now(), nonce = hex(16);
  const sig = hmac(credB, ["relay-status", mbB, String(ts), nonce]);
  const r = await get("/status", { mailbox_key: mbB, recipient_mb: mbB, ts, nonce, sig });
  check("status: 能看到 1 条待取", r.status === 200 && r.body.count >= 1, JSON.stringify(r.body));

  const ts2 = Date.now(), nonce2 = hex(16);
  const sig2 = hmac(credB, ["relay-fetch", mbB, "0", "10", String(ts2), nonce2]);
  const r2 = await get("/fetch", { mailbox_key: mbB, recipient_mb: mbB, after: "0", limit: "10", ts: ts2, nonce: nonce2, sig: sig2 });
  const found = r2.body.envelopes?.find((e) => e.id === mailId);
  check("fetch: 取到 A 发的信封", r2.status === 200 && !!found, JSON.stringify(r2.body).slice(0, 200));
  check("fetch: 密文原样返回（中继未篡改）", found?.ciphertext === ciphertext);
  check("fetch: seq_hint 保留", found?.seq_hint === 1);
}

// 4. B ack 删除
{
  const ts = Date.now(), nonce = hex(16);
  const sig = hmac(credB, ["relay-ack", mbB, mailId, String(ts), nonce]);
  const r = await post("/ack", { mailbox_key: mbB, recipient_mb: mbB, ids: [mailId], ts, nonce, sig });
  check("ack: 确认删除", r.status === 200 && r.body.deleted === 1, JSON.stringify(r));

  const ts2 = Date.now(), nonce2 = hex(16);
  const sig2 = hmac(credB, ["relay-fetch", mbB, "0", "10", String(ts2), nonce2]);
  const r2 = await get("/fetch", { mailbox_key: mbB, recipient_mb: mbB, after: "0", limit: "10", ts: ts2, nonce: nonce2, sig: sig2 });
  check("ack 后 fetch: 信封已不存在", !r2.body.envelopes?.some((e) => e.id === mailId));
}

// 5. 发现：A 心跳，B 在 B 的邮箱视角应该看不到 A（presence 存在各设备自己的邮箱里）
//    协议设计：A 把 endpoint 存进 A 自己的邮箱；B 问 A 的邮箱需持 A→A 凭证不行——
//    实际是 B 问"我自己的邮箱里，对端 A 是否在线"。A 需先在自己邮箱注册 self cred 并心跳。
{
  // A 在自己邮箱注册 self→self + 心跳
  const r0 = await post("/register", {
    mailbox_key: mbA, sender_mb: mbA, cred_hex: credA, expires_at: credExp,
    sig: hmac(credA, ["relay-register", mbA, mbA, String(credExp)]),
  });
  check("register: A 注册 self→self（presence 用）", r0.status === 200, JSON.stringify(r0));

  const endpointBlob = Buffer.from(JSON.stringify({ addr: "192.168.1.10", port: 47777, device_id: "devA" })).toString("base64");
  const ts = Date.now(), nonce = hex(16), ttl = 120_000;
  const sig = hmac(credA, ["relay-presence", mbA, sha256(endpointBlob), String(ttl), String(ts), nonce]);
  const r1 = await post("/presence", { mailbox_key: mbA, endpoint_blob: endpointBlob, ttl_ms: ttl, ts, nonce, sig });
  check("presence: A 心跳上报在线", r1.status === 200 && r1.body.ok, JSON.stringify(r1));

  // A 查自己的 peers：A 的邮箱里只注册了 A 自己，没有对端 → 空
  const ts2 = Date.now(), nonce2 = hex(16);
  const sig2 = hmac(credA, ["relay-peers", mbA, String(ts2), nonce2]);
  const r2 = await get("/peers", { mailbox_key: mbA, ts: ts2, nonce: nonce2, sig: sig2 });
  check("peers: A 邮箱无已绑定对端 → 空列表", r2.status === 200 && r2.body.peers?.length === 0, JSON.stringify(r2.body));
}

console.log(failures === 0 ? "\n全部通过 🎉 relay 工作正常\n" : `\n${failures} 项失败\n`);
process.exit(failures === 0 ? 0 : 1);
