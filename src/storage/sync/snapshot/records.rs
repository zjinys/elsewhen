//! 快照载荷 schema：版本常量、记录枚举与导出查询。

use super::*;

/// 快照载荷 schema 代际。v1 缺 wiki/待办/目标/规则与 head 种类（§29 的缺口），
/// 与 v2 记录格式互不兼容：接收端只认当前代（跨代 = 显式隔离，不猜测）。
/// 快照目录段只携带登记过的目录（fail-closed 的目录版覆盖门禁）：
/// 数据目录里其它任何内容（本地索引、私钥、缓存）永远不会被枚举外发。
/// rel 合法性（rel_is_valid）已把路径钉死在 tweet_images/<digits>/ 下。
pub(crate) const SNAPSHOT_CARRIED_DIRS: &[&str] = &["tweet_images"];

/// v3：新增 Record::WikiAsset（图片字节随快照携带）。新增 serde 变体后旧端
/// 标签分发失败会整份拒收，所以跨 v2/v3 隔离是真实的、必须显式 bump——
/// 与「wire 枚举加变体不 bump」不同（那边是 append-only 可被旧端忽略）。
/// v5：新增 Record::McpServer（迁移 48 平台标签方案的同步面）。
/// v6：新增 Record::KnowledgeSource（摘录孵化闭环）：负知识（identity +
/// opinion + reject_reason）跨设备可见——「你否定过这条来源」的提示
/// 必须在新设备上仍然成立。
/// 同步语义：定义随人走，激活看平台——stdio 的 platform 变体不再导致
/// 整表排除；行上不含 headers（凭据不离本机）与 trusted（信任是本机
/// 事实，apply 强制置 0 回到逐次审批）。新增 serde 变体后旧端标签分发
/// 失败会整份拒收，跨 v4/v5 必须显式 bump——与 wire 枚举不同（那边
/// append-only 可被旧端忽略）。
pub(crate) const SNAPSHOT_VERSION: u32 = 6;

/// 随快照携带的表。QUERIES 必须真实覆盖这里的每一张（覆盖性由门禁测试
/// `snapshot_carries_every_table_or_explicitly_excludes_it` 兜住：新增表不登记
/// 会直接红，再也不会静默断档）。
pub(crate) const SNAPSHOT_CARRIED_TABLES: &[&str] = &[
    "events",
    "wiki_pages",
    "wiki_revisions",
    "knowledge_maintenance_reviews",
    "knowledge_reading_states",
    "conversations",
    "messages",
    "message_response_links",
    "input_records",
    "event_analyses",
    "event_recordability_decisions",
    "todos",
    "goals",
    "rules",
    "sync_object_heads",
    "sync_known_revisions",
    "sync_members",
    "sync_message_order",
    "sync_operations",
    "sync_checkpoint_operations",
    "sync_conflicts",
    "sync_checkpoint_providers",
    "sync_provider_versions",
    "sync_wire_outbox",
    "ai_provider_configs",
    "wiki_assets",
    // 技能跨设备分发（migration 41/42）：技能文件字节随整库快照走，
    // 接收端 reconcile 用它落盘；墓碑拦「已卸载技能」被旧分发复活。
    "skill_files",
    "skill_tombstones",
    // MCP 定义（迁移 48 平台标签方案）：随快照携带，apply 不含 headers 与
    // trusted（见 Record::McpServer 文档）。
    "mcp_servers",
    // 负知识（摘录孵化闭环）：identity/opinion/reject_reason 是「记住我
    // 否定过」的最小面，快照表很小；判定 wiki_pages.opinion 本就同步。
    "knowledge_sources",
];

