//! 快照记录落库与帧解码。

use super::*;

impl Store {
    pub(crate) fn apply_snapshot_record(
        &self,
        snapshot_id: &str,
        manifest: &SnapshotManifest,
        record: &Record,
        provider_seen: &mut bool,
    ) -> Result<()> {
        match record {
            Record::Event {
                value,
                status,
                processed_at,
                updated_at,
            } => {
                validate_id(&value.id)?;
                ensure!(
                    matches!(status.as_str(), "pending" | "processed"),
                    "sync_snapshot_event_status_invalid"
                );
                crate::storage::events::insert_raw_event_on(
                    &self.connection,
                    &value.id,
                    &value.occurred_at,
                    &value.recorded_at,
                    &value.raw_text,
                    &value.source,
                    &value.created_at,
                )?;
                self.connection.execute(
                    "UPDATE events SET status=?2,processed_at=?3,updated_at=?4 WHERE id=?1",
                    params![value.id, status, processed_at, updated_at],
                )?;
            }
            Record::Conversation { value } => {
                validate_id(&value.id)?;
                if let Some(slug) = &value.wiki_page_slug {
                    self.sync_mark_wiki_slug_seen(slug)?;
                }
                self.connection.execute("INSERT INTO conversations(id,title,tag,archived,wiki_page_slug,assistant_mode,created_at,updated_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8)",params![value.id,value.title,value.tag,value.archived,value.wiki_page_slug,value.assistant_mode,value.created_at,value.updated_at])?;
            }
            Record::Message { value, order } => {
                validate_id(&value.id)?;
                if let Some(parent) = &value.parent_message_id {
                    let parent: Option<String> = self
                        .connection
                        .query_row(
                            "SELECT conversation_id FROM messages WHERE id=?1",
                            [parent],
                            |r| r.get(0),
                        )
                        .optional()?;
                    ensure!(
                        parent.as_deref() == Some(value.conversation_id.as_str()),
                        "sync_snapshot_parent_missing_or_invalid"
                    );
                }
                self.connection.execute("INSERT INTO messages(id,conversation_id,parent_message_id,role,content,created_at) VALUES(?1,?2,?3,?4,?5,?6)",params![value.id,value.conversation_id,value.parent_message_id,value.role,value.content,value.created_at])?;
                if let Some(input) = &value.input_message_id {
                    self.connection.execute("INSERT INTO message_response_links(message_id,input_message_id) VALUES(?1,?2)",params![value.id,input])?;
                }
                if let Some(order) = order {
                    validate_id(&order.origin_stream_id)?;
                    ensure!(
                        order.origin_sequence > 0
                            && order.logical_clock > 0
                            && order.logical_clock <= manifest.logical_clock,
                        "sync_snapshot_message_order_invalid"
                    );
                    self.connection.execute("INSERT INTO sync_message_order(message_id,origin_stream_id,origin_sequence,logical_clock,input_message_id) VALUES(?1,?2,?3,?4,?5)",params![value.id,order.origin_stream_id,order.origin_sequence,order.logical_clock,order.input_message_id])?;
                }
            }
            Record::Input { value } => {
                validate_id(&value.id)?;
                self.connection.execute("INSERT INTO input_records(id,raw_text,source,route_status,event_id,message_id,created_at,updated_at) VALUES(?1,?2,?3,'routed',?4,?5,?6,?6)",params![value.id,value.raw_text,value.source,value.event_id,value.message_id,value.created_at])?;
            }
            Record::Analysis {
                id,
                event_id,
                prompt_version,
                result_json,
                created_at,
            } => {
                validate_id(id)?;
                self.connection.execute("INSERT INTO event_analyses(id,event_id,prompt_version,result_json,created_at) VALUES(?1,?2,?3,?4,?5)",params![id,event_id,prompt_version,result_json,created_at])?;
            }
            Record::Classification {
                id,
                event_id,
                recordable,
                event_kind,
                reason,
                created_at,
            } => {
                validate_id(id)?;
                self.connection.execute("INSERT INTO event_recordability_decisions(id,event_id,recordable,kind,reason,created_at) VALUES(?1,?2,?3,?4,?5,?6)",params![id,event_id,recordable,event_kind,reason,created_at])?;
            }
            Record::Head {
                object_kind,
                object_id,
                revision_id,
                deleted,
            } => {
                // revision_id 即 op_id（形如 stream:N，含冒号），不能过
                // validate_id；真正的闸门是领域表存在性复核。
                let table = domain_table(object_kind)?;
                let exists: bool = self.connection.query_row(
                    &format!("SELECT EXISTS(SELECT 1 FROM {table} WHERE id=?1)"),
                    [object_id],
                    |r| r.get(0),
                )?;
                ensure!(exists != *deleted, "sync_snapshot_head_mismatch");
                self.sync_set_head(object_kind, object_id, revision_id, *deleted)?;
            }
            Record::KnownRevision {
                object_kind,
                object_id,
                revision_id,
            } => {
                // domain_table 兼作 object_kind 的 fail-closed 校验（非域对象报错），
                // 返回的表名此处用不到（known_revisions 自带 object_kind 列）。
                let _table = domain_table(object_kind)?;
                self.connection.execute("INSERT OR IGNORE INTO sync_known_revisions(object_kind,object_id,revision_id) VALUES(?1,?2,?3)",params![object_kind,object_id,revision_id])?;
            }
            Record::WikiPage {
                id,
                slug,
                page_kind,
                title,
                summary,
                content_md,
                tags,
                source_event_ids,
                evidence_count,
                first_seen_at,
                last_seen_at,
                status,
                created_at,
                updated_at,
                source_url,
                area,
                based_on,
                content_type,
                human_edited_at,
                opinion,
                knowledge_epoch,
            } => {
                validate_id(id)?;
                ensure!(!slug.trim().is_empty(), "sync_snapshot_wiki_slug_invalid");
                ensure!(
                    matches!(status.as_str(), "active" | "archived"),
                    "sync_snapshot_wiki_status_invalid"
                );
                // 结构化文本列不接受任意字节混入数据集：必须是合法 JSON。
                ensure!(
                    serde_json::from_str::<serde_json::Value>(tags).is_ok(),
                    "sync_snapshot_wiki_json_invalid"
                );
                ensure!(
                    serde_json::from_str::<serde_json::Value>(source_event_ids).is_ok(),
                    "sync_snapshot_wiki_json_invalid"
                );
                // 足迹与页面行同事务重建：会话引用锚等待在快照恢复后不悬空。
                self.sync_mark_wiki_slug_seen(slug)?;
                // 同一条不变式：旧 v2 代的快照行可能带本机绝对路径，落地前折叠。
                let content_md = crate::storage::wiki_images::fold_tweet_image_refs(content_md);
                self.connection.execute("INSERT INTO wiki_pages(id,slug,kind,title,summary,content_md,tags,source_event_ids,evidence_count,first_seen_at,last_seen_at,status,created_at,updated_at,source_url,area,based_on,content_type,human_edited_at,opinion,knowledge_epoch) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20,?21)",params![id,slug,page_kind,title,summary,content_md,tags,source_event_ids,evidence_count,first_seen_at,last_seen_at,status,created_at,updated_at,source_url,area,based_on,content_type,human_edited_at,opinion,knowledge_epoch])?;
            }
            Record::WikiRevision {
                id,
                page_id,
                content_md,
                reason,
                source_event_id,
                created_at,
            } => {
                validate_id(id)?;
                if let Some(event) = source_event_id {
                    validate_id(event)?;
                }
                let page: Option<String> = self
                    .connection
                    .query_row("SELECT id FROM wiki_pages WHERE id=?1", [page_id], |r| {
                        r.get(0)
                    })
                    .optional()?;
                ensure!(
                    page.as_deref() == Some(page_id.as_str()),
                    "sync_snapshot_wiki_revision_orphaned"
                );
                self.connection.execute("INSERT INTO wiki_revisions(id,page_id,content_md,reason,source_event_id,created_at) VALUES(?1,?2,?3,?4,?5,?6)",params![id,page_id,content_md,reason,source_event_id,created_at])?;
            }
            Record::KnowledgeReview {
                fingerprint,
                page_id,
                resolution,
                created_at,
                issue_description,
            } => {
                validate_id(page_id)?;
                ensure!(
                    !fingerprint.trim().is_empty()
                        && fingerprint.len() <= 128
                        && !created_at.trim().is_empty()
                        && created_at.len() <= 64
                        && resolution.len() <= 64,
                    "sync_snapshot_knowledge_review_invalid"
                );
                if let Some(desc) = issue_description {
                    ensure!(desc.len() <= 4096, "sync_snapshot_knowledge_review_invalid");
                }
                // 快照流顺序保证 wiki_pages 行先于评审行（QUERIES 序）；孤儿
                // 指纹 fail-closed，绝不落悬空引用。
                let page: Option<String> = self
                    .connection
                    .query_row(
                        "SELECT id FROM wiki_pages WHERE id=?1",
                        [page_id.as_str()],
                        |r| r.get(0),
                    )
                    .optional()?;
                ensure!(
                    page.as_deref() == Some(page_id.as_str()),
                    "sync_snapshot_knowledge_review_orphaned"
                );
                self.connection.execute(
                    "INSERT OR IGNORE INTO knowledge_maintenance_reviews\
                     (fingerprint,page_id,resolution,created_at,issue_description)\
                     VALUES(?1,?2,?3,?4,?5)",
                    params![
                        fingerprint,
                        page_id,
                        resolution,
                        created_at,
                        issue_description
                    ],
                )?;
            }
            Record::KnowledgeReadingState {
                page_id,
                state,
                updated_at,
            } => {
                validate_id(page_id)?;
                ensure!(
                    matches!(
                        state.as_str(),
                        "unread" | "read" | "valuable" | "adopted" | "archived"
                    ),
                    "sync_snapshot_knowledge_state_invalid"
                );
                ensure!(
                    !updated_at.trim().is_empty() && updated_at.len() <= 64,
                    "sync_snapshot_knowledge_state_invalid"
                );
                let page: Option<String> = self
                    .connection
                    .query_row(
                        "SELECT id FROM wiki_pages WHERE id=?1",
                        [page_id.as_str()],
                        |r| r.get(0),
                    )
                    .optional()?;
                ensure!(
                    page.as_deref() == Some(page_id.as_str()),
                    "sync_snapshot_knowledge_state_orphaned"
                );
                self.connection.execute(
                    "INSERT OR IGNORE INTO knowledge_reading_states(page_id,state,updated_at)\
                     VALUES(?1,?2,?3)",
                    params![page_id, state, updated_at],
                )?;
            }
            Record::Todo {
                id,
                title,
                status,
                priority,
                due_at,
                reminder,
                related_event_id,
                related_wiki_slug,
                note,
                created_at,
                updated_at,
            } => {
                validate_id(id)?;
                ensure!(
                    matches!(status.as_str(), "open" | "done" | "archived")
                        && matches!(priority.as_str(), "high" | "normal" | "low"),
                    "sync_snapshot_todo_status_invalid"
                );
                self.connection.execute("INSERT INTO todos(id,title,status,priority,due_at,reminder,related_event_id,related_wiki_slug,note,created_at,updated_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)",params![id,title,status,priority,due_at,reminder,related_event_id,related_wiki_slug,note,created_at,updated_at])?;
            }
            Record::Goal {
                id,
                content,
                phase,
                status,
                created_at,
                updated_at,
                superseded_at,
            } => {
                validate_id(id)?;
                ensure!(
                    matches!(phase.as_str(), "near" | "mid" | "long")
                        && matches!(status.as_str(), "active" | "superseded"),
                    "sync_snapshot_goal_status_invalid"
                );
                // 源端受同样的活跃上限不变式约束，快照里不可能出现违限行；
                // 违限行到此必炸（触发器 fail-closed），不旁路。
                self.connection.execute("INSERT INTO goals(id,content,phase,status,created_at,updated_at,superseded_at) VALUES(?1,?2,?3,?4,?5,?6,?7)",params![id,content,phase,status,created_at,updated_at,superseded_at])?;
            }
            Record::Rule {
                id,
                content,
                status,
                created_at,
                updated_at,
                conversation_id,
                review_version,
            } => {
                validate_id(id)?;
                ensure!(
                    matches!(status.as_str(), "active" | "pending"),
                    "sync_snapshot_rule_status_invalid"
                );
                self.connection.execute("INSERT INTO rules(id,content,status,conversation_id,created_at,updated_at,review_version) VALUES(?1,?2,?3,?4,?5,?6,?7)",params![id,content,status,conversation_id,created_at,updated_at,review_version])?;
            }
            Record::WikiAsset {
                rel,
                sha256,
                mime,
                size,
            } => {
                // 台账行：无字节，仅记文件名片；hash 形态在这里只能验格式，
                // 与文件字节的一致性由 WikiFile 记录负责。
                ensure!(
                    crate::storage::wiki_images::rel_is_valid(rel)
                        && *size >= 0
                        && valid_hash(sha256)
                        && matches!(
                            mime.as_str(),
                            "image/jpeg" | "image/png" | "image/webp" | "image/gif"
                        ),
                    "sync_snapshot_wiki_asset_invalid"
                );
                self.connection.execute(
                    "INSERT OR REPLACE INTO wiki_assets(rel,sha256,mime,size) VALUES(?1,?2,?3,?4)",
                    params![rel, sha256, mime, size],
                )?;
            }
            Record::WikiFile {
                rel,
                sha256,
                mime,
                size,
                bytes_hex,
            } => {
                // 文件字节：直写接收端数据目录（不落库）。四道闸门同前：
                // rel 形态 / mime 白名单 / hex 长度 / sha256 与字节一致。
                ensure!(
                    crate::storage::wiki_images::rel_is_valid(rel)
                        && *size >= 0
                        && bytes_hex.len() == (*size as usize) * 2
                        && matches!(
                            mime.as_str(),
                            "image/jpeg" | "image/png" | "image/webp" | "image/gif"
                        ),
                    "sync_snapshot_wiki_file_invalid"
                );
                let bytes = hex_to_vec(bytes_hex)?;
                ensure!(
                    bytes.len() == *size as usize,
                    "sync_snapshot_wiki_file_invalid"
                );
                let digest = hex(ring::digest::digest(&SHA256, &bytes).as_ref());
                ensure!(&digest == sha256, "sync_snapshot_wiki_file_hash_mismatch");
                let data_dir = self
                    .path
                    .parent()
                    .ok_or_else(|| anyhow::anyhow!("sync_snapshot_data_dir_missing"))?;
                let target = data_dir.join(rel);
                if let Some(dir) = target.parent() {
                    std::fs::create_dir_all(dir)
                        .map_err(|_| anyhow::anyhow!("sync_snapshot_wiki_file_write_failed"))?;
                }
                std::fs::write(&target, &bytes)
                    .map_err(|_| anyhow::anyhow!("sync_snapshot_wiki_file_write_failed"))?;
                // 落地即记账：接收端的台账与文件同步就位（幂等）。
                self.connection.execute(
                    "INSERT OR REPLACE INTO wiki_assets(rel,sha256,mime,size) VALUES(?1,?2,?3,?4)",
                    params![rel, sha256, mime, size],
                )?;
            }
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
        }
        Ok(())
    }
}

