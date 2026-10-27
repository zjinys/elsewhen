//! Snapshot publication and relay scan progress are durable but distinct from
//! per-operation application acknowledgements. Incomplete exports never upload.

use anyhow::{Context, Result, ensure};
use rusqlite::{OptionalExtension, params};

use crate::storage::Store;
use crate::sync::crypto::{ContentKey, DeviceIdentity, EnvelopeHeader};
use crate::sync::protocol::validate_id;

/// 一份快照在 relay 上的传输方式。**只有一种**（2026-10-26 C5-A）。
///
/// 曾经这里是个枚举（`Mail`/`Blob`）：mail 分片路径把整份快照封成几百封信推进
/// `sync_wire_outbox`，靠「全塞进去 + 重发兜底」收敛。它在 relay 上留下整份副本、
/// 每片信封套两层 base64（≈2.67×）、且必须手工补上 blob 数据面（R4）本来就有的
/// 三条不变式（有界在途 / 进度反馈 / 断点续传）。实测吞吐 mail 0.38 MB/s 对 blob
/// 4.1 MB/s，而 `from_env()` 的默认值还是 mail —— 也就是说生产一直在跑那条更差、
/// 更复杂的路。C5-A 删掉它：分片只走 blob 数据面，信箱只承载信令。
///
/// 为什么还留着 `sync_snapshot_exports.transport` 列：删列要重建 `sync_snapshot_exports`
/// 与 `sync_snapshots` 两张被外键引用的表，风险与收益不成比例。列现在是**只写
/// 常量**的历史标记（迁移 64 已清空所有 `'mail'` 行），读它的地方只剩「续传必须是
/// blob」这一条闸门。
pub(crate) const SNAPSHOT_TRANSPORT: &str = "blob";

/// `begin_snapshot_export` 的结论。
#[derive(Debug, Clone)]
pub(crate) enum BeginExport {
    /// 新建的导出：从第 0 片开始。
    Begun,
    /// 在途的导出：从已传完的片数继续，不再重封前面的片。
    Resume {
        blob_index: u64,
        blob_id: Option<String>,
    },
    /// 这份传输已经建好或已确认，不需要再做任何事。
    Done,
}

impl Store {
    /// 导出前用域表事实自愈头表：live head 指向已不存在的域行会让接收端
    /// 的 (deleted ⇔ 行存在) 一致性闸门必炸（快照整块作废，见
    /// snapshot/apply.rs 的 Record::Head 校验）。只修状态位，不删记录，
    /// 幂等；历史 FK 中断等原因造成的陈旧头由此复原。
    pub(crate) fn heal_snapshot_heads(&self) -> Result<usize> {
        let mut touched = 0;
        for kind in [
            "conversation",
            "wiki_page",
            "todo",
            "habit_nudge",
            "goal",
            "rule",
        ] {
            let table = crate::storage::sync::snapshot::decode::domain_table(kind)?;
            touched += self.connection.execute(
                &format!(
                    "UPDATE sync_object_heads SET deleted=1
                     WHERE object_kind=?1 AND deleted=0
                       AND NOT EXISTS(SELECT 1 FROM {table} WHERE id=object_id)"
                ),
                [kind],
            )?;
        }
        Ok(touched)
    }

