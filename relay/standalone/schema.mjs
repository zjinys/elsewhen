// 信箱库的表结构与 PRAGMA 只写一遍：`server.mjs` 与 `sweeper.mjs` 都要开库，
// 两处各写一份迟早漂移（只在一侧补幂等索引，另一侧 sweep 完就缺了）。
import { DatabaseSync } from 'node:sqlite';

/** 建投递幂等索引；只有「唯一约束被现有重复行挡住」才走去重。
 *
 * 老库里可能已经攒了重复行（历史上每次 tick 重发都新插一行），直接建唯一索引
 * 会在启动时抛错把 relay 打死 —— 先按「保留最大 id」去重再建，去重掉的正是
 * 同一封信的旧副本，语义上无损。
 *
 * 但只认「唯一约束」这一种失败。原来这里 `catch {}` 吞掉**一切**异常：
 * `SQLITE_BUSY`（sweeper 正在写同一个信箱）、磁盘满、SQL 写错都会被当成
 * 「有重复行」，接着在同一张写锁住的库上跑两条 DELETE，报的还是另一个错 ——
 * 排查时看到的现象与根因完全无关。
 *
 * 两条 DELETE 必须同一个事务：只删信不删分片会留下一堆孤儿分片白占配额；
 * 反过来中途失败就是删掉了新信还在引用的分片。
 */
export function ensureIdempotencyIndex(db) {
  const idemIndex = 'CREATE UNIQUE INDEX IF NOT EXISTS relay_v2_mail_idem ON relay_v2_mail(sender_mb,seq_hint) WHERE seq_hint>0';
  try {
    db.prepare(idemIndex).run();
    return;
  } catch (error) {
    if (!/UNIQUE constraint failed/.test(String(error?.message ?? ''))) throw error;
  }
  db.exec('BEGIN IMMEDIATE');
  try {
    db.prepare('DELETE FROM relay_v2_mail WHERE seq_hint>0 AND id NOT IN (SELECT MAX(id) FROM relay_v2_mail WHERE seq_hint>0 GROUP BY sender_mb,seq_hint)').run();
    db.prepare('DELETE FROM relay_v2_chunks WHERE mail_id NOT IN (SELECT id FROM relay_v2_mail)').run();
    db.exec('COMMIT');
  } catch (inner) {
    try { db.exec('ROLLBACK'); } catch { /* 已回滚 */ }
    throw inner;
  }
  db.prepare(idemIndex).run();
}

/** 拿不到写锁时等多久（毫秒）。
 *
 * 默认 5 s 对线上足够（正常争用只有 sweeper 的 VACUUM），环境变量是为了让
 * 「写锁被占住」这条路径能被测试确定性地触发 —— 否则一个用例要空等 5 s。
 */
export function busyTimeoutMs() {
  const parsed = Number(process.env.SQLITE_BUSY_TIMEOUT_MS);
  return Number.isSafeInteger(parsed) && parsed >= 1 && parsed <= 60_000 ? parsed : 5000;
}

/**
 * 打开（必要时建立）一个信箱库。调用方负责 `close()`。
 *
 * `auto_vacuum=INCREMENTAL` 只在**空库**上真正生效，这正是我们想要的：
 * 新信箱从第一张表起就带增量回收能力，不必等 sweep 重写整个文件；
 * 老库上写这个 pragma 只是登记为「下次 VACUUM 后生效」，不会就地改写文件，
 * 所以对现网 690 MB 的信箱也安全 —— 它们会由 sweeper 的 `VACUUM` 转过去。
 */
