/**
 * relay 磁盘回收（R2-6）契约：草稿删行不缩文件是事故的根因之一，
 * 所以这里断言的是**磁盘字节真的降下来**，而不是「行数变少了」。
 */
import test from 'node:test';
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { mkdirSync, mkdtempSync, statSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, dirname } from 'node:path';
import { randomBytes } from 'node:crypto';
import { DatabaseSync } from 'node:sqlite';
import { fileURLToPath } from 'node:url';
import { openMailboxDb, purgeExpired } from './schema.mjs';
import { sweepAll, sweepMailbox } from './sweeper.mjs';

const here = dirname(fileURLToPath(import.meta.url));

function tempMailboxDir() {
  const dataDir = mkdtempSync(join(tmpdir(), 'relay-sweep-'));
  mkdirSync(join(dataDir, 'mailboxes'), { recursive: true });
  return dataDir;
}

/** 灌入再删掉 n 封 4 KB 邮件：制造事故现场那种「文件大、活数据少」。 */
function churn(db, n = 300) {
  const body = 'A'.repeat(4 * 1024);
  const now = Date.now();
  for (let i = 1; i <= n; i++) {
    const { id } = db.prepare('INSERT INTO relay_v2_mail (sender_mb,seq_hint,bytes,created_at,expires_at) VALUES (?,?,?,?,?) RETURNING id')
      .get('b'.repeat(64), i, body.length, now, now + 60_000);
    db.prepare('INSERT INTO relay_v2_chunks (mail_id,part,ciphertext) VALUES (?,?,?)').run(Number(id), 0, body);
  }
  db.prepare('DELETE FROM relay_v2_chunks').run();
  db.prepare('DELETE FROM relay_v2_mail').run();
}

test('sweep 把被删行撑大的信箱库缩回原尺寸', () => {
  const dataDir = tempMailboxDir();
  const file = join(dataDir, 'mailboxes', `${'c'.repeat(64)}.db`);
  const db = openMailboxDb(file);
  churn(db);
  db.close();
  const before = statSync(file).size;
  // 前提：确实撑大了，否则这条用例什么也没证明
  assert.ok(before > 512 * 1024, `churn 应该把库撑大，实际 ${before} 字节`);
  const result = sweepMailbox(file);
  const after = statSync(file).size;
  assert.equal(result.vacuumed, true, 'freelist 占比远超阈值时必须 VACUUM');
  assert.ok(after * 4 < before, `VACUUM 应显著缩文件：${before} → ${after}`);
  assert.ok(after < 128 * 1024, `空信箱应回到 ~100 KB 量级，实际 ${after} 字节`);
  // 表结构不能被回收路径破坏
  const check = new DatabaseSync(file);
  assert.equal(check.prepare('SELECT COUNT(*) AS n FROM relay_v2_mail').get().n, 0);
  assert.equal(check.prepare("SELECT name FROM sqlite_master WHERE name='relay_v2_mail_idem'").get().name, 'relay_v2_mail_idem');
  check.close();
});

test('sweep 只看目录，退役设备（再无请求）的信箱也会被清', () => {
  const dataDir = tempMailboxDir();
  const file = join(dataDir, 'mailboxes', `${'d'.repeat(64)}.db`);
  const db = openMailboxDb(file);
  const now = Date.now();
  // 一封已过期 + 一封未过期：只清过期的
  db.prepare('INSERT INTO relay_v2_mail (sender_mb,seq_hint,bytes,created_at,expires_at) VALUES (?,?,?,?,?)')
    .run('e'.repeat(64), 1, 10, now - 1000, now - 500);
  db.prepare('INSERT INTO relay_v2_chunks (mail_id,part,ciphertext) VALUES (?,?,?)').run(1, 0, 'stale');
  db.prepare('INSERT INTO relay_v2_mail (sender_mb,seq_hint,bytes,created_at,expires_at) VALUES (?,?,?,?,?)')
    .run('e'.repeat(64), 2, 10, now, now + 60_000);
  db.prepare('INSERT INTO relay_v2_chunks (mail_id,part,ciphertext) VALUES (?,?,?)').run(2, 0, 'live');
  db.close();
  const result = sweepAll(dataDir);
  assert.equal(result.swept, 1);
  const check = new DatabaseSync(file);
  const rows = check.prepare('SELECT id FROM relay_v2_mail ORDER BY id').all();
  assert.deepEqual(rows.map((r) => Number(r.id)), [2], '过期邮件被清、未过期邮件保留');
  assert.deepEqual(check.prepare('SELECT mail_id FROM relay_v2_chunks').all().map((r) => Number(r.mail_id)), [2]);
  check.close();
});

