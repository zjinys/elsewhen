//! 数据库 schema 的引导与版本化迁移。
//!
//! 迁移正文按 `rusqlite_migration` 官方约定放在仓库根的 [`migrations/`](../../migrations)：
//! 每个版本一个子目录 `{序号}-{名字}/up.sql`。序号是**从 1 开始的连续整数**
//! （crate 内部用 `id - 1` 去索引数组，跳号或重号会直接报错），执行顺序由序号
//! 决定而非文件名排序，所以不要求补零对齐。需要回滚时同目录下再加 `down.sql`，
//! 本仓库不需要。
//!
//! 整个目录用 `include_dir!` 在编译期嵌进二进制，再交给
//! `Migrations::from_directory` 扫描——运行时不依赖磁盘上的 `migrations/`，
//! 六种安装包不用多带一份资源文件。迁移正文也不放 Rust 源码里：`goals` 那条
//! 一度写成内联 `const GOALS_V2: &str`，于是 SQL、解释它为什么这么写的注释、
//! 以及触发器里那个上限字面量被拆在三个地方，改一处要同时看两个文件。
//!
//! **`01-baseline/up.sql` 是冻结的基线**，只作为版本 1 应用一次，之后不得再改：
//! `to_latest` 对任何 `user_version >= 1` 的库都会跳过它，所以改它对已有库
//! 完全无效——新库会带上改动，老库静默缺表缺列且不报任何错。
//! 后续一切 schema 变更都是新建 `migrations/{下一个序号}-{名字}/up.sql`。
//!
//! 基线里有一行 `CREATE TABLE schema_migrations`，那是旧迁移链的版本记录，现在
//! 已无人读写，由 `03-drop-legacy-migrations` 删掉。留着不动是因为基线冻结；
//! 这也正是「只追加、不改历史」的用处——新库走完 v1 建、v3 删，与老库终态一致。
//!
//! 版本的唯一真源是 `PRAGMA user_version`，也就是文件头第 60 字节起的 4 个字节。
//! 库里没有第二张表在记版本。

use anyhow::Result;
use include_dir::{Dir, include_dir};
use rusqlite::Connection;
use rusqlite_migration::{Migrations, SchemaVersion};

/// `migrations/` 整个目录，编译期嵌入二进制。
///
/// 用 `$CARGO_MANIFEST_DIR` 而非 `../../`：`include_dir!` 里相对路径的基准是
/// **调用处所在文件**，而 `env!` 的基准是 crate 根，不会因源文件挪位置而失效。
static MIGRATION_DIR: Dir<'static> = include_dir!("$CARGO_MANIFEST_DIR/migrations");

/// 触发器拒绝写入时使用的标识，存储层据此把错误翻成人话。
pub(crate) const ACTIVE_GOAL_LIMIT_REACHED: &str = "active_goal_limit_reached";

/// 按官方约定从 `migrations/` 组装迁移集。
///
/// 返回 `Result` 而不是常量：`from_directory` 会校验目录结构（缺 `up.sql`、
/// 序号跳号或重号都在这里报错）。这是 crate 给的结构性防线，绕开它自己维护
/// 一张登记表，就等于把「新增文件忘了登记 → 版本静默不执行」请回来。
fn migrations() -> Result<Migrations<'static>> {
    Ok(Migrations::from_directory(&MIGRATION_DIR)?)
}

pub(crate) fn initialize(connection: &mut Connection) -> Result<()> {
    migrations()?.to_latest(connection)?;
    Ok(())
}

/// 代码期望的最新 schema 版本（迁移目录的最大序号）。
/// 后台路径用它判断库是否待升级：版本落后就跳过本轮工作，
/// 绝不在后台补跑迁移——启动写入只属于前台初始化路径。
pub(crate) fn latest_version() -> Result<i64> {
    let mut connection = Connection::open_in_memory()?;
    debug_assert!(matches!(
        migrations()?.current_version(&connection)?,
        SchemaVersion::NoneSet
    ));
    initialize(&mut connection)?;
    let version: i64 = connection.query_row("PRAGMA main.user_version", [], |row| row.get(0))?;
    Ok(version)
}

