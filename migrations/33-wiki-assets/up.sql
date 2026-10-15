-- 图片资产的同步载体（快照 v3 携带）。
-- rel 是相对数据目录的确定性路径（tweet_images/<tweet_id>/img_N.ext）：
-- 正文 content_md 只存占位符 {{tweet-image:REL}}，本机绝对路径任何情况都
-- 不入库；bytes 让接收端（含快照断档恢复的全新设备）渲染时自愈落盘。
CREATE TABLE IF NOT EXISTS wiki_assets (
    rel TEXT PRIMARY KEY,
    sha256 TEXT NOT NULL,
    mime TEXT NOT NULL,
    bytes BLOB NOT NULL,
    size INTEGER NOT NULL,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);