    /// False means this exact transfer is already ready (or acknowledged). An
    /// interrupted export needs a new request, not a different view under old IDs.
    pub(crate) fn begin_snapshot_export(
        &self,
        id: &str,
        recipient: &str,
        key_version: u64,
    ) -> Result<BeginExport> {
        validate_id(id)?;
        validate_id(recipient)?;
        ensure!(
            (1..=i64::MAX as u64).contains(&key_version),
            "sync_snapshot_key_version_invalid"
        );
        let tx = self.begin_write_transaction()?;
        let identity = self.sync_identity()?.context("sync_not_initialized")?;
        let active: bool = self.connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM sync_members WHERE device_id=?1 AND active=1)",
            [recipient],
            |r| r.get(0),
        )?;
        ensure!(
            active && recipient != identity.device_id,
            "sync_snapshot_recipient_not_authorized"
        );
        // logical_clock 与导出视图一起钉住：blob 续传跨拍重开 exporter，只有数据
        // 一格没动才允许接着传。漂了就必须换新请求 id —— 半新半旧的片拼出来的
        // 快照 manifest 摘要对不上，接收端整份作废。
        let logical_clock = self.replica_logical_clock()?;
        let existing: Option<(String, i64, String, i64, Option<String>, i64)> = self
            .connection
            .query_row(
                "SELECT recipient_device_id,key_version,state,blob_index,blob_id,blob_clock
                 FROM sync_snapshot_exports WHERE id=?1",
                [id],
                |r| {
                    Ok((
                        r.get(0)?,
                        r.get(1)?,
                        r.get(2)?,
                        r.get(3)?,
                        r.get(4)?,
                        r.get(5)?,
                    ))
                },
            )
            .optional()?;
        if let Some((peer, version, state, blob_index, blob_id, blob_clock)) = existing {
            ensure!(
                peer == recipient && version == key_version as i64,
                "sync_snapshot_identity_conflict"
            );
            if matches!(state.as_str(), "ready" | "acknowledged") {
                if let Some(tx) = tx {
                    tx.commit()?;
                }
                return Ok(BeginExport::Done);
            }
            // 同 id 续传：导出的进度是逐片 durable 的（`blob_index`），续传不重传
            // 来过的片。
            //
            // 这里不再检查 `transport` 列：值只可能是 `'blob'`（迁移 64 用触发器把
            // `'mail'` 写入直接拒掉，存量 mail 行也已清空）。原先这条比对是「在途
            // 导出不许被环境变量降级」的闸门；环境变量没了，闸门也就没东西可挡。
            //
            // `aborted` 为什么也能续传（review 2026-10-25 M1）：`abort_snapshot_export`
            // 只作用于 `building`，所以 `aborted` 恒等于「建过但没建成」，不会出现
            // 「已写成又 abort」。而 `blob_index`/`blob_id`/`blob_clock` 三列在 abort 时
            // 未被触碰，续传点由 `upload_blob_chunks` 与 relay 事实对齐——
            // `blob_open` 说 created 就从 0 开始，否则取 `max(本地, relay.highest+1)`，
            // 因此续传**不会跳过没上传的片**。不放行的话，任何一次 staging 失败
            // （断电/provay 抖一下）都会把 `claim_sync_resnapshot` 拿到的**同一个 id**
            // 变成永久失败：每拍重 begin 一次、每次都 `requires_new_request`。
            ensure!(
                matches!(state.as_str(), "building" | "aborted"),
                "sync_snapshot_export_requires_new_request"
            );
            ensure!(
                blob_clock == logical_clock,
                "sync_snapshot_export_requires_new_request"
            );
            // 从 `aborted` 续传时把状态摆回 `building`：否则后续 `abort_snapshot_export`
            // （只作用于 building）会变成空操作，失败也只能停在 aborted；而发布路径
            // 也要求 building。幂等：本来就是 building 时这一句不改变任何东西。
            if state != "building" {
                self.connection.execute(
                    "UPDATE sync_snapshot_exports SET state='building' WHERE id=?1",
                    [id],
                )?;
            }
            if let Some(tx) = tx {
                tx.commit()?;
            }
            return Ok(BeginExport::Resume {
                blob_index: blob_index.max(0) as u64,
                blob_id,
            });
        }
        // A claimed re-snapshot (v30) may only be built under the request's
        // current id: an id rotated after re-flagging can never be satisfied
        // by an export built for the older request.
        let expected_request: Option<String> = self
            .connection
            .query_row(
                "SELECT snapshot_id FROM sync_resnapshot_requests WHERE device_id=?1",
                [recipient],
                |r| r.get(0),
            )
            .optional()?;
        if let Some(expected) = expected_request {
            ensure!(expected == id, "sync_snapshot_resnapshot_request_mismatch");
        }
        // Pin the recipient's starting application watermark to the LOCAL
        // stream frontier at snapshot-build time. The exported snapshot contains
        // exactly the ops up to this frontier, so this is the correct starting
        // watermark — never 0 (holds back GC) and never the live frontier
        // (over-claims ops captured after the snapshot).
        let local_watermark: i64 = self
            .connection
            .query_row(
                "SELECT applied_sequence FROM sync_frontiers WHERE origin_stream_id=?1",
                [&identity.stream_id],
                |r| r.get(0),
            )
            .optional()?
            .unwrap_or(0);
        // Which signed roster the export will embed as its authority frame.
        // 0 stays valid for pre-authority flows (none in-tree today); the
        // receiver cross-checks the embedded roster revision on completion.
        let roster_revision: i64 = self
            .current_signed_roster()?
            .map(|s| s.roster.revision as i64)
            .unwrap_or(0);
        self.connection.execute(
            "INSERT INTO sync_snapshot_exports(id,recipient_device_id,key_version,state,local_watermark,roster_revision,transport,blob_clock) VALUES(?1,?2,?3,'building',?4,?5,?6,?7)",
            params![id,recipient,key_version as i64,local_watermark,roster_revision,SNAPSHOT_TRANSPORT,logical_clock],
        )?;
        if let Some(tx) = tx {
            tx.commit()?;
        }
        Ok(BeginExport::Begun)
    }

    /// 记下这份导出的 blob 身份。开一次就一直复用，重试不再开新 blob。
    pub(crate) fn set_snapshot_blob_id(&self, id: &str, blob_id: &str) -> Result<()> {
        ensure!(
            (16..=64).contains(&blob_id.len())
                && blob_id
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-'),
            "sync_snapshot_blob_id_invalid"
        );
        let changed = self.connection.execute(
            "UPDATE sync_snapshot_exports SET blob_id=?2 WHERE id=?1 AND state='building'",
            params![id, blob_id],
        )?;
        ensure!(changed == 1, "sync_snapshot_export_not_building");
        Ok(())
    }

    /// 上传进度：已传完的分片数。每片一提交，丢响应/断电都能从断点继续。
    pub(crate) fn mark_snapshot_blob_progress(&self, id: &str, index: u64) -> Result<()> {
        ensure!(index <= i64::MAX as u64, "sync_snapshot_blob_index_invalid");
        let changed = self.connection.execute(
            "UPDATE sync_snapshot_exports SET blob_index=?2 WHERE id=?1 AND state='building'",
            params![id, index as i64],
        )?;
        ensure!(changed == 1, "sync_snapshot_export_not_building");
        Ok(())
    }

    pub(crate) fn stage_snapshot_authority(
        &self,
        id: &str,
        header: &EnvelopeHeader,
        key: &ContentKey,
        signer: &DeviceIdentity,
        payload: &[u8],
    ) -> Result<()> {
        let tx = self.begin_write_transaction()?;
        let valid: bool = self.connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM sync_snapshot_exports WHERE id=?1 AND recipient_device_id=?2 AND key_version=?3 AND state='building')",
            params![id, header.recipient_device_id, header.key_version as i64], |r| r.get(0))?;
        ensure!(
            valid && header.message_id.starts_with("membership:"),
            "sync_snapshot_authority_mismatch"
        );
        self.stage_sync_wire_envelope(header, key, signer, payload)?;
        // A wire has one export owner. Only an abandoned attempt may hand its
        // identical roster to a new attempt; ready exports keep their bindings.
        let changed = self.connection.execute(
            "INSERT INTO sync_snapshot_export_wires(snapshot_id,recipient_device_id,message_id) VALUES(?1,?2,?3)
             ON CONFLICT(recipient_device_id,message_id) DO UPDATE SET snapshot_id=excluded.snapshot_id
             WHERE sync_snapshot_export_wires.snapshot_id=excluded.snapshot_id
                OR EXISTS(SELECT 1 FROM sync_snapshot_exports s WHERE s.id=sync_snapshot_export_wires.snapshot_id AND s.state='aborted')",
            params![id, header.recipient_device_id, header.message_id],
        )?;
        ensure!(changed == 1, "sync_snapshot_authority_already_in_use");
        if let Some(tx) = tx {
            tx.commit()?;
        }
        Ok(())
    }

    /// 把这份导出的 **manifest**（`snapshot-end:{id}`）排进 outbox，并把导出置 `ready`。
    ///
    /// 分片不在 outbox 里（它们走 blob 数据面），所以这里是导出唯一会进信箱的东西：
    /// 它宣告「分片都传完了，去 `blob_id` 拉」，也是接收方唯一能断言「全片已落盘」的证据。
    pub(crate) fn stage_snapshot_manifest(
        &self,
        id: &str,
        header: &EnvelopeHeader,
        key: &ContentKey,
        signer: &DeviceIdentity,
        payload: &[u8],
    ) -> Result<()> {
        let tx = self.begin_write_transaction()?;
        let (peer, version, state): (String, i64, String) = self.connection.query_row(
            "SELECT recipient_device_id,key_version,state FROM sync_snapshot_exports WHERE id=?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )?;
        ensure!(
            state == "building"
                && peer == header.recipient_device_id
                && version as u64 == header.key_version,
            "sync_snapshot_export_not_building"
        );
        ensure!(
            header.message_id == format!("snapshot-end:{id}"),
            "sync_snapshot_identity_conflict"
        );
        self.stage_sync_wire_envelope(header, key, signer, payload)?;
        self.connection.execute(
            "INSERT INTO sync_snapshot_export_wires(snapshot_id,recipient_device_id,message_id) VALUES(?1,?2,?3)",
            params![id,peer,header.message_id],
        )?;
        self.connection.execute(
            "UPDATE sync_snapshot_exports SET state='ready' WHERE id=?1",
            [id],
        )?;
        if let Some(tx) = tx {
            tx.commit()?;
        }
        Ok(())
    }

    pub(crate) fn abort_snapshot_export(&self, id: &str) -> Result<()> {
        self.connection.execute(
            "UPDATE sync_snapshot_exports SET state='aborted' WHERE id=?1 AND state='building'",
            [id],
        )?;
        Ok(())
    }

    /// 只有经签名认证的「最终应用回执」能退休这份导出。被覆盖的操作有自己的
    /// 确认，这不是 GC 标记。
    ///
    /// subject 恒为 `snapshot-end:{id}`（manifest）：它是接收方唯一能断言「全片
    /// 已落盘且已发布」的事实。分片级确认在 blob 数据面上由 `blob_ack` 完成，
    /// 不经过这张表 —— mail 分片路径和它的逐片回执已于 2026-10-26（C5-A）删除。
    pub(crate) fn acknowledge_snapshot_export(
        &self,
        peer: &str,
        id: &str,
        subject: &str,
        payload_hash: &str,
    ) -> Result<()> {
        let tx = self.begin_write_transaction()?;
        let ready: bool = self.connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM sync_snapshot_exports WHERE id=?1 AND recipient_device_id=?2 AND state IN ('ready','acknowledged'))",
            params![id,peer], |r| r.get(0),
        )?;
        ensure!(ready, "sync_snapshot_export_not_ready");
        self.acknowledge_sync_wire(peer, subject, payload_hash)?;
        self.connection.execute(
            "UPDATE sync_wire_outbox SET state='acknowledged' WHERE recipient_device_id=?1 AND message_id IN (SELECT message_id FROM sync_snapshot_export_wires WHERE snapshot_id=?2 AND recipient_device_id=?1)",
            params![peer,id],
        )?;
        self.connection.execute(
            "UPDATE sync_snapshot_exports SET state='acknowledged' WHERE id=?1",
            [id],
        )?;
        // The recipient has confirmed the snapshot: seed its application
        // watermark at the pinned build-time frontier so it neither holds back
        // GC (as 0 would) nor over-claims (as the live frontier would).
        let watermark: i64 = self.connection.query_row(
            "SELECT local_watermark FROM sync_snapshot_exports WHERE id=?1",
            [id],
            |r| r.get(0),
        )?;
        let identity = self.sync_identity()?.context("sync_not_initialized")?;
        let mut frontiers = std::collections::BTreeMap::new();
        frontiers.insert(identity.stream_id.clone(), watermark);
        self.mark_member_snapshotted(peer, &frontiers)?;
        if let Some(tx) = tx {
            tx.commit()?;
        }
        Ok(())
    }

    /// `sync_wire_outbox` 里还没被对端回执的封包：条数 + 字节数。
    ///
    /// 这是「同步卡在哪」唯一诚实的可观测口径（缺陷 D4：主界面此前完全看不到
    /// 积压）。条数说明还有多少消息在飞，字节数说明量级 —— 锁定时用户最想知道的
    /// 是「还要不要等」，只给条数给不出这个判断。
    ///
    /// 只算 `state='pending'`：`acknowledged` 是终态，留在表里的是历史记录。
    /// 这个设备是否还在我的名册里、且未被撤销。
    ///
    /// 迟到重投的判定只能用这一个事实：对端是成员 ⇒ 交接那封信要达成的事已经
    /// 达成（幂等收尾）；不是成员 ⇒ 这封信本来就不该到这儿，照旧拒收。
    pub(crate) fn sync_member_is_active(&self, device_id: &str) -> Result<bool> {
        Ok(self.connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM sync_members WHERE device_id=?1 AND active=1)",
            [device_id],
            |r| r.get(0),
        )?)
    }

    pub(crate) fn pending_wire_backlog(&self) -> Result<(i64, i64)> {
        Ok(self.connection.query_row(
            "SELECT COUNT(*), COALESCE(SUM(LENGTH(encrypted_envelope)), 0) \
             FROM sync_wire_outbox WHERE state='pending'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?)
    }

    pub(crate) fn snapshot_import_pending(&self) -> Result<bool> {
        Ok(self.connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM sync_snapshot_target t JOIN sync_snapshots s ON s.id=t.snapshot_id WHERE s.state='staging')",
            [], |r| r.get(0),
        )?)
    }

    pub(crate) fn staged_snapshot_manifest(&self, id: &str) -> Result<Option<Vec<u8>>> {
        Ok(self.connection.query_row(
            "SELECT manifest FROM sync_snapshots WHERE id=?1",
            [id],
            |r| r.get(0),
        )?)
    }

    pub(crate) fn relay_scan_cursor(&self, mailbox: &str) -> Result<String> {
        Ok(self
            .connection
            .query_row(
                "SELECT after_id FROM sync_relay_cursors WHERE mailbox_id=?1",
                [mailbox],
                |r| r.get(0),
            )
            .optional()?
            .unwrap_or_else(|| "0".into()))
    }

    /// 「必表态」计数（R5-2）：同一封邮件被扫描拒绝一次就 +1，返回累计次数。
    ///
    /// 为什么必须落库：扫描游标是持久的，进程重启后同一封坏信会被重新扫到；
    /// 计数放内存等于每次重启重新数，阈值永远到不了 —— 事故里 9996 封死信正是
    /// 「每圈都拒、每圈都不删、谁也不表态」堆出来的。
    pub(crate) fn record_wire_rejection(
        &self,
        mailbox: &str,
        sender_mailbox: &str,
        mail_id: &str,
        message_id: &str,
        error: &str,
        now_ms: i64,
    ) -> Result<i64> {
        // 冲突键是 `(mail_id, mailbox_id)`：relay 一个信箱一个库文件，`mail_id` 只是
        // 那个文件里的 rowid，换信箱后同一个字符串会落到另一封信上（迁移 63）。
        //
        // `WHERE dropped_at IS NULL`：已经表态放弃的行是审计，不再被扫到的信改写 ——
        // 否则「几次之后放弃」这个事实会被后续扫描悄悄抹掉，计数也会复活。
        self.connection.execute(
            "INSERT INTO sync_wire_rejections(mail_id,mailbox_id,sender_mailbox_id,message_id,attempts,last_error,first_seen_at,last_seen_at) \
             VALUES(?1,?2,?3,?4,1,?5,?6,?6) \
             ON CONFLICT(mail_id,mailbox_id) DO UPDATE SET attempts=sync_wire_rejections.attempts+1, \
               last_error=excluded.last_error, last_seen_at=excluded.last_seen_at, \
               message_id=CASE WHEN excluded.message_id='' THEN sync_wire_rejections.message_id ELSE excluded.message_id END \
             WHERE sync_wire_rejections.dropped_at IS NULL",
            params![mail_id, mailbox, sender_mailbox, message_id, error, now_ms],
        )?;
        Ok(self.connection.query_row(
            "SELECT attempts FROM sync_wire_rejections WHERE mail_id=?1 AND mailbox_id=?2",
            params![mail_id, mailbox],
            |r| r.get(0),
        )?)
    }

    /// 已表态放弃：行留着做审计（哪个 subject、为什么、几次之后放弃），
    /// 但不再参与阈值判定。
    pub(crate) fn mark_wire_rejection_dropped(
        &self,
        mailbox: &str,
        mail_id: &str,
        now_ms: i64,
    ) -> Result<()> {
        self.connection.execute(
            "UPDATE sync_wire_rejections SET dropped_at=?3 WHERE mail_id=?1 AND mailbox_id=?2 AND dropped_at IS NULL",
            params![mail_id, mailbox, now_ms],
        )?;
        Ok(())
    }

    /// 接受成功就整行删掉：这封信已经消费并 ack，留着只是一个永远不会被读的计数。
    pub(crate) fn forget_wire_rejection(&self, mailbox: &str, mail_id: &str) -> Result<()> {
        self.connection.execute(
            "DELETE FROM sync_wire_rejections WHERE mail_id=?1 AND mailbox_id=?2",
            params![mail_id, mailbox],
        )?;
        Ok(())
    }

    /// 清掉过期审计行。信件的 relay TTL 是 7 天，超过这个窗口的记录既不可能
    /// 再被扫到，也没有排查价值；不清理会让这张表随死信无限增长（就是「为治
    /// 无限增长又添一张无限增长的表」）。
    pub(crate) fn prune_wire_rejections(&self, now_ms: i64) -> Result<()> {
        const RETENTION_MS: i64 = 7 * 24 * 60 * 60 * 1000;
        self.connection.execute(
            "DELETE FROM sync_wire_rejections WHERE last_seen_at < ?1",
            [now_ms - RETENTION_MS],
        )?;
        Ok(())
    }

    /// This is a scan cursor, not an ack: unresolved mail remains at the relay and
    /// is revisited after wrapping. Poison mail cannot monopolize the first page.
    pub(crate) fn save_relay_scan_cursor(&self, mailbox: &str, cursor: &str) -> Result<()> {
        self.connection.execute(
            "INSERT INTO sync_relay_cursors(mailbox_id,after_id) VALUES(?1,?2) ON CONFLICT(mailbox_id) DO UPDATE SET after_id=excluded.after_id",
            params![mailbox,cursor],
        )?;
        Ok(())
    }
}