pub(crate) const SNAPSHOT_EXCLUDED_TABLES: &[(&str, &str)] = &[
    // 设备身份与签名私钥：快照是给新设备的，身份由接收端自己初始化。
    ("sync_local_vault", "local-identity-and-keys"),
    ("sync_replica", "local-identity-and-keys"),
    // Provider key 材料不离本机（key 不回收披露）；配置经 provider bundle 走。
    ("provider_edit_tokens", "local-provider-secrets-and-state"),
    ("provider_key_policy", "local-provider-secrets-and-state"),
    ("provider_key_secrets", "local-provider-secrets-and-state"),
    // 成员授权与密钥材料：经 enrollment/signed roster 渠道走，不夹带。
    ("sync_authority", "enrollment-channel"),
    ("sync_authority_versions", "enrollment-channel"),
    ("sync_member_keys", "enrollment-channel"),
    ("sync_member_state", "enrollment-channel"),
    // 发布时由 manifest.frontiers 覆盖校验重建。
    ("sync_frontiers", "rebuilt-from-manifest-at-publish"),
    // GC 只认本设备的应用层水位，不跨设备继承。
    ("sync_gc_state", "per-device-gc-watermark"),
    // 本机同步收发/传输/迁移残留状态：不属于数据集内容。
    // MCP 服务器定义随快照走（迁移 48）：stdio 命令是平台绑定事实，靠
    // platform 标签分发而非整表不同步（远程 http 行标 all 即可全端可用）。
    // 排除面由 apply 闸门兑现：不含 headers（凭据）与 trusted（信任是
    // 本机事实，落表强制置 0）。若未来要回到 per-device，必须连同
    // Record::McpServer 与门禁测试一起撤。
    // ("mcp_servers", "per-device-execution-environment"),
    ("ai_provider_sync_order", "rebuilt-by-provider-bundle-apply"),
    // 运行时 failover 落点（迁移 55）：本机会话事实，永不入同步域——
    // 入域则 bundle 明文随每次 failover 漂移（对端已 ack 信封无限 re-seal）。
    ("ai_provider_runtime_active", "per-device-runtime-failover"),
    ("analysis_jobs", "per-device-sync-state"),
    ("sync_operation_fragment_outbox", "per-device-sync-state"),
    ("sync_operation_fragment_parts", "per-device-sync-state"),
    ("sync_operation_fragments", "per-device-sync-state"),
    ("sync_operations_new", "per-device-sync-state"),
    ("sync_outbox", "per-device-sync-state"),
    ("sync_processor_handoffs", "per-device-sync-state"),
    ("sync_processor_state", "per-device-sync-state"),
    ("sync_provider_dirty", "per-device-sync-state"),
    ("sync_provider_state", "per-device-sync-state"),
    ("sync_provider_versions_new", "per-device-sync-state"),
    ("sync_receipts", "per-device-sync-state"),
    ("sync_relay_cursors", "per-device-sync-state"),
    ("sync_resnapshot_requests", "per-device-sync-state"),
    ("sync_snapshot_chunks", "per-device-sync-state"),
    ("sync_snapshot_export_wires", "per-device-sync-state"),
    ("sync_snapshot_exports", "per-device-sync-state"),
    ("sync_snapshot_target", "per-device-sync-state"),
    ("sync_snapshots", "per-device-sync-state"),
    ("sync_wire_receipts", "per-device-sync-state"),
    // 应用记录时用 sync_mark_wiki_slug_seen 重建。
    ("sync_wiki_slug_seen", "derived-on-apply"),
    // 规则触发命中挂本地事件（events 不同步，提醒在哪台设备响是设备侧行为）；
    // 偏好开关同理。规则触发词字段随 rules 行携带。
    ("rule_hits", "local-event-reminders"),
    // 对话影子校验命中（迁移 54）：预筛/判定记录挂本地会话与消息，
    // messages 本身不同步，命中记录同样留在产生它的设备上（同 rule_hits）。
    ("chat_rule_hits", "local-event-reminders"),
    // 定时提醒的本机触发去重（migration 50）：「提醒在哪台设备响是设备侧行为」，
    // 多设备各自弹各自的，不同步触发记录（与 rule_hits 同纪律）。
    ("todo_reminder_fires", "local-event-reminders"),
    ("rule_trigger_settings", "local-event-reminders"),
    // 情感回访偏好与记账：本机提醒体验状态，不属于跨设备数据集。
    ("companion_settings", "local-event-reminders"),
    // guidance 采纳反馈（遗留收口切片 B）：只记不外推的本机度量，
    // analysis 随快照走但反馈不跨设备复制（与 rule_hits feedback 同纪律）。
    ("guidance_feedback", "local-event-reminders"),
    // 本机应用配置（含 web_search 凭据），不是数据集内容。
    ("app_meta", "local-app-config"),
    ("network_settings", "local-app-config"),
    ("schema_migrations", "local-app-config"),
    ("web_search_configs", "local-app-config"),
    // 聊天执行/排程本地状态。
    ("chat_input_effects", "local-chat-execution-state"),
    ("chat_input_queue", "local-chat-execution-state"),
    ("chat_page_tasks", "local-chat-execution-state"),
    ("chat_proposal_decisions", "local-chat-execution-state"),
    ("chat_queue_dispatch", "local-chat-execution-state"),
    ("chat_run_events", "local-chat-execution-state"),
    ("chat_run_requests", "local-chat-execution-state"),
    ("chat_runs", "local-chat-execution-state"),
    ("chat_summary_nodes", "local-chat-execution-state"),
    // 翻篇标记引用本机摘要节点（同上域）；跨端以消息为准按需重建，
    // Node 缺失时装配退回原文——配套性检查（P4）前不同步。
    ("conversation_page_marks", "local-chat-execution-state"),
    ("pending_actions", "local-chat-execution-state"),
    ("suggestion_feedback", "local-chat-execution-state"),
    // knowledge 管线本地产物，不在同步数据集（不产生 op）。用户语义状态
    // （maintenance_reviews 评审行 / reading_states 阅读状态）已入快照
    // carried 集——双设备 issue 关闭与已读/收藏互见；其余管道过程产物
    // （jobs/runs/snapshots/metrics/topic_plans…）每设备本地重算，永久排除。
    ("knowledge_artifact_adoptions", "local-knowledge-pipeline"),
    // 周期报告与偏差检测（R3）：管道产物，本机回顾状态不跨设备复制
    // （report/stats 本地可重算，偏差与已读是本机体验状态）。
    ("deviation_reports", "local-knowledge-pipeline"),
    ("knowledge_artifact_versions", "local-knowledge-pipeline"),
    ("knowledge_background_runs", "local-knowledge-pipeline"),
    ("knowledge_dependencies", "local-knowledge-pipeline"),
    ("knowledge_digest_jobs", "local-knowledge-pipeline"),
    ("knowledge_digest_runs", "local-knowledge-pipeline"),
    ("knowledge_generations", "local-knowledge-pipeline"),
    ("knowledge_metadata", "local-knowledge-pipeline"),
    ("knowledge_page_sources", "local-knowledge-pipeline"),
    ("knowledge_proposals", "local-knowledge-pipeline"),
    ("knowledge_refresh_jobs", "local-knowledge-pipeline"),
    ("knowledge_reconsolidation_jobs", "local-knowledge-pipeline"),
    ("knowledge_relations", "local-knowledge-pipeline"),
    ("knowledge_review_decisions", "local-knowledge-pipeline"),
    ("knowledge_semantic_issues", "local-knowledge-pipeline"),
    ("knowledge_snapshot_metrics", "local-knowledge-pipeline"),
    ("knowledge_snapshots", "local-knowledge-pipeline"),
    ("knowledge_source_pages", "local-knowledge-pipeline"),
    ("knowledge_source_readings", "local-knowledge-pipeline"),
    ("knowledge_topic_plan_pages", "local-knowledge-pipeline"),
    ("knowledge_topic_plans", "local-knowledge-pipeline"),
    ("knowledge_topic_replacements", "local-knowledge-pipeline"),
    ("knowledge_usage", "local-knowledge-pipeline"),
    // 实体/洞察等本地 AI 产物，可由事件重算。
    ("daily_review_sources", "local-ai-artifacts"),
    ("daily_reviews", "local-ai-artifacts"),
    ("entity_aliases", "local-ai-artifacts"),
    ("entity_facts", "local-ai-artifacts"),
    ("entity_merge_snapshots", "local-ai-artifacts"),
    ("entity_merges", "local-ai-artifacts"),
    ("insights", "local-ai-artifacts"),
    ("relations", "local-ai-artifacts"),
    ("token_usage", "local-ai-artifacts"),
    ("wiki_log", "local-ai-artifacts"),
];