export function openMailboxDb(filePath) {
  const db = new DatabaseSync(filePath);
  db.exec('PRAGMA journal_mode = WAL');
  db.exec(`PRAGMA busy_timeout = ${busyTimeoutMs()}`);
  db.exec('PRAGMA auto_vacuum = INCREMENTAL');
  db.exec(`
    CREATE TABLE IF NOT EXISTS relay_v2_owner (singleton INTEGER PRIMARY KEY CHECK(singleton=1), mailbox_key TEXT NOT NULL, cred_hex TEXT NOT NULL, generation INTEGER NOT NULL);
    CREATE TABLE IF NOT EXISTS relay_v2_senders (sender_mb TEXT PRIMARY KEY, cred_hex TEXT, expires_at INTEGER NOT NULL, revision INTEGER NOT NULL);
    CREATE TABLE IF NOT EXISTS relay_v2_nonces (nonce TEXT PRIMARY KEY, expires_at INTEGER NOT NULL);
    CREATE INDEX IF NOT EXISTS relay_v2_nonce_expiry ON relay_v2_nonces(expires_at);
    CREATE TABLE IF NOT EXISTS relay_v2_mail (id INTEGER PRIMARY KEY AUTOINCREMENT, sender_mb TEXT NOT NULL, seq_hint INTEGER NOT NULL, bytes INTEGER NOT NULL, created_at INTEGER NOT NULL, expires_at INTEGER NOT NULL);
    CREATE INDEX IF NOT EXISTS relay_v2_mail_expiry ON relay_v2_mail(expires_at);
    CREATE TABLE IF NOT EXISTS relay_v2_chunks (mail_id INTEGER NOT NULL, part INTEGER NOT NULL, ciphertext TEXT NOT NULL, PRIMARY KEY(mail_id,part));
    -- 数据面（v3 blob）。身份就是 (blob_id, idx)：重复 PUT 同一片在物理上不占新空间，
    -- 这是「4.4× 重复副本」结构性消失的地方。列名叫 idx 而不是 index（SQL 关键字）。
    -- 只记「谁开的、什么时候过期」。片数/总字节/整流摘要这里**故意不存**：
    -- 流式上传时总量只有发完才知道，声明会逼客户端先整份读一遍；总量与摘要由
    -- 端到端签名的 manifest 携带，relay 存一份只是多一个可见字段。
    CREATE TABLE IF NOT EXISTS relay_v3_blob (blob_id TEXT PRIMARY KEY, sender_mb TEXT NOT NULL, created_at INTEGER NOT NULL, expires_at INTEGER NOT NULL);
    CREATE TABLE IF NOT EXISTS relay_v3_blob_chunk (blob_id TEXT NOT NULL, idx INTEGER NOT NULL, ciphertext BLOB NOT NULL, PRIMARY KEY(blob_id,idx));
  `);
  // 投递幂等键：同一 sender 的同一 seq_hint 只保留一封。seq_hint 是客户端分配的
  // 稳定投递序号（明文且在 sender 签名里），所以可以安全地当幂等键用；seq_hint=0
  // 表示「本轮没有稳定序号」（配对帧等），它必须能重复出现，所以用**部分索引**
  // 而不是普通 UNIQUE 约束。升级路径（去重 + 只认唯一约束冲突）见
  // `ensureIdempotencyIndex`，那段逻辑只写一遍。
  ensureIdempotencyIndex(db);
  return db;
}

/** 删掉已过期的行。返回被删的邮件条数。
 *
 * 不只有信：数据面的 blob 也走这条「无进展 TTL」（30 分钟，比信件 TTL 短得多），
 * 否则中止的传输会一直在退役信箱里占着空间 —— sweeper 扫目录正是靠这里回收。
 * 旧库在首次 `openMailboxDb` 时就建了 blob 两张表，所以这两个 DELETE 不会撞到不存在
 * 的表（sweeper 也走同一个 `openMailboxDb`）。
 */
export function purgeExpired(db, now = Date.now()) {
  db.prepare('DELETE FROM relay_v2_chunks WHERE mail_id IN (SELECT id FROM relay_v2_mail WHERE expires_at<=?)').run(now);
  const mail = db.prepare('DELETE FROM relay_v2_mail WHERE expires_at<=?').run(now).changes;
  db.prepare('DELETE FROM relay_v2_nonces WHERE expires_at<=?').run(now);
  db.prepare('DELETE FROM relay_v3_blob_chunk WHERE blob_id IN (SELECT blob_id FROM relay_v3_blob WHERE expires_at<=?)').run(now);
  db.prepare('DELETE FROM relay_v3_blob WHERE expires_at<=?').run(now);
  return Number(mail);
}
