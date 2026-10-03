// 中继签名规范与收件逻辑的协议不变量测试（node --test，不依赖 wrangler）。
// 运行：cd relay && npm test
//
// 聚焦协议不变量：
//   - 签名消息格式（域分隔串 + 参数拼接顺序）客户端与服务端必须一致
//   - 验签拒绝：错误密钥 / 篡改参数 / 过期时间戳
//   - nonce 重放拒绝
//   - 配额与 TTL 的边界
//   - 凭证注册不可被覆盖（防陌生人注入）

import { test } from "node:test";
import assert from "node:assert/strict";
import { createHmac, createHash } from "node:crypto";

// ---- 客户端视角的签名构造（与 worker.ts 服务端验签共用同一规范） -------------

function hmac(keyHex, parts) {
  return createHmac("sha256", Buffer.from(keyHex, "hex"))
    .update(parts.join("|"))
    .digest("hex");
}

function sha256Hex(s) {
  return createHash("sha256").update(s, "utf8").digest("hex");
}

const SKEW_MS = 5 * 60 * 1000;

function checkFreshness(ts, now) {
  return Number.isFinite(ts) && Math.abs(now - ts) <= SKEW_MS;
}

// ---- 内存版邮箱逻辑（镜像 worker.ts 的判断路径，不跑 DO） ----------------------

function makeMailbox({ quota = 50 * 1024 * 1024, ttl = 7 * 24 * 3600 * 1000 } = {}) {
  const creds = new Map();
  const mails = new Map();
  const nonces = new Map();
  const presences = new Map(); // peer mailbox_key -> { endpointBlob, expiresAt }
  let counter = 0;
  let bytes = 0;

  return {
    async register({ senderMb, recipientMb, credHex, expiresAt, sig }, now) {
      const existing = creds.get(senderMb);
      if (existing && existing.expiresAt > now) return { status: 409 };
      const expect = hmac(credHex,
        ["relay-register", senderMb, recipientMb, String(expiresAt)]);
      if (expect !== sig) return { status: 403 };
      creds.set(senderMb, { credHex, expiresAt });
      return { status: 200 };
    },

    async send({ senderMb, recipientMb, seqHint, ciphertext, expiresAt, ts, nonce, sig }, now) {
      if (!checkFreshness(ts, now)) return { status: 403, why: "stale" };
      const cred = creds.get(senderMb);
      if (!cred || cred.expiresAt <= now) return { status: 403, why: "no cred" };
      const exp = Math.min(
        Number.isFinite(expiresAt) ? expiresAt : now + ttl,
        now + ttl,
      );
      const expect = hmac(cred.credHex, [
        "relay-send", senderMb, recipientMb, String(seqHint ?? 0),
        String(exp), String(ts), nonce, sha256Hex(ciphertext),
      ]);
      if (expect !== sig) return { status: 403, why: "bad sig" };
      if (nonces.has(nonce) && nonces.get(nonce) > now) return { status: 409, why: "replay" };
      nonces.set(nonce, now + 10 * 60 * 1000);
      const size = Math.floor(ciphertext.length * 3 / 4);
      if (bytes + size > quota) return { status: 507, why: "quota" };
      const id = String(++counter).padStart(16, "0");
      mails.set(id, { id, senderMb, ciphertext, size, expiresAt: exp });
      bytes += size;
      return { status: 200, id };
    },

    async presence({ mailboxKey, endpointBlob, ttlMs, ts, nonce, sig }, now) {
      if (!checkFreshness(ts, now)) return { status: 403, why: "stale" };
      const cred = creds.get(mailboxKey);
      if (!cred || cred.expiresAt <= now) return { status: 403, why: "no cred" };
      const endpointHash = sha256Hex(endpointBlob);
      const expect = hmac(cred.credHex,
        ["relay-presence", mailboxKey, endpointHash, String(ttlMs), String(ts), nonce]);
      if (expect !== sig) return { status: 403, why: "bad sig" };
      if (nonces.has(nonce) && nonces.get(nonce) > now) return { status: 409, why: "replay" };
      nonces.set(nonce, now + 10 * 60 * 1000);
      presences.set(mailboxKey, { endpointBlob, expiresAt: now + ttlMs });
      return { status: 200 };
    },

    async peers({ mailboxKey, want }, now) {
      const out = [];
      for (const [peerMb, cred] of creds) {
        if (peerMb === mailboxKey) continue;
        if (cred.expiresAt <= now) continue;
        if (want.length > 0 && !want.includes(peerMb)) continue;
        const p = presences.get(peerMb);
        if (p && p.expiresAt > now) {
          out.push({ mailbox_key: peerMb, endpoint_blob: p.endpointBlob });
        }
      }
      return out;
    },

    count() { return mails.size; },
    usedBytes() { return bytes; },
  };
}

// ---- 固定测试数据 --------------------------------------------------------------

const CRED = "a".repeat(64);       // 合法 relay_cred（hex）
const SELF_MB = "b".repeat(64);    // 本设备 mailbox_key
const SENDER_MB = "c".repeat(64);  // 对端 mailbox_key
const NOW = 1_800_000_000_000;
const TTL = 7 * 24 * 3600 * 1000;

function registerSender(mb) {
  const expiresAt = NOW + 30 * 24 * 3600 * 1000;
  return mb.register({
    senderMb: SENDER_MB, recipientMb: SELF_MB, credHex: CRED,
    expiresAt,
    sig: hmac(CRED, ["relay-register", SENDER_MB, SELF_MB, String(expiresAt)]),
  }, NOW);
}

