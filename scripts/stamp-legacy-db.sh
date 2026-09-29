#!/usr/bin/env bash
# 把旧迁移链（v1–v31）建出来的数据库标记为「已应用 v1 基线」，修掉启动失败。
#
# 为什么需要这个脚本：
#   旧链把版本记在 schema_migrations 表里（31 行），文件头的 user_version 一直是 0。
#   新链用 rusqlite_migration，只认文件头 user_version —— 读到 0 就判定「空库」，
#   于是去执行 migrations/01-baseline/up.sql 里的裸 CREATE TABLE，对着已有的 27 张
#   表撞出 table already exists，app 起不来。
#
# 钉 user_version = 1 等于断言「这个库的结构已经是 v1 基线」。所以本脚本不盲目钉：
# 它先拿基线文件建一个参照库，逐项比对表集合、每张表的列定义、索引、触发器，
# 全等才写；不等就打印差异并拒绝执行 —— 那种情况下该写一条真正的迁移，
# 而不是靠改元数据蒙混过关。比对刻意忽略列的物理顺序（见下）。
#
# 为什么不把这段逻辑放进 app 启动路径：
#   1) 那是 rusqlite_migration 官方明确划出的禁区。README 的 Limits 第 1 条：
#      user_version 被本程序或任何其他库改动过，行为 unspecified。
#   2) 在 Rust 侧自建「有表但 user_version=0 就补版本」的判断，等于绕过
#      rusqlite_migration 自己管版本，本仓已经走过并记录过这个弯路。
#   3) 自动改写版本元数据会把「谎报」变成「自动生效的谎报」，静默且不可审计。
#   这是每个装了旧版、又升级到新版的人都要手动做一次的一次性修复，所以是脚本。
#
# 幂等：user_version >= 1 时直接退出并说明，不做任何写入。可重复执行。
#
# 依赖：sqlite3（带 CLI），bash
#
# 用法：
#   scripts/stamp-legacy-db.sh [数据库路径]
#   默认 ~/.local/share/elsewhen/elsewhen.db

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BASELINE="$REPO_ROOT/migrations/01-baseline/up.sql"
DB="${1:-$HOME/.local/share/elsewhen/elsewhen.db}"

die() { echo "错误：$*" >&2; exit 1; }

command -v sqlite3 >/dev/null || die "需要 sqlite3 命令行工具"
[ -f "$BASELINE" ] || die "找不到基线文件 $BASELINE"
[ -f "$DB" ] || die "找不到数据库 $DB"

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

