/**
 * 信箱库 schema 升级路径的单测（review m10-c）。
 *
 * 起因：老库里攒了重复 `(sender_mb, seq_hint)` 行，直接建唯一索引会在启动时抛错。
 * 原来的补救是 `try { CREATE UNIQUE INDEX } catch { 去重; 再建 }` —— 但那个 catch
 * 吞掉了**一切**异常：
 *   - `SQLITE_BUSY`（sweeper 正在 VACUUM 同一个信箱）会被当成「有重复行」，
 *     接着在同一张写锁住的库上跑两条 DELETE，报的还是另一个错；
 *   - 磁盘满 / SQL 写错同样被当成「有重复行」，真正的根因被吃掉；
 *   - 两条 DELETE 不在同一个事务里：先删信后删分片，中途失败就留下孤儿分片
 *     永久占着配额。
 * 这里逐条钉住：只有唯一约束冲突才走去重，其余异常原样抛出，去重是原子的。
 */
import assert from 'node:assert/strict';
import { mkdtempSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';
import { DatabaseSync } from 'node:sqlite';
import { ensureIdempotencyIndex, openMailboxDb } from './schema.mjs';

const file = () => join(mkdtempSync(join(tmpdir(), 'relay-schema-')), 'mb.db');
const SENDER = 'a'.repeat(64);

/** 造一个「老库」：有重复行、有分片，但**没有**幂等索引。 */
function legacy(path, rows) {
  const db = new DatabaseSync(path);
  db.exec('CREATE TABLE relay_v2_mail (id INTEGER PRIMARY KEY AUTOINCREMENT, sender_mb TEXT NOT NULL, seq_hint INTEGER NOT NULL, bytes INTEGER NOT NULL, created_at INTEGER NOT NULL, expires_at INTEGER NOT NULL)');
  db.exec('CREATE TABLE relay_v2_chunks (mail_id INTEGER NOT NULL, part INTEGER NOT NULL, ciphertext TEXT NOT NULL, PRIMARY KEY(mail_id,part))');
  const insertMail = db.prepare('INSERT INTO relay_v2_mail (sender_mb,seq_hint,bytes,created_at,expires_at) VALUES (?,?,?,?,?)');
  const insertChunk = db.prepare('INSERT INTO relay_v2_chunks (mail_id,part,ciphertext) VALUES (?,?,?)');
  for (const row of rows) {
    const { lastInsertRowid } = insertMail.run(row.sender ?? SENDER, row.seq, 10, 1, 2);
    for (let part = 0; part < (row.parts ?? 1); part += 1) insertChunk.run(lastInsertRowid, part, `chunk-${part}`);
  }
  db.close();
}

const ids = (db) => db.prepare('SELECT id FROM relay_v2_mail ORDER BY id').all().map((row) => Number(row.id));
const seqs = (db) => db.prepare('SELECT seq_hint FROM relay_v2_mail ORDER BY id').all().map((row) => Number(row.seq_hint));
const chunkOwners = (db) => db.prepare('SELECT mail_id FROM relay_v2_chunks ORDER BY mail_id').all().map((row) => Number(row.mail_id));

test('老库里的重复行由升级路径去重：保留最大 id，孤儿分片一并清掉', () => {
  const path = file();
  legacy(path, [
    { seq: 1, parts: 2 },
    { seq: 1, parts: 2 },
    { seq: 1, parts: 1 },
    { seq: 0, parts: 1 },
    { seq: 0, parts: 1 },
  ]);
  const db = openMailboxDb(path);
  try {
    // 同一 (sender, seq>0) 只留最大 id；seq=0 表示「本轮没有稳定序号」，必须都能留。
    assert.deepEqual(seqs(db), [1, 0, 0]);
    assert.deepEqual(ids(db), [3, 4, 5]);
    // 被去重掉的信的分片不能留成孤儿（否则永久白占配额）。
    assert.deepEqual([...new Set(chunkOwners(db))], [3, 4, 5]);
    // 索引真的建起来了：同键普通 INSERT 会被唯一约束拦住，seq=0 仍可重复。
    const insert = db.prepare('INSERT INTO relay_v2_mail (sender_mb,seq_hint,bytes,created_at,expires_at) VALUES (?,?,?,?,?)');
    assert.throws(() => insert.run(SENDER, 1, 10, 1, 2), /UNIQUE constraint failed/);
    assert.equal(insert.run(SENDER, 0, 10, 1, 2).changes, 1);
  } finally { db.close(); }
});

test('已经建好索引的库重复打开不改动任何一行（幂等）', () => {
  const path = file();
  legacy(path, [{ seq: 7 }]);
  for (let round = 0; round < 3; round += 1) {
    const db = openMailboxDb(path);
    assert.deepEqual(ids(db), [1]);
    db.close();
  }
});

/** 记录所有语句的假库，用来观察「失败时到底跑了什么」。 */
function fakeDb(failOnIndexWith, { failDeletesAt = -1 } = {}) {
  const statements = [];
  let indexAttempts = 0;
  let deletes = 0;
  return {
    statements,
    exec(sql) { statements.push(sql); },
    prepare(sql) {
      statements.push(sql);
      return {
        run() {
          if (sql.includes('CREATE UNIQUE INDEX')) {
            indexAttempts += 1;
            if (indexAttempts === 1 && failOnIndexWith) throw failOnIndexWith;
            return { changes: 0 };
          }
          deletes += 1;
          if (deletes === failDeletesAt) throw new Error('disk I/O error');
          return { changes: 1 };
        },
      };
    },
  };
}

test('只有唯一约束冲突才走去重，其它异常原样抛出且一条 DELETE 都不跑', () => {
  const busy = Object.assign(new Error('database is locked'), { errcode: 5 });
  const disk = new Error('disk I/O error');
  for (const error of [busy, disk]) {
    const db = fakeDb(error);
    assert.throws(() => ensureIdempotencyIndex(db), (thrown) => thrown === error,
      '非 UNIQUE 异常必须原样抛出，不能被当成「有重复行」');
    assert.equal(db.statements.filter((sql) => sql.startsWith('DELETE')).length, 0);
    assert.equal(db.statements.includes('BEGIN IMMEDIATE'), false);
    assert.equal(db.statements.includes('COMMIT'), false);
  }
});

test('去重是一个事务：任一条 DELETE 失败就整体回滚', () => {
  const unique = new Error('UNIQUE constraint failed: relay_v2_mail.sender_mb, relay_v2_mail.seq_hint');
  const db = fakeDb(unique, { failDeletesAt: 2 });
  assert.throws(() => ensureIdempotencyIndex(db), /disk I\/O error/);
  assert.equal(db.statements.includes('BEGIN IMMEDIATE'), true, '两条 DELETE 必须在同一个事务里');
  assert.equal(db.statements.includes('COMMIT'), false, '失败不能提交半份去重');
  assert.equal(db.statements.at(-1), 'ROLLBACK', '必须回滚，不能留下「信删了分片还在」');
});

test('去重成功后重试建索引，且写事务已提交', () => {
  const unique = new Error('UNIQUE constraint failed: relay_v2_mail.sender_mb, relay_v2_mail.seq_hint');
  const db = fakeDb(unique);
  assert.doesNotThrow(() => ensureIdempotencyIndex(db));
  const exec = db.statements.filter((sql) => sql === 'BEGIN IMMEDIATE' || sql === 'COMMIT' || sql === 'ROLLBACK');
  assert.deepEqual(exec, ['BEGIN IMMEDIATE', 'COMMIT']);
  // 建索引 -> 去重两条 -> 再建索引
  assert.equal(db.statements.filter((sql) => sql.startsWith('CREATE UNIQUE INDEX')).length, 2);
  assert.equal(db.statements.filter((sql) => sql.startsWith('DELETE')).length, 2);
});
