//! 快照记录落库（后半段分支：skill/wiki 资产与同步成员/检查点）。

use super::*;

impl Store {
    pub(crate) fn apply_snapshot_record_rest(
        &self,
        record: &Record,
        snapshot_id: &str,
        manifest: &SnapshotManifest,
        provider_seen: &mut bool,
    ) -> Result<()> {
        match record {
            Record::SkillFile {
                skill_id,
                rel,
                sha256,
                size,
                updated_at,
                bytes_hex,
            } => {
                // 技能分发字节只进表（分发事实源），目录由接收端启动 reconcile
                // 从表落盘——不在快照 apply 里碰技能目录，保持「库先行」的单一顺序。
                // 闸门：skill_id 形态 / rel 钉在技能目录内（无越界、无隐藏文件段）/
                // hex 长度 / sha256 与字节一致。
                let rel_ok = !rel.is_empty()
                    && rel.len() <= 256
                    && !rel.starts_with('/')
                    && !rel.contains('\\')
                    && rel.split('/').all(|seg| {
                        !seg.is_empty() && seg != "." && seg != ".." && !seg.starts_with('.')
                    });
                ensure!(
                    validate_id(skill_id).is_ok()
                        && rel_ok
                        && *size >= 0
                        && bytes_hex.len() == (*size as usize) * 2,
                    "sync_snapshot_skill_file_invalid"
                );
                let bytes = hex_to_vec(bytes_hex)?;
                ensure!(
                    bytes.len() == *size as usize,
                    "sync_snapshot_skill_file_invalid"
                );
                let digest = hex(ring::digest::digest(&SHA256, &bytes).as_ref());
                ensure!(&digest == sha256, "sync_snapshot_skill_file_hash_mismatch");
                self.connection.execute(
                    "INSERT INTO skill_files(skill_id,rel,bytes,sha256,size,updated_at)
                     VALUES(?1,?2,?3,?4,?5,?6)
                     ON CONFLICT(skill_id,rel) DO UPDATE SET
                       bytes=excluded.bytes, sha256=excluded.sha256,
                       size=excluded.size, updated_at=excluded.updated_at",
                    params![skill_id, rel, bytes, sha256, size, updated_at],
                )?;
            }
            Record::SkillTombstone {
                skill_id,
                updated_at,
            } => {
                ensure!(
                    validate_id(skill_id).is_ok() && !updated_at.is_empty(),
                    "sync_snapshot_skill_tombstone_invalid"
                );
                self.connection.execute(
                    "INSERT INTO skill_tombstones(skill_id,updated_at) VALUES(?1,?2)
                     ON CONFLICT(skill_id) DO UPDATE SET
                       updated_at=excluded.updated_at
                     WHERE excluded.updated_at > skill_tombstones.updated_at",
                    params![skill_id, updated_at],
                )?;
            }
            Record::McpServer {
                id,
                name,
                platform,
                transport,
                command,
                args_json,
                env_json,
                url,
                enabled,
                created_at,
                updated_at,
            } => {
                // 闸门：id 形态、name 字符集（工具名拼接安全）、platform 与
                // transport 值域、args/env 合法 JSON。headers/trusted 不在
                // 快照记录里（凭据不离本机；信任是本机事实）。
                ensure!(validate_id(id).is_ok(), "sync_snapshot_mcp_server_invalid");
                ensure!(
                    !name.is_empty()
                        && name
                            .chars()
                            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'),
                    "sync_snapshot_mcp_server_invalid"
                );
                ensure!(
                    matches!(platform.as_str(), "all" | "linux" | "macos" | "windows"),
                    "sync_snapshot_mcp_server_invalid"
                );
                ensure!(
                    matches!(transport.as_str(), "stdio" | "streamable-http"),
                    "sync_snapshot_mcp_server_invalid"
                );
                validate_json_arg(args_json, "sync_snapshot_mcp_server_invalid")?;
                validate_json_arg(env_json, "sync_snapshot_mcp_server_invalid")?;
                // 本机获胜：同 id 或同 (name,platform) 已有行 → 忽略远端行
                // （INSERT OR IGNORE 对主键与唯一索引两个目标都会静默跳过）。
                // trusted 强制 0：同步到达的 server 回到逐次审批，用户看过、
                // 信任了再开——URL 可以搭车，豁免不搭车。
                self.connection.execute(
                    "INSERT OR IGNORE INTO mcp_servers
                        (id,name,platform,transport,command,args_json,env_json,url,headers_json,enabled,trusted,created_at,updated_at)
                     VALUES(?1,?2,?3,?4,?5,?6,?7,?8,'{}',?9,0,?10,?11)",
                    params![
                        id,
                        name,
                        platform,
                        transport,
                        command,
                        args_json,
                        env_json,
                        url,
                        *enabled as i64,
                        created_at,
                        updated_at
                    ],
                )?;
            }
            Record::KnowledgeSource {
                id,
                identity,
                source_kind,
                locator,
                opinion,
                reject_reason,
                created_at,
            } => {
                // 负知识台账（migration 50）：id 形态与 opinion 值域闘门;
                // 本机获胜（INSERT OR IGNORE）：台账是导入事实，同 identity
                // 本机已导入时不被远端行覆写；纯「否定决定」随行到达。
                ensure!(
                    validate_id(id).is_ok(),
                    "sync_snapshot_knowledge_source_invalid"
                );
                ensure!(
                    !identity.is_empty(),
                    "sync_snapshot_knowledge_source_invalid"
                );
                ensure!(
                    matches!(source_kind.as_str(), "webpage" | "tweet" | "file" | "text"),
                    "sync_snapshot_knowledge_source_invalid"
                );
                ensure!(
                    opinion
                        .as_deref()
                        .is_none_or(|o| o == "endorse" || o == "reject"),
                    "sync_snapshot_knowledge_source_invalid"
                );
                self.connection.execute(
                    "INSERT OR IGNORE INTO knowledge_sources
                        (id,identity,kind,locator,opinion,reject_reason,created_at)
                     VALUES(?1,?2,?3,?4,?5,?6,?7)",
                    params![
                        id,
                        identity,
                        &source_kind,
                        locator,
                        opinion,
                        reject_reason,
                        created_at
                    ],
                )?;
            }
            Record::Member {
                device_id,
                stream_id,
                active,
            } => {
                validate_id(device_id)?;
                validate_id(stream_id)?;
                if self.signed_snapshot_member(device_id, stream_id)? {
                    // Keep the latest owner-signed permission, not this content
                    // snapshot's possibly older active/revoked projection.
                    return Ok(());
                }
                let identity = self.sync_identity()?.context("sync_not_initialized")?;
                if device_id == &identity.device_id && *active {
                    ensure!(
                        stream_id == &identity.stream_id,
                        "sync_snapshot_local_stream_conflict"
                    );
                }
                if stream_id == &identity.stream_id {
                    ensure!(
                        device_id == &identity.device_id && *active,
                        "sync_snapshot_local_member_invalid"
                    );
                }
                let existing: Option<String> = self
                    .connection
                    .query_row(
                        "SELECT device_id FROM sync_members WHERE stream_id=?1",
                        [stream_id],
                        |r| r.get(0),
                    )
                    .optional()?;
                ensure!(
                    existing.as_deref().is_none_or(|id| id == device_id),
                    "sync_stream_identity_conflict"
                );
                self.connection.execute("INSERT INTO sync_members(device_id,stream_id,active) VALUES(?1,?2,?3) ON CONFLICT(stream_id) DO UPDATE SET active=excluded.active",params![device_id,stream_id,active])?;
            }
            Record::Checkpoint {
                op_id,
                origin_device_id,
                origin_stream_id,
                origin_sequence,
                payload_hash,
            } => {
                validate_id(origin_device_id)?;
                validate_id(origin_stream_id)?;
                self.signed_snapshot_member(origin_device_id, origin_stream_id)?;
                ensure!(
                    *origin_sequence > 0
                        && *op_id == format!("{origin_stream_id}:{origin_sequence}")
                        && manifest
                            .frontiers
                            .get(origin_stream_id)
                            .is_some_and(|n| *n >= *origin_sequence)
                        && valid_hash(payload_hash),
                    "sync_snapshot_checkpoint_invalid"
                );
                let member: Option<String> = self
                    .connection
                    .query_row(
                        "SELECT device_id FROM sync_members WHERE stream_id=?1",
                        [origin_stream_id],
                        |r| r.get(0),
                    )
                    .optional()?;
                ensure!(
                    member.as_deref() == Some(origin_device_id.as_str()),
                    "sync_snapshot_origin_identity_invalid"
                );
                self.connection.execute("INSERT INTO sync_checkpoint_operations(op_id,origin_device_id,origin_stream_id,origin_sequence,payload_hash) VALUES(?1,?2,?3,?4,?5)",params![op_id,origin_device_id,origin_stream_id,origin_sequence,payload_hash])?;
            }
            Record::Operation {
                value,
                state,
                payload_hash,
            } => {
                value.validate()?;
                self.signed_snapshot_member(&value.origin_device_id, &value.origin_stream_id)?;
                ensure!(
                    value.dataset_id == manifest.dataset_id
                        && matches!(state.as_str(), "applied" | "conflict")
                        && manifest
                            .frontiers
                            .get(&value.origin_stream_id)
                            .is_some_and(|n| *n >= value.origin_sequence),
                    "sync_snapshot_operation_invalid"
                );
                let member: Option<String> = self
                    .connection
                    .query_row(
                        "SELECT device_id FROM sync_members WHERE stream_id=?1",
                        [&value.origin_stream_id],
                        |r| r.get(0),
                    )
                    .optional()?;
                ensure!(
                    member.as_deref() == Some(value.origin_device_id.as_str())
                        && value.logical_clock <= manifest.logical_clock,
                    "sync_snapshot_origin_identity_invalid"
                );
                let payload = serde_json::to_string(value)?;
                ensure!(
                    super::hash(payload.as_bytes()) == *payload_hash,
                    "sync_snapshot_operation_hash_mismatch"
                );
                self.connection.execute("INSERT INTO sync_operations(op_id,origin_device_id,origin_stream_id,origin_sequence,payload,payload_hash,local,state) VALUES(?1,?2,?3,?4,?5,?6,0,?7)",params![value.op_id,value.origin_device_id,value.origin_stream_id,value.origin_sequence,payload,payload_hash,state])?;
            }
            Record::Conflict {
                op_id,
                object_kind,
                object_id,
                current_revision,
                reason,
            } => {
                self.connection.execute("INSERT INTO sync_conflicts(op_id,object_kind,object_id,current_revision,reason) VALUES(?1,?2,?3,?4,?5)",params![op_id,object_kind,object_id,current_revision,reason])?;
            }
            Record::ProviderCheckpoint {
                revision_id,
                content_hash,
            } => {
                validate_id(revision_id)?;
                ensure!(valid_hash(content_hash), "sync_snapshot_checkpoint_invalid");
                let existing: Option<String> = self
                    .connection
                    .query_row(
                        "SELECT content_hash FROM sync_checkpoint_providers WHERE revision_id=?1",
                        [revision_id],
                        |r| r.get(0),
                    )
                    .optional()?;
                ensure!(
                    existing.as_deref().is_none_or(|hash| hash == content_hash),
                    "sync_provider_revision_content_conflict"
                );
                self.connection.execute("INSERT OR IGNORE INTO sync_checkpoint_providers(revision_id,content_hash,snapshot_id) VALUES(?1,?2,?3)",params![revision_id,content_hash,snapshot_id])?;
            }
            Record::Provider { value } => {
                ensure!(!*provider_seen, "sync_snapshot_duplicate_provider_bundle");
                value.validate()?;
                self.replace_provider_bundle_on(value)?;
                self.connection.execute("INSERT INTO sync_provider_state(singleton,revision_id,base_revision) VALUES(1,?1,?2) ON CONFLICT(singleton) DO UPDATE SET revision_id=excluded.revision_id,base_revision=excluded.base_revision",params![value.revision_id,value.base_revision])?;
                let digest = super::hash(&serde_json::to_vec(value)?);
                let old: Option<String> = self
                    .connection
                    .query_row(
                        "SELECT content_hash FROM sync_checkpoint_providers WHERE revision_id=?1",
                        [&value.revision_id],
                        |r| r.get(0),
                    )
                    .optional()?;
                ensure!(
                    old.as_deref().is_none_or(|hash| hash == digest),
                    "sync_provider_revision_content_conflict"
                );
                self.connection.execute("INSERT OR IGNORE INTO sync_checkpoint_providers(revision_id,content_hash,snapshot_id) VALUES(?1,?2,?3)",params![value.revision_id,digest,snapshot_id])?;
                *provider_seen = true;
            }
            _ => {
                ensure!(false, "sync_snapshot_record_kind_unhandled")
            }
        }
        Ok(())
    }
}
