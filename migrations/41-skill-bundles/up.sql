-- 用户技能跨设备分发（roadmap P3）：把技能文件字节随整库同步走，
-- 无需为文件新增同步协议。skills 目录是设备的本地视图（用户主权），
-- 本表是分发事实源；启停状态（skills_state.json）按设备独立，不同步。
--
-- 写入方：安装成功（install_staged）与启动 reconcile（目录领先于表时回写）。
-- 读取方：启动 reconcile（表领先于目录时落盘；目录消失时记删除墓碑）。

CREATE TABLE skill_files (
  skill_id TEXT NOT NULL,
  rel      TEXT NOT NULL,        -- 技能目录内相对路径，禁 '..' 与绝对路径
  bytes    BLOB  NOT NULL,
  sha256   TEXT NOT NULL,
  size     INTEGER NOT NULL,
  updated_at TEXT NOT NULL,       -- RFC3339，最后写入设备的时间
  PRIMARY KEY (skill_id, rel)
);

CREATE TABLE skill_tombstones (
  skill_id   TEXT PRIMARY KEY,
  updated_at TEXT NOT NULL
);

CREATE INDEX idx_skill_tombstones_updated ON skill_tombstones(updated_at);