test('openMailboxDb 幂等：重复打开同一个库不清数据', () => {
  const dataDir = tempMailboxDir();
  const file = join(dataDir, 'mailboxes', `${'f'.repeat(64)}.db`);
  const first = openMailboxDb(file);
  const now = Date.now();
  first.prepare('INSERT INTO relay_v2_mail (sender_mb,seq_hint,bytes,created_at,expires_at) VALUES (?,?,?,?,?)')
    .run('a'.repeat(64), 1, 4, now, now + 60_000);
  assert.equal(purgeExpired(first, now), 0);
  first.close();
  const second = openMailboxDb(file);
  assert.equal(second.prepare('SELECT COUNT(*) AS n FROM relay_v2_mail').get().n, 1);
  second.close();
});

test('句柄 LRU：信箱数超过上限后旧句柄被关闭但请求仍能重新打开', async () => {
  const dataDir = mkdtempSync(join(tmpdir(), 'relay-lru-'));
  const proc = spawn(process.execPath, [join(here, 'server.mjs')], {
    env: { ...process.env, PORT: '0', DATA_DIR: dataDir, HANDLE_LIMIT: '2', ELSEWHEN_DISABLE_SWEEP: '1' },
    stdio: ['ignore', 'pipe', 'inherit'],
  });
  const port = await new Promise((resolve, reject) => {
    proc.stdout.on('data', (chunk) => {
      const match = /listening on [^\n]*:(\d+)/.exec(String(chunk));
      if (match) resolve(Number(match[1]));
    });
    proc.on('exit', (code) => reject(new Error(`relay exited early: ${code}`)));
  });
  const { key, auth, mailbox } = await import('../test/helpers.mjs');
  const create = async (credential) => {
    const mb = mailbox(credential);
    const a = auth(credential, 'create', [mb]);
    const res = await fetch(`http://127.0.0.1:${port}/create`, {
      method: 'POST', headers: { 'content-type': 'application/json' },
      body: JSON.stringify({ version: 2, mailbox_key: mb, owner_cred_hex: credential, ...a }),
    });
    return { mb, credential, body: await res.json(), status: res.status };
  };
  const status = async (mb, credential) => {
    const a = auth(credential, 'status', [mb]);
    const res = await fetch(`http://127.0.0.1:${port}/status?version=2&mailbox_key=${mb}&ts=${a.ts}&nonce=${a.nonce}&sig=${a.sig}`);
    return { status: res.status, body: await res.json() };
  };
  try {
    const a = await create(key());
    const b = await create(key());
    assert.equal(b.status, 200);
    const c = await create(key()); // 超过 HANDLE_LIMIT=2，最早的 a 句柄被淘汰
    assert.equal(c.status, 200);
    assert.equal(a.status, 200);
    // a 的句柄已被关闭：重新请求必须能重新打开并正常应答，而不是炸掉/丢数据
    const reopened = await status(a.mb, a.credential);
    assert.equal(reopened.status, 200);
    assert.equal(reopened.body.ok, true);
    assert.equal(reopened.body.count, 0);
    const again = await create(key()); // 继续加压：不崩
    assert.equal(again.status, 200);
  } finally { proc.kill(); }
});