#[cfg(test)]
pub(crate) fn initialize_test_version(connection: &mut Connection, version: usize) -> Result<()> {
    migrations()?.to_version(connection, version)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn memory_db() -> Connection {
        let mut connection = Connection::open_in_memory().unwrap();
        initialize(&mut connection).unwrap();
        connection
    }

    fn insert(connection: &Connection, id: &str, phase: &str, status: &str) {
        connection
            .execute(
                "INSERT INTO goals(id, content, phase, status, created_at, updated_at, superseded_at)
                 VALUES (?1, 'x', ?2, ?3, 't', 't', ?4)",
                rusqlite::params![id, phase, status, if status == "active" { None } else { Some("t0") }],
            )
            .unwrap();
    }

    /// 官方内置自检：把全部 up 迁移在临时内存库上从头跑到尾。
    ///
    /// 这是 crate 推荐的迁移测试入口，覆盖「SQL 本身跑不跑得通」；下面那些测试
    /// 覆盖的是目标表的语义，两者互补。
    #[test]
    fn migrations_validate() {
        migrations().unwrap().validate().unwrap();
    }

    /// 目录发现的结构性保证：八个迁移、按序号取名。
    ///
    /// 「新增文件却忘了登记」这类错误在这里不可能发生——`from_directory` 直接
    /// 扫目录，不经过任何手工登记表。这条测试盯的是别一种漂移：有人把子目录
    /// 改名成不满足 `{序号}-{名字}` 的形式，或把序号写成不连续。
    #[test]
    fn migrations_are_discovered_from_the_directory() {
        let mut names: Vec<String> = MIGRATION_DIR
            .dirs()
            .map(|d| d.path().file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        names.sort();
        assert_eq!(
            names,
            [
                "01-baseline",
                "02-goals",
                "03-drop-legacy-migrations",
                "04-knowledge",
                "05-source-compilation",
                "06-wiki-maintenance",
                "07-knowledge-maintenance",
                "08-knowledge-repair",
                "09-wiki-workflows",
                "10-chat-approvals",
                "11-chat-generation",
                "12-chat-input-queue",
                "13-chat-summary",
                "14-chat-page-tasks",
                "15-chat-supplements",
                "16-sync-core",
                "17-sync-snapshot-delivery",
                "18-sync-operation-fragments",
                "19-sync-local-vault",
                "20-sync-processor-guard",
                "21-chat-response-input",
                "22-sync-membership-authority",
                "23-provider-key-sealing",
                "24-provider-edit-basis",
                "25-web-search",
                "26-doh-mode",
                "27-sync-source-isolation",
                "28-sync-provider-conflicts",
                "29-sync-gc",
                "30-sync-resnapshot",
                "31-sync-processor-handoff",
                "32-sync-wiki-slug-seen",
                "33-wiki-assets",
                "34-wiki-assets-ledger",
                "35-rule-triggers",
                "36-rule-hit-feedback",
                "37-companion-revisit",
            ],
            "迁移子目录应形如 {{序号}}-{{名字}}，且序号从 1 起连续"
        );
    }

    /// 冻结约定与内容归属：基线里不该出现 `goals`。
    ///
    /// `goals` 若混进冻结的基线，新库正常而老库（`user_version >= 1`，基线被
    /// 跳过）静默缺表——这是本仓库最阴的失败模式，没有报错。
    #[test]
    fn baseline_stays_frozen_and_goals_live_in_the_second_migration() {
        let baseline = include_str!("../../migrations/01-baseline/up.sql");
        assert!(
            baseline.contains("CREATE TABLE events"),
            "01-baseline 应是当前 schema 基线"
        );
        assert!(
            !baseline.contains("CREATE TABLE goals"),
            "goals 属于 02-goals，混进冻结的基线会让老库静默少一张表"
        );
    }

    /// 全部迁移跑完后，版本号就是 `PRAGMA user_version`。
    ///
    /// 这条断言同时是「版本真源只有文件头」的证据：库里没有第二张表在记版本。
    #[test]
    fn migration_applies_goals_table() {
        let connection = memory_db();
        let version: i64 = connection
            .query_row("PRAGMA main.user_version", [], |row| row.get(0))
            .unwrap();
        assert_eq!(version, 37, "三十七条迁移跑完，user_version 应为 37");
        assert_eq!(
            latest_version().unwrap(),
            version,
            "latest_version 必须与迁移目录推导出的 user_version 一致"
        );
    }

    #[test]
    fn v10_history_survives_generation_schema_without_synthetic_runs() {
        let mut connection = Connection::open_in_memory().unwrap();
        migrations()
            .unwrap()
            .to_version(&mut connection, 10)
            .unwrap();
        connection.execute_batch("INSERT INTO conversations(id,title,created_at,updated_at) VALUES('legacy','legacy','t','t');
            WITH RECURSIVE seq(n) AS (VALUES(1) UNION ALL SELECT n+1 FROM seq WHERE n<1000)
            INSERT INTO messages(id,conversation_id,role,content,created_at)
            SELECT 'message-'||n,'legacy','user','original-'||n,'t' FROM seq;").unwrap();
        let before: String = connection.query_row("SELECT group_concat(id||':'||content,'|') FROM (SELECT id,content FROM messages ORDER BY id)", [], |r| r.get(0)).unwrap();
        initialize(&mut connection).unwrap();
        let after: String = connection.query_row("SELECT group_concat(id||':'||content,'|') FROM (SELECT id,content FROM messages ORDER BY id)", [], |r| r.get(0)).unwrap();
        assert_eq!(before, after);
        let runs: i64 = connection
            .query_row("SELECT COUNT(*) FROM chat_runs", [], |r| r.get(0))
            .unwrap();
        assert_eq!(runs, 0, "old messages never authorize automatic generation");
    }

    #[test]
    fn v27_rebuilds_sync_operations_preserving_rows_and_accepting_isolated() {
        let mut connection = Connection::open_in_memory().unwrap();
        migrations()
            .unwrap()
            .to_version(&mut connection, 26)
            .unwrap();
        // Pre-v27 data: one applied op in the old table (which must survive the
        // v27 DROP/RENAME rebuild). Populate the columns the old schema had.
        connection.execute_batch("INSERT INTO sync_replica(singleton,dataset_id,device_id,stream_id,next_sequence,logical_clock,processor_device_id) VALUES(1,'ds','dev-a','stream-a',1,5,'dev-a');
            INSERT INTO sync_members(device_id,stream_id,active) VALUES('dev-a','stream-a',1);
            INSERT INTO sync_frontiers(origin_stream_id,applied_sequence) VALUES('stream-a',1);
            INSERT INTO sync_operations(op_id,origin_device_id,origin_stream_id,origin_sequence,payload,payload_hash,local,state) VALUES('stream-a:1','dev-a','stream-a',1,'{}','hash',0,'applied');").unwrap();
        initialize(&mut connection).unwrap();
        // Row survived the rebuild.
        let kept: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM sync_operations WHERE op_id='stream-a:1' AND state='applied'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(
            kept, 1,
            "v27 rebuild must preserve existing sync_operations rows"
        );
        // The new CHECK accepts 'isolated' (and rejects anything outside the set).
        connection.execute("INSERT INTO sync_operations(op_id,origin_device_id,origin_stream_id,origin_sequence,payload,payload_hash,local,state,error_code) VALUES('stream-a:2','dev-a','stream-a',2,'{}','h2',0,'isolated','sync_operation_content_conflict')", []).unwrap();
        let iso: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM sync_operations WHERE state='isolated'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(iso, 1);
        let bad = connection.execute("INSERT INTO sync_operations(op_id,origin_device_id,origin_stream_id,origin_sequence,payload,payload_hash,local,state) VALUES('stream-a:3','dev-a','stream-a',3,'{}','h3',0,'bogus')", []);
        assert!(
            bad.is_err(),
            "state outside the CHECK set must still be rejected"
        );
        // The rebuilt unique identity constraint is intact.
        let dup = connection.execute("INSERT INTO sync_operations(op_id,origin_device_id,origin_stream_id,origin_sequence,payload,payload_hash,local,state) VALUES('other:1','dev-a','stream-a',1,'{}','h4',0,'applied')", []);
        assert!(
            dup.is_err(),
            "UNIQUE(origin_stream_id,origin_sequence) must survive the rebuild"
        );
    }

    #[test]
    fn v28_rebuilds_sync_provider_versions_preserving_rows_and_accepting_dismissed() {
        let mut connection = Connection::open_in_memory().unwrap();
        migrations()
            .unwrap()
            .to_version(&mut connection, 27)
            .unwrap();
        // Pre-v28 data: one applied and one conflicted revision, both of which
        // must survive the DROP/RENAME rebuild with their ciphertext intact.
        connection.execute_batch("INSERT INTO sync_provider_versions(revision_id,base_revision,sender_device_id,content_hash,encrypted_envelope,state) VALUES('rev-applied',NULL,'dev-a','hash-a',x'01','applied'),('rev-conflict','rev-applied','dev-b','hash-b',x'0203','conflict');").unwrap();
        initialize(&mut connection).unwrap();
        let kept: i64 = connection.query_row("SELECT COUNT(*) FROM sync_provider_versions WHERE (revision_id='rev-applied' AND state='applied' AND encrypted_envelope=x'01') OR (revision_id='rev-conflict' AND state='conflict' AND encrypted_envelope=x'0203')",[],|r|r.get(0)).unwrap();
        assert_eq!(
            kept, 2,
            "v28 rebuild must preserve existing sync_provider_versions rows"
        );
        // 'dismissed' is now accepted; states outside the set are still rejected.
        connection.execute("UPDATE sync_provider_versions SET state='dismissed' WHERE revision_id='rev-conflict'", []).unwrap();
        let dismissed: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM sync_provider_versions WHERE state='dismissed'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(dismissed, 1);
        let bad = connection.execute("INSERT INTO sync_provider_versions(revision_id,base_revision,sender_device_id,content_hash,encrypted_envelope,state) VALUES('rev-bogus',NULL,'dev-a','h',x'01','bogus')", []);
        assert!(
            bad.is_err(),
            "state outside the CHECK set must still be rejected"
        );
    }

    #[test]
    fn v14_queue_entries_keep_their_eligibility_when_supplement_fields_are_added() {
        let mut connection = Connection::open_in_memory().unwrap();
        migrations()
            .unwrap()
            .to_version(&mut connection, 14)
            .unwrap();
        connection.execute_batch("INSERT INTO conversations(id,title,created_at,updated_at) VALUES('queue-legacy','queue','t','t');
            INSERT INTO messages(id,conversation_id,role,content,created_at) VALUES
              ('pending-input','queue-legacy','user','pending original','t'),
              ('withdrawn-input','queue-legacy','user','withdrawn original','t'),
              ('started-input','queue-legacy','user','started original','t');
            INSERT INTO chat_runs(id,request_key,conversation_id,input_message_id,input_hash,status,executor_id,lease_until,created_at,updated_at)
              VALUES('old-run','old-request','queue-legacy','started-input','basis','completed','old-owner','t','t','t');
            INSERT INTO chat_input_queue(conversation_id,input_message_id,status,run_id,created_at,updated_at) VALUES
              ('queue-legacy','pending-input','pending',NULL,'t','t'),
              ('queue-legacy','withdrawn-input','withdrawn',NULL,'t','t'),
              ('queue-legacy','started-input','started','old-run','t','t');
            INSERT INTO chat_queue_dispatch(conversation_id,paused,updated_at) VALUES('queue-legacy',1,'t');").unwrap();
        let before: String = connection.query_row("SELECT group_concat(id||':'||content,'|') FROM (SELECT id,content FROM messages ORDER BY id)", [], |r| r.get(0)).unwrap();
        initialize(&mut connection).unwrap();
        let after: String = connection.query_row("SELECT group_concat(id||':'||content,'|') FROM (SELECT id,content FROM messages ORDER BY id)", [], |r| r.get(0)).unwrap();
        assert_eq!(before, after);
        let states: String = connection.query_row("SELECT group_concat(status||':'||mode,'|') FROM (SELECT status,mode FROM chat_input_queue ORDER BY id)", [], |r| r.get(0)).unwrap();
        assert_eq!(states, "pending:next|withdrawn:next|started:next");
        let plain: i64 = connection.query_row("SELECT count(*) FROM chat_input_queue WHERE input_basis IS NULL AND context_json='{}' AND defer_reason IS NULL AND target_run_id IS NULL", [], |r| r.get(0)).unwrap();
        assert_eq!(plain, 3, "migration must not infer supplement intent");
        let preserved: bool = connection.query_row("SELECT EXISTS(SELECT 1 FROM chat_input_queue WHERE input_message_id='started-input' AND run_id='old-run') AND EXISTS(SELECT 1 FROM chat_queue_dispatch WHERE conversation_id='queue-legacy' AND paused=1)", [], |r| r.get(0)).unwrap();
        assert!(preserved);
    }

    #[test]
    fn v9_chat_history_and_pending_proposals_remain_unapproved() {
        let mut connection = Connection::open_in_memory().unwrap();
        migrations()
            .unwrap()
            .to_version(&mut connection, 9)
            .unwrap();
        connection.execute_batch("INSERT INTO conversations(id,title,created_at,updated_at) VALUES('old-conv','旧会话','t','t');
            INSERT INTO messages(id,conversation_id,role,content,created_at) VALUES('old-message','old-conv','user','原始消息不变','t');
            INSERT INTO rules(id,content,status,created_at,updated_at,conversation_id) VALUES('old-rule','旧规则草稿','pending','t','t','old-conv');
            INSERT INTO pending_actions(id,conversation_id,action,args_json,created_at) VALUES('old-action','old-conv','create_todo','{\"title\":\"旧草稿\"}','t');").unwrap();
        initialize(&mut connection).unwrap();
        let content: String = connection
            .query_row(
                "SELECT content FROM messages WHERE id='old-message'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(content, "原始消息不变");
        for table in ["rules", "pending_actions"] {
            let (status, version): (String, i64) = connection
                .query_row(
                    &format!("SELECT status,review_version FROM {table}"),
                    [],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .unwrap();
            assert_eq!((status, version), ("pending".into(), 1));
        }
        let decisions: i64 = connection
            .query_row("SELECT COUNT(*) FROM chat_proposal_decisions", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(decisions, 0);
    }

    #[test]
    fn existing_v4_proposals_keep_manual_ownership_after_upgrade() {
        let mut connection = Connection::open_in_memory().unwrap();
        migrations()
            .unwrap()
            .to_version(&mut connection, 4)
            .unwrap();
        connection.execute("INSERT INTO knowledge_proposals(id,dedupe_key,target_slug,kind,title,content_md,reason,created_at)
            VALUES ('old','old','method/old','method','旧建议','旧正文','manual','2026-09-29')", []).unwrap();
        initialize(&mut connection).unwrap();
        let origin: String = connection
            .query_row(
                "SELECT origin FROM knowledge_proposals WHERE id='old'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(origin, "manual");
    }

    #[test]
    fn v7_derivatives_keep_unknown_revision_bases_after_upgrade() {
        let mut connection = Connection::open_in_memory().unwrap();
        migrations()
            .unwrap()
            .to_version(&mut connection, 7)
            .unwrap();
        connection.execute("INSERT INTO wiki_pages(id,slug,kind,title,content_md,first_seen_at,last_seen_at,created_at,updated_at) VALUES('parent','topic/parent','topic','父知识','人工正文','t','t','t','t')",[]).unwrap();
        connection.execute("INSERT INTO wiki_pages(id,slug,kind,title,content_md,based_on,area,first_seen_at,last_seen_at,created_at,updated_at) VALUES('child','der/child','derivative','旧产物','旧正文','topic/parent','derivative','t','t','t','t')",[]).unwrap();
        initialize(&mut connection).unwrap();
        let row: (String, Option<String>) = connection
            .query_row(
                "SELECT upstream_id,basis FROM knowledge_dependencies WHERE page_id='child'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(row, ("parent".into(), None));
        let text: String = connection
            .query_row(
                "SELECT content_md FROM wiki_pages WHERE id='child'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(text, "旧正文");
        assert!(
            connection
                .execute(
                    "UPDATE knowledge_dependencies SET upstream_id='child' WHERE page_id='child'",
                    []
                )
                .is_err()
        );
    }

    /// 旧迁移链留下的 `schema_migrations` 表必须已被 v3 删掉。
    ///
    /// 它声称「1–31」而迁移已重新编号，留着会误导所有后来打开库的人；基线里
    /// 仍建着它是因为基线冻结、不得改，所以只能在 v3 里删——这也正是「只追加、
    /// 不改历史」的意义：新库走完 v1 建、v3 删，与老库终态一致。
    #[test]
    fn legacy_version_table_is_gone() {
        let connection = memory_db();
        let count: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master
                 WHERE type='table' AND name='schema_migrations'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(
            count, 0,
            "版本状态只该记在 PRAGMA user_version 上，不该再有 schema_migrations 表"
        );
    }

    /// 触发器 SQL 里的上限是硬编码字面量（SQL 无法引用 Rust 常量），而报错文案
    /// 用的是 [`crate::storage::MAX_ACTIVE_GOALS`]。两处若漂移，用户会看到
    /// 「已达 4 条上限」却连第 4 条都加不进去。这条测试把漂移变成编译期就红。
    #[test]
    fn trigger_cap_literal_matches_the_error_message_constant() {
        let goals_sql = include_str!("../../migrations/02-goals/up.sql");
        let needle = format!(">= {}", crate::storage::MAX_ACTIVE_GOALS);
        assert_eq!(
            goals_sql.matches(&needle).count(),
            2,
            "两个触发器都应按 MAX_ACTIVE_GOALS 封顶；若你改了上限，\
             改 `goals.rs` 常量后忘了同步 \
             `migrations/02-goals/up.sql` 里的触发器字面量，这里会失败"
        );
    }

    #[test]
    fn cap_rejects_fourth_active_goal() {
        let connection = memory_db();
        for (id, phase) in [("a", "near"), ("b", "near"), ("c", "long")] {
            insert(&connection, id, phase, "active");
        }
        let err = connection
            .execute(
                "INSERT INTO goals(id, content, phase, status, created_at, updated_at)
                 VALUES ('d', 'x', 'mid', 'active', 't', 't')",
                [],
            )
            .unwrap_err();
        assert!(
            err.to_string().contains(ACTIVE_GOAL_LIMIT_REACHED),
            "应因上限被拒，实际: {err}"
        );
    }

    /// 复活已归档目标走 UPDATE 路径，只挡 INSERT 的实现会漏掉这里。
    #[test]
    fn cap_also_guards_the_reactivate_update_path() {
        let connection = memory_db();
        for (id, phase) in [("a", "near"), ("b", "near"), ("c", "long")] {
            insert(&connection, id, phase, "active");
        }
        insert(&connection, "d", "mid", "superseded");

        let err = connection
            .execute("UPDATE goals SET status='active' WHERE id='d'", [])
            .unwrap_err();
        assert!(
            err.to_string().contains(ACTIVE_GOAL_LIMIT_REACHED),
            "复活路径也应受上限约束，实际: {err}"
        );
        assert_eq!(active_count(&connection), 3);
    }

    /// 上限满时编辑活跃目标正文不得被误判为第四条。
    #[test]
    fn editing_a_live_goal_does_not_trip_the_cap() {
        let connection = memory_db();
        for (id, phase) in [("a", "near"), ("b", "near"), ("c", "long")] {
            insert(&connection, id, phase, "active");
        }
        connection
            .execute("UPDATE goals SET content='改了' WHERE id='a'", [])
            .unwrap();
        let content: String = connection
            .query_row("SELECT content FROM goals WHERE id='a'", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(content, "改了");
    }

    #[test]
    fn archiving_frees_a_slot_but_keeps_history() {
        let connection = memory_db();
        for (id, phase) in [("a", "near"), ("b", "near"), ("c", "long")] {
            insert(&connection, id, phase, "active");
        }
        connection
            .execute(
                "UPDATE goals SET status='superseded', superseded_at='t1' WHERE id='a'",
                [],
            )
            .unwrap();
        insert(&connection, "d", "mid", "active");
        assert_eq!(active_count(&connection), 3);
        assert_eq!(superseded_count(&connection), 1);
    }

    /// phase 是标签：同一阶段可以有多条活跃目标。
    #[test]
    fn phase_is_a_label_not_a_slot() {
        let connection = memory_db();
        insert(&connection, "a", "near", "active");
        insert(&connection, "b", "near", "active");
        let n: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM goals WHERE status='active' AND phase='near'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(n, 2);
    }

    fn active_count(connection: &Connection) -> i64 {
        connection
            .query_row(
                "SELECT COUNT(*) FROM goals WHERE status='active'",
                [],
                |row| row.get(0),
            )
            .unwrap()
    }

    fn superseded_count(connection: &Connection) -> i64 {
        connection
            .query_row(
                "SELECT COUNT(*) FROM goals WHERE status='superseded'",
                [],
                |row| row.get(0),
            )
            .unwrap()
    }
}
