# stamp-legacy-db.ps1 - 把旧迁移链（v1–v31）建出来的数据库标记为「已应用 v1 基线」。
#
# 与 stamp-legacy-db.sh 等价，逻辑（结构指纹比对、拒绝盲写、写锁内复查）
# 一字未改，只有路径解析和 diff 换成了 Windows 形式。
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
# 依赖：sqlite3（带 CLI）、PowerShell 5.1+、git（仅用于产出统一 diff 格式）
#
# 用法：
#   powershell -File scripts\stamp-legacy-db.ps1 [数据库路径]
#   默认 %LOCALAPPDATA%\elsewhen\elsewhen.db
param(
    [string]$Db
)

$ErrorActionPreference = "Stop"

$root = Split-Path -Parent (Split-Path -Parent $MyInvocation.MyCommand.Path)
$baseline = Join-Path $root "migrations\01-baseline\up.sql"

if (-not $Db) {
    $dataDir = if ($env:ELSEWHEN_DATA_DIR) { $env:ELSEWHEN_DATA_DIR } else { Join-Path $env:LOCALAPPDATA "elsewhen" }
    $Db = Join-Path $dataDir "elsewhen.db"
}

function Die($Message) { Write-Error "错误：$Message"; exit 1 }

$sqlite = (Get-Command sqlite3 -ErrorAction SilentlyContinue).Source
if (-not $sqlite) { Die "需要 sqlite3 命令行工具" }
if (-not (Test-Path $baseline)) { Die "找不到基线文件 $baseline" }
if (-not (Test-Path $Db)) { Die "找不到数据库 $Db" }

$tmp = Join-Path ([System.IO.Path]::GetTempPath()) ("elsewhen-stamp-" + [guid]::NewGuid().ToString("N"))
New-Item -ItemType Directory -Force -Path $tmp | Out-Null

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
function Get-Fingerprint([string]$Path) {
    $tables = & $sqlite -noheader $Path "SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name;"
    foreach ($t in $tables) {
        "TABLE $t"
        # cut -d'|' -f2- ：丢掉 cid 列；排序让列顺序无关。
        (& $sqlite -noheader -separator '|' $Path "PRAGMA table_info('$t');") |
            ForEach-Object { ($_ -split '\|', 2)[1] } |
            Sort-Object -CaseSensitive
    }
    # 索引与触发器：名字 + 去掉全部空白的定义。索引/触发器定义里没有带空格的
    # 字符串字面量，去空白只影响排版，不会掩盖真实差异。
    & $sqlite -noheader $Path @"
SELECT 'INDEX ' || name || ' ' || replace(replace(COALESCE(sql,''), char(10), ''), ' ', '')
FROM sqlite_master WHERE type='index' AND name NOT LIKE 'sqlite_%' ORDER BY name;
"@
    & $sqlite -noheader $Path @"
SELECT 'TRIGGER ' || name || ' ' || replace(replace(COALESCE(sql,''), char(10), ''), ' ', '')
FROM sqlite_master WHERE type='trigger' ORDER BY name;
"@
}

