-- v70：命名密钥库（named secrets）——一等凭据管理 P1。
--
-- 「密钥」是独立的一等概念：集中定义（name 唯一），各消费方（MCP env /
-- headers、web_search 等）以 {{secret:<name>}} 占位符引用，运行时最后一刻
-- 解密插值。表本体**不入同步快照**（SNAPSHOT_EXCLUDED_TABLES，
-- 「凭据不离本机」红线，见 records.rs 同名单）。
--
-- 密封策略与 provider_key_secrets 同基建（ring AEAD CHACHA20-POLY1305）：
-- 同步密钥已就绪时写入即密封（ciphertext 有值、plaintext 置 NULL）；
-- master 未解锁 / 未配置同步时退回明文列（与 provider key 的未密封安装
-- 一致），enable_provider_key_sealing 的同款密封升级路径负责补封。
-- AAD 绑定 (dataset_id, installation, id, name)，换库/换名密文自动失效。

CREATE TABLE named_secrets (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    title TEXT NOT NULL DEFAULT '',
    ciphertext BLOB,
    plaintext TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