/// MCP 快照行的 args_json/env_json 有效性闸门（数组或对象，非法即拒）。
pub(crate) fn validate_json_arg(text: &str, code: &'static str) -> Result<()> {
    let value: serde_json::Value = serde_json::from_str(text).map_err(|_| anyhow::anyhow!(code))?;
    ensure!(value.is_array() || value.is_object(), code);
    Ok(())
}

pub(crate) fn hex_to_vec(hex: &str) -> Result<Vec<u8>> {
    Ok((0..hex.len() / 2)
        .map(|i| u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16))
        .collect::<std::result::Result<Vec<u8>, _>>()
        .map_err(|_| anyhow::anyhow!("sync_snapshot_wiki_asset_invalid"))?)
}

/// head/known_revision 记录的种类必须落在随快照携带的领域表上。
pub(crate) fn domain_table(kind: &str) -> Result<&'static str> {
    Ok(match kind {
        "conversation" => "conversations",
        "wiki_page" => "wiki_pages",
        "todo" => "todos",
        "goal" => "goals",
        "rule" => "rules",
        _ => return Err(anyhow::anyhow!("sync_snapshot_object_kind_invalid")),
    })
}

#[derive(Default)]
pub(crate) struct FrameDecoder {
    pub(crate) pending: Vec<u8>,
}
impl FrameDecoder {
    pub(crate) fn push(
        &mut self,
        bytes: &[u8],
        mut apply: impl FnMut(Record) -> Result<()>,
    ) -> Result<()> {
        self.pending.extend_from_slice(bytes);
        let mut consumed = 0;
        loop {
            let remaining = &self.pending[consumed..];
            if remaining.len() < 4 {
                break;
            }
            let size = u32::from_be_bytes(remaining[..4].try_into().unwrap()) as usize;
            ensure!(
                size > 0 && size <= MAX_RECORD_BYTES,
                "sync_snapshot_record_size_invalid"
            );
            if remaining.len() < size + 4 {
                break;
            }
            let record: Record = serde_json::from_slice(&remaining[4..size + 4])
                .map_err(|_| anyhow::anyhow!("sync_snapshot_record_invalid"))?;
            apply(record)?;
            consumed += size + 4;
        }
        if consumed > 0 {
            self.pending.drain(..consumed);
        }
        ensure!(
            self.pending.len() <= MAX_RECORD_BYTES + 4,
            "sync_snapshot_record_size_invalid"
        );
        Ok(())
    }
}