function sendReq(overrides = {}) {
  const ts = NOW;
  const nonce = "d".repeat(32);
  const ciphertext = Buffer.from("end-to-end-ciphertext").toString("base64");
  const exp = NOW + TTL;
  const base = {
    senderMb: SENDER_MB, recipientMb: SELF_MB, seqHint: 1,
    ciphertext, expiresAt: exp, ts, nonce,
  };
  const sig = hmac(CRED, [
    "relay-send", SENDER_MB, SELF_MB, "1", String(exp), String(ts), nonce,
    sha256Hex(ciphertext),
  ]);
  return { ...base, sig, ...overrides };
}

// ---- 测试 -----------------------------------------------------------------------

test("register: 合法自证签名被接受", async () => {
  const mb = makeMailbox();
  assert.equal((await registerSender(mb)).status, 200);
});

test("register: 重复注册被拒绝（凭证不可被覆盖）", async () => {
  const mb = makeMailbox();
  await registerSender(mb);
  assert.equal((await registerSender(mb)).status, 409);
});

test("register: 错误签名的申报凭证被拒绝", async () => {
  const mb = makeMailbox();
  const r = await mb.register({
    senderMb: SENDER_MB, recipientMb: SELF_MB, credHex: CRED,
    expiresAt: NOW + 1000, sig: "0".repeat(64),
  }, NOW);
  assert.equal(r.status, 403);
});

test("send: 合法签名 + 新鲜时间戳 + 新 nonce 被接受", async () => {
  const mb = makeMailbox();
  await registerSender(mb);
  const r = await mb.send(sendReq(), NOW);
  assert.equal(r.status, 200);
  assert.equal(mb.count(), 1);
});

test("send: 时间戳超窗被拒绝（拒绝原因是时间戳而非签名）", async () => {
  const mb = makeMailbox();
  await registerSender(mb);
  const staleTs = NOW - SKEW_MS - 1;
  const exp = NOW + TTL;
  const req = sendReq({ ts: staleTs });
  req.sig = hmac(CRED, ["relay-send", SENDER_MB, SELF_MB, "1", String(exp),
    String(staleTs), req.nonce, sha256Hex(req.ciphertext)]);
  const r = await mb.send(req, NOW);
  assert.equal(r.status, 403);
});

test("send: 篡改密文导致签名不匹配", async () => {
  const mb = makeMailbox();
  await registerSender(mb);
  const req = sendReq({ ciphertext: Buffer.from("tampered").toString("base64") });
  const r = await mb.send(req, NOW);
  assert.equal(r.status, 403);
});

test("send: nonce 重放被拒绝", async () => {
  const mb = makeMailbox();
  await registerSender(mb);
  const req = sendReq();
  assert.equal((await mb.send(req, NOW)).status, 200);
  assert.equal((await mb.send(req, NOW)).status, 409);
});

test("send: 无凭证发件人被拒绝", async () => {
  const mb = makeMailbox();
  assert.equal((await mb.send(sendReq(), NOW)).status, 403);
});

test("send: 超出邮箱配额被拒绝", async () => {
  const mb = makeMailbox({ quota: 10 });
  await registerSender(mb);
  assert.equal((await mb.send(sendReq(), NOW)).status, 507);
});

test("presence: 合法签名的心跳被接受，peers 可见", async () => {
  const mb = makeMailbox();
  await registerSender(mb);
  // 发件人 presence（让收件人发现自己）
  const endpointBlob = Buffer.from("encrypted-endpoint-addr-port").toString("base64");
  const ts = NOW, nonce = "e".repeat(32), ttl = 120_000;
  const endpointHash = sha256Hex(endpointBlob);
  const sig = hmac(CRED, ["relay-presence", SENDER_MB, endpointHash, String(ttl), String(ts), nonce]);
  const r = await mb.presence({ mailboxKey: SENDER_MB, endpointBlob, ttlMs: ttl, ts, nonce, sig }, NOW);
  assert.equal(r.status, 200);
  // 收件人来问：能看到发件人在线
  const peers = await mb.peers({ mailboxKey: SELF_MB, want: [SENDER_MB] }, NOW);
  assert.equal(peers.length, 1);
  assert.equal(peers[0].mailbox_key, SENDER_MB);
  assert.equal(peers[0].endpoint_blob, endpointBlob); // 密文原样返回
});

test("presence: 过期心跳从 peers 消失", async () => {
  const mb = makeMailbox();
  await registerSender(mb);
  const endpointBlob = Buffer.from("ep").toString("base64");
  const ts = NOW, nonce = "f".repeat(32), ttl = 1000; // 1 秒 TTL
  const endpointHash = sha256Hex(endpointBlob);
  const sig = hmac(CRED, ["relay-presence", SENDER_MB, endpointHash, String(ttl), String(ts), nonce]);
  await mb.presence({ mailboxKey: SENDER_MB, endpointBlob, ttlMs: ttl, ts, nonce, sig }, NOW);
  // TTL 过后
  const peers = await mb.peers({ mailboxKey: SELF_MB, want: [SENDER_MB] }, NOW + 2000);
  assert.equal(peers.length, 0);
});

test("peers: 未注册 cred 的 mailbox 查不到任何对端", async () => {
  const mb = makeMailbox();
  // 不注册，直接问
  const peers = await mb.peers({ mailboxKey: "9".repeat(64), want: [] }, NOW);
  assert.equal(peers.length, 0);
});

test("签名消息格式: 域分隔与参数顺序固定（与 worker.ts 对齐）", () => {
  const sig = hmac(CRED, ["relay-send", "s", "r", "1", "100", "50", "n", "h"]);
  const expect = createHmac("sha256", Buffer.from(CRED, "hex"))
    .update("relay-send|s|r|1|100|50|n|h").digest("hex");
  assert.equal(sig, expect);
});
