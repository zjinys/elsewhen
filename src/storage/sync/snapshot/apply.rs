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
                essence,
            } => {
                validate_id(id)?;
                ensure!(
                    matches!(status.as_str(), "active" | "pending"),
                    "sync_snapshot_rule_status_invalid"
                );
                self.connection.execute("INSERT INTO rules(id,content,status,conversation_id,essence,created_at,updated_at,review_version) VALUES(?1,?2,?3,?4,?5,?6,?7,?8)",params![id,content,status,conversation_id,essence,created_at,updated_at,review_version])?;
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
            other_record => {
                self.apply_snapshot_record_rest(
                    other_record,
                    snapshot_id,
                    manifest,
                    provider_seen,
                )?;
            }
        }
        Ok(())
    }
}