# 生成一个库的结构指纹，一行一条规范化的对象定义。
#
# 逐表用**字面量**表名调 PRAGMA table_info，而不是用 pragma_table_info(...) 这个
# 表值函数做跨 schema 比对：那个函数不带 schema 限定，拿 ref.sqlite_master 的行去
# 喂它时，它仍然去读 main 里的同名表，于是两侧列集合永远相同、差集恒为空——
# 给 todos 多加一列都测不出来（实测漏检过）。字面量没有这个歧义。
#
# 列定义去掉 cid（第 1 段）后按行排序，所以**忽略列的物理顺序**：实测活库与基线在
# ai_provider_configs / conversations / messages / relations / wiki_pages 五张表上
# 顺序不同，那是旧链 ALTER TABLE ADD COLUMN 追加列的痕迹。但列名、类型、NOT NULL、
# 默认值、主键逐项相同，而本仓所有查询都显式写列名、从不 SELECT *，顺序不影响行为。
# 不能直接比 sqlite_master.sql 原文：空格拉伸与列追加位置都会造成假差异。
fingerprint() {
    local db="$1" t
    while IFS= read -r t; do
        printf 'TABLE %s\n' "$t"
        sqlite3 -noheader -separator '|' "$db" "PRAGMA table_info('$t');" \
            | cut -d'|' -f2- | LC_ALL=C sort
    done < <(sqlite3 -noheader "$db" "
        SELECT name FROM sqlite_master
        WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name;")
    # 索引与触发器：名字 + 去掉全部空白的定义。索引/触发器定义里没有带空格的
    # 字符串字面量，去空白只影响排版，不会掩盖真实差异。
    sqlite3 -noheader "$db" "
        SELECT 'INDEX ' || name || ' ' || replace(replace(COALESCE(sql,''), char(10), ''), ' ', '')
        FROM sqlite_master WHERE type='index' AND name NOT LIKE 'sqlite_%' ORDER BY name;"
    sqlite3 -noheader "$db" "
        SELECT 'TRIGGER ' || name || ' ' || replace(replace(COALESCE(sql,''), char(10), ''), ' ', '')
        FROM sqlite_master WHERE type='trigger' ORDER BY name;"
}

UV="$(sqlite3 "$DB" 'PRAGMA main.user_version;')"

if [ "$UV" -ge 1 ]; then
    echo "user_version = $UV，已经不需要处理，退出。"
    echo "（若 app 仍起不来，那不是这个原因，请另查。）"
    exit 0
fi

if [ "$UV" != "0" ]; then
    die "user_version = $UV，不是预期的 0，脚本只处理旧链遗留的 0 值"
fi

# 旧链的标记：有 schema_migrations 表且有版本记录。没有它就说明这不是旧链的库，
# 钉版本号会是在给一个来历不明的库伪造元数据。
# 注意分两步：version 列在 schema_migrations 这张表里，不在 sqlite_master 上。
#
# 新库/已修复的库里这张表已被迁移 03-drop-legacy-migrations 删掉了，但那种库的
# user_version >= 1，上面一步就已经退出了，走不到这个检查。所以这条检查只可能
# 对真正的旧链库生效。
HAS_TABLE="$(sqlite3 "$DB" "
    SELECT COUNT(*) FROM sqlite_master
    WHERE type='table' AND name='schema_migrations';")"
[ "$HAS_TABLE" != "0" ] || die \
    "该库没有 schema_migrations 表，无法确认它来自旧迁移链，拒绝写版本号"

LEGACY_MAX="$(sqlite3 "$DB" 'SELECT COALESCE(MAX(version), 0) FROM schema_migrations;')"
[ "$LEGACY_MAX" -gt 0 ] || die \
    "schema_migrations 表是空的，无法确认它来自旧迁移链，拒绝写版本号"

# 表名里带单引号会让 PRAGMA table_info('...') 的字面量失配，直接拒绝而不是猜。
if fingerprint "$DB" | grep -q "^TABLE '"; then
    die "存在名字含单引号的表，指纹比对不可靠，请人工核对"
fi

echo "库：       $DB"
echo "基线：     migrations/01-baseline/up.sql"
echo "旧链版本：schema_migrations 最高 $LEGACY_MAX（user_version 仍为 0）"
echo

sqlite3 "$TMP/ref.db" < "$BASELINE"
fingerprint "$DB" > "$TMP/live.fp"
fingerprint "$TMP/ref.db" > "$TMP/ref.fp"

if ! diff -u "$TMP/ref.fp" "$TMP/live.fp" > "$TMP/struct.diff"; then
    echo "结构与 v1 基线不等，拒绝写版本号。差异（- 基线 / + 该库）："
    head -40 "$TMP/struct.diff"
    echo
    echo "这种情况说明它其实不是 v1 基线形态，应该写一条真正的迁移把它升级上来，"
    echo "而不是把版本号钉到 1。请把上面的差异发出来再定处置方式。"
    exit 1
fi

echo "结构比对通过：表集合、每张表的列定义、索引、触发器均与基线一致。"

STAMP="$(date +%Y%m%d-%H%M%S)"
BACKUP="${DB}.pre-stamp-${STAMP}"
cp -p "$DB" "$BACKUP" || die "备份失败：$BACKUP"
echo "已备份：   $BACKUP"

# 在写锁内复查一遍 user_version 再写，避免比对期间被别的进程改过。
# BEGIN IMMEDIATE 拿的是写锁：app 若正开着这个库，这一步会因 busy 而失败。
#
# 条件写入用一条 CHECK 约束把「版本号已经不是 0」变成失败的 INSERT，配合 -bail
# 让 sqlite3 非零退出、事务随连接关闭回滚。
# 这里不能用 RAISE(ABORT, ...)：它只在校验约束的触发器里合法。
if ! sqlite3 -bail "$DB" "
BEGIN IMMEDIATE;
    CREATE TEMP TABLE stamp_guard(x INTEGER CHECK(x = 1));
    INSERT INTO stamp_guard SELECT ((SELECT * FROM pragma_user_version) = 0);
    PRAGMA main.user_version = 1;
COMMIT;"; then
    die "写入失败：user_version 可能已被改动，或数据库正被占用。未做任何修改。"
fi

AFTER="$(sqlite3 "$DB" 'PRAGMA main.user_version;')"
echo "user_version: 0 → $AFTER"
echo
echo "只钉了版本号，没有动任何数据。v2（goals 表）由 app 下次启动时正常迁移上去。"
echo "现在可以启动 app 了。"
