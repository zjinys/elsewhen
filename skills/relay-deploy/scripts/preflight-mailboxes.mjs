// 线上信箱预检：用**仓库当前的 schema.mjs** 去开一遍线上真实信箱的副本。
//
// 为什么需要它：`openMailboxDb()` 在 open 时会跑建表 + `ensureIdempotencyIndex()`
// 迁移，而这一步失败会把 relay 打死在启动阶段 —— `/health` 全绿之后再发现就晚了。
// 本次（2026-10-25）就发生过：线上镜像是 P4 之前的代码，仓库里的 schema.mjs
// 已经带上了「去重建幂等索引」的新逻辑，部署前必须先证明它在真实数据上不炸。
//
// 用法（在仓库根执行；参数是**信箱目录的副本**，不要指向线上卷）：
//
//   node skills/relay-deploy/scripts/preflight-mailboxes.mjs /tmp/mailbox-copy
//
// 只读保证：脚本自己把每个 `*.db`（连同 -wal/-shm）再复制到临时目录，然后只在
// 临时副本上开库，源目录一个字节都不改。所以即使传进来的是线上目录也不会写坏，
// 但仍然建议传副本 —— 线上开 WAL 会在源目录里落下 -shm 文件。
import { DatabaseSync } from 'node:sqlite';
import { cpSync, mkdtempSync, readdirSync, rmSync, statSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { openMailboxDb } from '../../../relay/standalone/schema.mjs';

const source = process.argv[2];
if (!source) {
  console.error('用法：node preflight-mailboxes.mjs <信箱目录>');
  process.exit(2);
}
if (!statSync(source).isDirectory()) {
  console.error(`不是目录：${source}`);
  process.exit(2);
}

const COUNT = {
  mail: 'SELECT COUNT(*) AS n FROM relay_v2_mail',
  senders: 'SELECT COUNT(*) AS n FROM relay_v2_senders',
  owner: 'SELECT COUNT(*) AS n FROM relay_v2_owner',
  blobs: 'SELECT COUNT(*) AS n FROM relay_v3_blob',
  blobChunks: 'SELECT COUNT(*) AS n FROM relay_v3_blob_chunk',
};

// 幂等索引只能靠 sqlite_master 判断：`CREATE UNIQUE INDEX IF NOT EXISTS` 成功
// 不代表索引存在（它只在「已存在同名索引」时静默返回），所以直接查目录。
const IDEM_INDEX = "SELECT COUNT(*) AS n FROM sqlite_master WHERE type='index' AND name='relay_v2_mail_idem'";

const files = readdirSync(source).filter((name) => name.endsWith('.db')).sort();
if (files.length === 0) {
  console.error(`${source} 里没有 *.db —— 数据卷挂错了吗？`);
  process.exit(2);
}

let failed = 0;
for (const name of files) {
  const scratch = mkdtempSync(join(tmpdir(), 'relay-preflight-'));
  try {
    // 整个库搬过来（含 -wal/-shm）：WAL 里可能还有没 checkpoint 的已提交事务，
    // 只拷主文件会让计数偏少 —— 那正是最危险的一种「看起来没事」。
    for (const suffix of ['', '-wal', '-shm']) {
      const from = join(source, `${name}${suffix}`);
      try {
        cpSync(from, join(scratch, `${name}${suffix}`));
      } catch (error) {
        if (suffix === '') throw error;
      }
    }
    const copy = join(scratch, name);
    const db = openMailboxDb(copy);
    try {
      const one = (sql) => db.prepare(sql).get().n;
      const integrity = db.prepare('PRAGMA quick_check').get();
      const report = {
        mailbox: name.slice(0, 8),
        bytes: statSync(copy).size,
        idemIndex: one(IDEM_INDEX),
        integrity: Object.values(integrity)[0],
      };
      for (const [key, sql] of Object.entries(COUNT)) report[key] = one(sql);
      const ok = report.integrity === 'ok' && report.idemIndex === 1;
      if (!ok) failed += 1;
      console.log(`${ok ? '✓' : '✗'} ${JSON.stringify(report)}`);
    } finally {
      db.close();
    }
  } catch (error) {
    failed += 1;
    console.log(`✗ ${name.slice(0, 8)} 开库失败：${error?.message ?? error}`);
  } finally {
    rmSync(scratch, { recursive: true, force: true });
  }
}

// 非 0 退出让部署脚本 `set -e` 直接停在这里：预检不过就不该动线上容器。
process.exit(failed === 0 ? 0 : 1);