pub(crate) const CHUNK_BYTES: usize = 256 * 1024;
pub(crate) const MAX_RECORD_BYTES: usize = 20 * 1024 * 1024;
pub(crate) const MAX_SNAPSHOT_BYTES: usize = 256 * 1024 * 1024;
pub(crate) const MAX_CHUNKS: usize = MAX_SNAPSHOT_BYTES / CHUNK_BYTES + 1;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SnapshotChunk {
    pub(crate) id: String,
    pub(crate) index: usize,
    pub(crate) data: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SnapshotManifest {
    pub version: u32,
    pub id: String,
    pub dataset_id: String,
    pub source_device_id: String,
    pub recipient_device_id: String,
    pub processor_device_id: String,
    pub key_version: u64,
    pub chunks: usize,
    pub bytes: usize,
    pub records: usize,
    pub digest: String,
    pub frontiers: BTreeMap<String, i64>,
    pub logical_clock: i64,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct MessageOrder {
    pub(crate) origin_stream_id: String,
    pub(crate) origin_sequence: i64,
    pub(crate) logical_clock: i64,
    pub(crate) input_message_id: Option<String>,
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum Record {
    Event {
        value: EventRecord,
        status: String,
        processed_at: Option<String>,
        updated_at: String,
    },
    Conversation {
        value: ConversationRecord,
    },
    Message {
        value: MessageRecord,
        order: Option<MessageOrder>,
    },
    Input {
        value: InputRecord,
    },
    Analysis {
        id: String,
        event_id: String,
        prompt_version: String,
        result_json: String,
        created_at: String,
    },
    Classification {
        id: String,
        event_id: String,
        recordable: bool,
        event_kind: String,
        reason: String,
        created_at: String,
    },
    KnowledgeReview {
        fingerprint: String,
        page_id: String,
        resolution: String,
        created_at: String,
        issue_description: Option<String>,
    },
    KnowledgeReadingState {
        page_id: String,
        state: String,
        updated_at: String,
    },
    Head {
        object_kind: String,
        object_id: String,
        revision_id: String,
        deleted: bool,
    },
    KnownRevision {
        object_kind: String,
        object_id: String,
        revision_id: String,
    },
    Member {
        device_id: String,
        stream_id: String,
        active: bool,
    },
    Checkpoint {
        op_id: String,
        origin_device_id: String,
        origin_stream_id: String,
        origin_sequence: i64,
        payload_hash: String,
    },
    Operation {
        value: SyncOperation,
        state: String,
        payload_hash: String,
    },
    Conflict {
        op_id: String,
        object_kind: String,
        object_id: String,
        current_revision: Option<String>,
        reason: String,
    },
    ProviderCheckpoint {
        revision_id: String,
        content_hash: String,
    },
    Provider {
        value: ProviderBundle,
    },
    WikiPage {
        id: String,
        slug: String,
        page_kind: String,
        title: String,
        summary: String,
        content_md: String,
        tags: String,
        source_event_ids: String,
        evidence_count: i64,
        first_seen_at: String,
        last_seen_at: String,
        status: String,
        created_at: String,
        updated_at: String,
        source_url: Option<String>,
        area: Option<String>,
        based_on: Option<String>,
        content_type: Option<String>,
        human_edited_at: Option<String>,
        opinion: Option<String>,
        knowledge_epoch: i64,
    },
    WikiRevision {
        id: String,
        page_id: String,
        content_md: String,
        reason: String,
        source_event_id: Option<String>,
        created_at: String,
    },
    Todo {
        id: String,
        title: String,
        status: String,
        priority: String,
        due_at: Option<String>,
        /// SQLite json_object 对本列输出 0/1 整数（不是 true/false），镜像类型用 i64。
        reminder: i64,
        related_event_id: Option<String>,
        related_wiki_slug: Option<String>,
        note: Option<String>,
        created_at: String,
        updated_at: String,
    },
    Goal {
        id: String,
        content: String,
        phase: String,
        status: String,
        created_at: String,
        updated_at: String,
        superseded_at: Option<String>,
    },
    Rule {
        id: String,
        content: String,
        status: String,
        created_at: String,
        updated_at: String,
        conversation_id: Option<String>,
        review_version: i64,
        /// 语义内核（迁移 56 新增；旧快照行可能缺失）
        #[serde(default)]
        essence: Option<String>,
    },
    /// 台账行（文件名片，字节不落库，§31b）：随 QUERIES 常规携带。
    WikiAsset {
        /// 相对数据目录的确定性路径（tweet_images/<tweet_id>/img_N.ext）。
        rel: String,
        sha256: String,
        mime: String,
        size: i64,
    },
    /// 文件字节段（导出器从发送端磁盘读出，接收端直接落盘）。
    WikiFile {
        rel: String,
        sha256: String,
        mime: String,
        size: i64,
        /// 字节的 hex 编码（2×size）；apply 端解码并复核长度与哈希。
        bytes_hex: String,
    },
    /// 技能分发（migration 41）：技能目录内单文件字节，skill_id×rel 主键。
    SkillFile {
        skill_id: String,
        rel: String,
        sha256: String,
        size: i64,
        updated_at: String,
        /// 字节的 hex 编码（2×size）；apply 端解码并复核长度与哈希。
        bytes_hex: String,
    },
    /// 技能墓碑（migration 41）：拦截已卸载技能被旧分发复活。
    SkillTombstone {
        skill_id: String,
        updated_at: String,
    },
    /// MCP 服务器定义（迁移 48 平台标签方案）：定义随人走，激活看平台。
    /// 不含 headers_json（凭据不入快照，与 provider key 同一先例）与
    /// trusted（信任是本机事实，apply 强制置 0）。platform 值域
    /// all/linux/macos/windows；接收端 (name,platform) 已有行则本机获胜
    /// （INSERT OR IGNORE 对 id 与 (name,platform) 两个唯一目标都静默跳过）。
    McpServer {
        id: String,
        name: String,
        platform: String,
        transport: String,
        command: String,
        args_json: String,
        env_json: String,
        url: String,
        enabled: bool,
        created_at: String,
        updated_at: String,
    },
    /// 负知识台账（摘录孵化闭环，migration 50）：identity 同源识别 +
    /// 否定原因。kind/locator 仅作展示面；接收端 upsert 保守（已有行不
    /// 覆盖 opinion——台账是导入事实，本机导入时间线更可信）。
    KnowledgeSource {
        id: String,
        identity: String,
        /// tag 与字段同名冲突，沿用 WikiPage 变体的 page_kind 先例
        source_kind: String,
        locator: Option<String>,
        opinion: Option<String>,
        reject_reason: Option<String>,
        created_at: String,
    },
}

// rowid is only a cursor inside this one pinned source view, never a replicated
// identity or display-order key. Parent messages exist before their children in
// supported write paths; the importer refuses forward/cyclic parent references.
pub(crate) const QUERIES: &[&str] = &[
    "SELECT rowid,json_object('kind','event','value',json_object('id',id,'occurred_at',occurred_at,'recorded_at',recorded_at,'raw_text',raw_text,'source',source,'created_at',created_at),'status',status,'processed_at',processed_at,'updated_at',updated_at) FROM events WHERE rowid>?1 ORDER BY rowid LIMIT 1",
    "SELECT rowid,json_object('kind','wiki_page','id',id,'slug',slug,'page_kind',kind,'title',title,'summary',summary,'content_md',content_md,'tags',tags,'source_event_ids',source_event_ids,'evidence_count',evidence_count,'first_seen_at',first_seen_at,'last_seen_at',last_seen_at,'status',status,'created_at',created_at,'updated_at',updated_at,'source_url',source_url,'area',area,'based_on',based_on,'content_type',content_type,'human_edited_at',human_edited_at,'opinion',opinion,'knowledge_epoch',knowledge_epoch) FROM wiki_pages WHERE rowid>?1 ORDER BY rowid LIMIT 1",
    "SELECT rowid,json_object('kind','wiki_revision','id',id,'page_id',page_id,'content_md',content_md,'reason',reason,'source_event_id',source_event_id,'created_at',created_at) FROM wiki_revisions WHERE rowid>?1 ORDER BY rowid LIMIT 1",
    "SELECT rowid,json_object('kind','knowledge_review','fingerprint',fingerprint,'page_id',page_id,'resolution',resolution,'created_at',created_at,'issue_description',issue_description) FROM knowledge_maintenance_reviews WHERE rowid>?1 ORDER BY rowid LIMIT 1",
    "SELECT rowid,json_object('kind','knowledge_reading_state','page_id',page_id,'state',state,'updated_at',updated_at) FROM knowledge_reading_states WHERE rowid>?1 ORDER BY rowid LIMIT 1",
    "SELECT rowid,json_object('kind','wiki_asset','rel',rel,'sha256',sha256,'mime',mime,'size',size) FROM wiki_assets WHERE rowid>?1 ORDER BY rowid LIMIT 1",
    "SELECT rowid,json_object('kind','conversation','value',json_object('id',id,'title',title,'tag',tag,'archived',json(CASE WHEN archived=1 THEN 'true' ELSE 'false' END),'wiki_page_slug',wiki_page_slug,'assistant_mode',assistant_mode,'created_at',created_at,'updated_at',updated_at)) FROM conversations WHERE rowid>?1 ORDER BY rowid LIMIT 1",
    "SELECT m.rowid,json_object('kind','message','value',json_object('id',m.id,'conversation_id',m.conversation_id,'parent_message_id',m.parent_message_id,'role',m.role,'content',m.content,'created_at',m.created_at,'input_message_id',(SELECT input_message_id FROM message_response_links WHERE message_id=m.id)),'order',CASE WHEN s.message_id IS NULL THEN NULL ELSE json_object('origin_stream_id',s.origin_stream_id,'origin_sequence',s.origin_sequence,'logical_clock',s.logical_clock,'input_message_id',s.input_message_id) END) FROM messages m JOIN conversations c ON c.id=m.conversation_id LEFT JOIN sync_message_order s ON s.message_id=m.id WHERE m.rowid>?1 ORDER BY m.rowid LIMIT 1",
    "SELECT i.rowid,json_object('kind','input','value',json_object('id',i.id,'raw_text',i.raw_text,'source',i.source,'event_id',i.event_id,'message_id',i.message_id,'created_at',i.created_at)) FROM input_records i WHERE i.rowid>?1 AND i.event_id IS NOT NULL AND (i.message_id IS NULL OR EXISTS(SELECT 1 FROM messages m WHERE m.id=i.message_id)) ORDER BY i.rowid LIMIT 1",
    "SELECT rowid,json_object('kind','analysis','id',id,'event_id',event_id,'prompt_version',prompt_version,'result_json',result_json,'created_at',created_at) FROM event_analyses WHERE rowid>?1 ORDER BY rowid LIMIT 1",
    "SELECT rowid,json_object('kind','classification','id',id,'event_id',event_id,'recordable',json(CASE WHEN recordable=1 THEN 'true' ELSE 'false' END),'event_kind',kind,'reason',reason,'created_at',created_at) FROM event_recordability_decisions WHERE rowid>?1 ORDER BY rowid LIMIT 1",
    "SELECT rowid,json_object('kind','todo','id',id,'title',title,'status',status,'priority',priority,'due_at',due_at,'reminder',(reminder!=0),'related_event_id',related_event_id,'related_wiki_slug',related_wiki_slug,'note',note,'created_at',created_at,'updated_at',updated_at) FROM todos WHERE rowid>?1 ORDER BY rowid LIMIT 1",
    "SELECT rowid,json_object('kind','goal','id',id,'content',content,'phase',phase,'status',status,'created_at',created_at,'updated_at',updated_at,'superseded_at',superseded_at) FROM goals WHERE rowid>?1 ORDER BY rowid LIMIT 1",
    "SELECT rowid,json_object('kind','rule','id',id,'content',content,'status',status,'created_at',created_at,'updated_at',updated_at,'conversation_id',conversation_id,'review_version',review_version,'essence',essence) FROM rules WHERE rowid>?1 ORDER BY rowid LIMIT 1",
    "SELECT h.rowid,json_object('kind','head','object_kind',h.object_kind,'object_id',h.object_id,'revision_id',h.revision_id,'deleted',json(CASE WHEN h.deleted=1 THEN 'true' ELSE 'false' END)) FROM sync_object_heads h WHERE h.rowid>?1 ORDER BY h.rowid LIMIT 1",
    "SELECT r.rowid,json_object('kind','known_revision','object_kind',r.object_kind,'object_id',r.object_id,'revision_id',r.revision_id) FROM sync_known_revisions r JOIN sync_object_heads h ON h.object_kind=r.object_kind AND h.object_id=r.object_id WHERE r.rowid>?1 ORDER BY r.rowid LIMIT 1",
    "SELECT rowid,json_object('kind','member','device_id',device_id,'stream_id',stream_id,'active',json(CASE WHEN active=1 THEN 'true' ELSE 'false' END)) FROM sync_members WHERE rowid>?1 ORDER BY rowid LIMIT 1",
    "SELECT rowid,json_object('kind','checkpoint','op_id',op_id,'origin_device_id',origin_device_id,'origin_stream_id',origin_stream_id,'origin_sequence',origin_sequence,'payload_hash',payload_hash) FROM sync_operations WHERE rowid>?1 AND state='applied' ORDER BY rowid LIMIT 1",
    "SELECT rowid,json_object('kind','checkpoint','op_id',op_id,'origin_device_id',origin_device_id,'origin_stream_id',origin_stream_id,'origin_sequence',origin_sequence,'payload_hash',payload_hash) FROM sync_checkpoint_operations WHERE rowid>?1 ORDER BY rowid LIMIT 1",
    "SELECT rowid,json_object('kind','operation','value',json(payload),'state',state,'payload_hash',payload_hash) FROM sync_operations WHERE rowid>?1 AND state='conflict' ORDER BY rowid LIMIT 1",
    "SELECT rowid,json_object('kind','conflict','op_id',op_id,'object_kind',object_kind,'object_id',object_id,'current_revision',current_revision,'reason',reason) FROM sync_conflicts WHERE rowid>?1 ORDER BY rowid LIMIT 1",
    "SELECT rowid,json_object('kind','provider_checkpoint','revision_id',revision_id,'content_hash',content_hash) FROM sync_checkpoint_providers WHERE rowid>?1 ORDER BY rowid LIMIT 1",
    "SELECT rowid,json_object('kind','provider_checkpoint','revision_id',revision_id,'content_hash',content_hash) FROM sync_provider_versions WHERE rowid>?1 AND state='applied' ORDER BY rowid LIMIT 1",
    "SELECT rowid,json_object('kind','provider_checkpoint','revision_id',substr(message_id,10),'content_hash',payload_hash) FROM sync_wire_outbox WHERE rowid>?1 AND message_id LIKE 'provider:%' ORDER BY rowid LIMIT 1",
    "SELECT rowid,json_object('kind','skill_file','skill_id',skill_id,'rel',rel,'sha256',sha256,'size',size,'updated_at',updated_at,'bytes_hex',hex(bytes)) FROM skill_files WHERE rowid>?1 ORDER BY rowid LIMIT 1",
    "SELECT rowid,json_object('kind','skill_tombstone','skill_id',skill_id,'updated_at',updated_at) FROM skill_tombstones WHERE rowid>?1 ORDER BY rowid LIMIT 1",
    // MCP 定义（迁移 48）：headers_json 不导出（凭据不离本机）；trusted
    // 同理不入快照——apply 时强制置 0。
    "SELECT rowid,json_object('kind','mcp_server','id',id,'name',name,'platform',platform,'transport',transport,'command',command,'args_json',args_json,'env_json',env_json,'url',url,'enabled',json(CASE WHEN enabled=1 THEN 'true' ELSE 'false' END),'created_at',created_at,'updated_at',updated_at) FROM mcp_servers WHERE rowid>?1 ORDER BY rowid LIMIT 1",
    // 负知识（摘录孵化闭环）：reject_reason 跨设备（接收端提示「你否定过
    // 这条来源」）；opinion 与 wiki_pages 双写外的台账面。
    "SELECT rowid,json_object('kind','knowledge_source','id',id,'identity',identity,'source_kind',kind,'locator',locator,'opinion',opinion,'reject_reason',reject_reason,'created_at',created_at) FROM knowledge_sources WHERE rowid>?1 ORDER BY rowid LIMIT 1",
];

pub(crate) struct SnapshotExporter {
    pub(crate) view: Store,
    pub(crate) manifest: SnapshotManifest,
    pub(crate) table: usize,
    pub(crate) cursor: i64,
    pub(crate) frame: Vec<u8>,
    pub(crate) frame_offset: usize,
    pub(crate) hash: DigestContext,
    pub(crate) finished: bool,
    /// 目录段：台账 rel 的待发队列（进入目录段时惰性构建）。
    pub(crate) files: std::vec::IntoIter<String>,
    pub(crate) data_dir: Option<std::path::PathBuf>,
}
