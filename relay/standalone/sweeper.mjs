// 信箱库的磁盘回收器。
//
// 为什么需要它：relay 原先唯一的回收机制是「删行」，而删行**不缩文件**。事故现场
// 有信箱 0 封邮件仍占 67 MB、两个信箱各自 690 MB 顶满配额 —— 全是删/重插留下的
// freelist。更要命的是原来的周期清理只遍历**当时已打开的句柄**，设备退役后再无
// 请求的信箱永远不会被任何路径碰到。
//
// 所以这里按**目录**扫（不看句柄），跑在 `worker_threads` 里：`node:sqlite` 是同步
// API，放在主线程会阻塞事件循环。
import { readdirSync } from 'node:fs';
import { join } from 'node:path';
import { isMainThread, parentPort, workerData } from 'node:worker_threads';
import { openMailboxDb, purgeExpired } from './schema.mjs';

/** freelist 占比超过这个值才值得付一次全量重写（VACUUM）。 */
export const VACUUM_RATIO = 0.2;

/**
 * 回收一个信箱库：删过期行 → WAL 截断 → freelist 占比过高时 VACUUM。
 *
 * 返回 `{ mail, before, after, vacuumed }`（字节）。server 可能正持有同一个库的
 * 句柄，`VACUUM` 要独占访问；拿不到锁就跳过本轮（`busy_timeout` 已给 5 s），
 * 退役信箱本来就没有并发请求，下一轮照样能收。
 */
export function sweepMailbox(filePath) {
  const db = openMailboxDb(filePath);
  try {
    const mail = purgeExpired(db);
    db.exec('PRAGMA wal_checkpoint(TRUNCATE)');
    const page = Number(db.prepare('PRAGMA page_count').get().page_count);
    const free = Number(db.prepare('PRAGMA freelist_count').get().freelist_count);
    const before = page * pageSize(db);
    let vacuumed = false;
    if (page > 0 && free / page > VACUUM_RATIO) {
      db.exec('VACUUM');
      vacuumed = true;
    } else if (free > 0) {
      // 增量模式下顺手收掉零碎页，不值得整库重写。
      db.exec('PRAGMA incremental_vacuum');
    }
    db.exec('PRAGMA wal_checkpoint(TRUNCATE)');
    const after = Number(db.prepare('PRAGMA page_count').get().page_count) * pageSize(db);
    return { mail, before, after, vacuumed };
  } finally {
    db.close();
  }
}

function pageSize(db) {
  return Number(db.prepare('PRAGMA page_size').get().page_size);
}

/** 扫一遍整个 mailboxes 目录。单个信箱失败不影响其它信箱。 */
export function sweepAll(dataDir) {
  const dir = join(dataDir, 'mailboxes');
  let names = [];
  try {
    names = readdirSync(dir);
  } catch {
    return { swept: 0, failed: 0, freed: 0 }; // 目录还不存在：没有任何信箱
  }
  let swept = 0, failed = 0, freed = 0;
  for (const name of names) {
    if (!name.endsWith('.db')) continue;
    try {
      const result = sweepMailbox(join(dir, name));
      swept += 1;
      freed += Math.max(0, result.before - result.after);
    } catch (error) {
      // 正在被请求路径写入（锁争用）或文件已消失：跳过，下一轮再来。
      failed += 1;
      if (process.env.ELSEWHEN_DEBUG) console.error(`sweep skipped ${name}: ${error.message}`);
    }
  }
  return { swept, failed, freed };
}

if (!isMainThread && workerData?.sweep) {
  const { dataDir, intervalMs } = workerData;
  const tick = () => {
    const result = sweepAll(dataDir);
    parentPort.postMessage(result);
    // 回收是尽力而为：出错不该让 worker 退出（退出就等于永久失去回收能力）。
    setTimeout(tick, intervalMs).unref?.();
  };
  tick();
}