try {
    $uv = [int](& $sqlite $Db "PRAGMA main.user_version;")

    if ($uv -ge 1) {
        Write-Host "user_version = $uv，已经不需要处理，退出。"
        Write-Host "（若 app 仍起不来，那不是这个原因，请另查。）"
        exit 0
    }
    if ($uv -ne 0) {
        Die "user_version = $uv，不是预期的 0，脚本只处理旧链遗留的 0 值"
    }

    # 旧链的标记：有 schema_migrations 表且有版本记录。没有它就说明这不是旧链的库，
    # 钉版本号会是在给一个来历不明的库伪造元数据。
    # 注意分两步：version 列在 schema_migrations 这张表里，不在 sqlite_master 上。
    $hasTable = & $sqlite $Db "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='schema_migrations';"
    if ($hasTable -eq "0") {
        Die "该库没有 schema_migrations 表，无法确认它来自旧迁移链，拒绝写版本号"
    }
    $legacyMax = [int](& $sqlite $Db "SELECT COALESCE(MAX(version), 0) FROM schema_migrations;")
    if ($legacyMax -le 0) {
        Die "schema_migrations 表是空的，无法确认它来自旧迁移链，拒绝写版本号"
    }

    # 表名里带单引号会让 PRAGMA table_info('...') 的字面量失配，直接拒绝而不是猜。
    if ((Get-Fingerprint $Db) -match "^TABLE '") {
        Die "存在名字含单引号的表，指纹比对不可靠，请人工核对"
    }

    Write-Host "库：       $Db"
    Write-Host "基线：     migrations\01-baseline\up.sql"
    Write-Host "旧链版本：schema_migrations 最高 $legacyMax（user_version 仍为 0）"
    Write-Host ""

    $refDb = Join-Path $tmp "ref.db"
    Get-Content $baseline -Raw | & $sqlite $refDb

    $liveFp = Join-Path $tmp "live.fp"
    $refFp = Join-Path $tmp "ref.fp"
    Get-Fingerprint $Db | Set-Content -Path $liveFp -Encoding UTF8
    Get-Fingerprint $refDb | Set-Content -Path $refFp -Encoding UTF8

    $diff = Join-Path $tmp "struct.diff"
    & git diff --no-index --no-color -U3 -- $refFp $liveFp 2>&1 | Set-Content -Path $diff -Encoding UTF8
    # git diff 退出码 1 = 有差异（这是预期路径），>1 = 真的出错。
    if ($LASTEXITCODE -gt 1) { Die "git diff 执行失败，无法比对结构" }
    if ((Get-Item $diff).Length -gt 0) {
        Write-Host "结构与 v1 基线不等，拒绝写版本号。差异（- 基线 / + 该库）："
        Get-Content $diff -TotalCount 40 | ForEach-Object { Write-Host $_ }
        Write-Host ""
        Write-Host "这种情况说明它其实不是 v1 基线形态，应该写一条真正的迁移把它升级上来，"
        Write-Host "而不是把版本号钉到 1。请把上面的差异发出来再定处置方式。"
        exit 1
    }

    Write-Host "结构比对通过：表集合、每张表的列定义、索引、触发器均与基线一致。"

    $stamp = Get-Date -Format "yyyyMMdd-HHmmss"
    $backup = "$Db.pre-stamp-$stamp"
    Copy-Item $Db $backup
    Write-Host "已备份：   $backup"

    # 在写锁内复查一遍 user_version 再写，避免比对期间被别的进程改过。
    # BEGIN IMMEDIATE 拿的是写锁：app 若正开着这个库，这一步会因 busy 而失败。
    #
    # 条件写入用一条 CHECK 约束把「版本号已经不是 0」变成失败的 INSERT，配合 -bail
    # 让 sqlite3 非零退出、事务随连接关闭回滚。
    # 这里不能用 RAISE(ABORT, ...)：它只在校验约束的触发器里合法。
    & $sqlite -bail $Db @"
BEGIN IMMEDIATE;
    CREATE TEMP TABLE stamp_guard(x INTEGER CHECK(x = 1));
    INSERT INTO stamp_guard SELECT ((SELECT * FROM pragma_user_version) = 0);
    PRAGMA main.user_version = 1;
COMMIT;
"@
    if ($LASTEXITCODE -ne 0) {
        Die "写入失败：user_version 可能已被改动，或数据库正被占用。未做任何修改。"
    }

    $after = & $sqlite $Db "PRAGMA main.user_version;"
    Write-Host "user_version: 0 → $after"
    Write-Host ""
    Write-Host "只钉了版本号，没有动任何数据。v2（goals 表）由 app 下次启动时正常迁移上去。"
    Write-Host "现在可以启动 app 了。"
} finally {
    Remove-Item $tmp -Recurse -Force -ErrorAction SilentlyContinue
}
