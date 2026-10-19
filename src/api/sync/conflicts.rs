//! Wiki 页与对象冲突：冲突 DTO 装配、列表面与裁决入口。

use super::*;

fn wiki_page_conflict_dto(
    review: WikiPageConflictReview,
    sender_device_name: Option<String>,
) -> WikiPageConflictDto {
    WikiPageConflictDto {
        local: review.local.map(|l| WikiPageConflictSideDto {
            slug: l.slug,
            title: l.title,
            content_md_len: l.content_md_len,
            updated_at: l.updated_at,
        }),
        op_id: review.op_id,
        object_id: review.object_id,
        reason: review.reason,
        sender_device_id: review.sender_device_id,
        sender_device_name,
        incoming_slug: review.incoming.slug,
        incoming_title: review.incoming.title,
        incoming_deleted: review.incoming.deleted,
        incoming_renamed_from: review.incoming.renamed_from,
        incoming_summary: review.incoming.summary,
        incoming_content_md_len: review.incoming.content_md_len,
        incoming_updated_at: review.incoming.updated_at,
        local_head_deleted: review.local_head_deleted,
    }
}

/// 列出待审阅的 wiki 页同步冲突（按流内序）。UI 必须保存 `local.updated_at`
///（行已物理删除为 null）并在裁决时原样回传；`localHeadDeleted` 提示删除态。
pub fn list_wiki_page_conflicts() -> Result<Vec<WikiPageConflictDto>> {
    let config = AppConfig::load()?;
    let store = Store::open(&config.database_path)?;
    let reviews = store.list_wiki_page_conflicts()?;
    let roster = store.current_signed_roster()?;
    Ok(reviews
        .into_iter()
        .map(|r| {
            let sender_device_name = roster.as_ref().and_then(|roster| {
                roster
                    .roster
                    .members
                    .iter()
                    .find(|m| m.device.device_id == r.sender_device_id)
                    .map(|m| m.device.name.clone())
            });
            wiki_page_conflict_dto(r, sender_device_name)
        })
        .collect())
}

/// 显式裁决一条 wiki 页冲突。`expected_local_updated_at` 原样回传审阅时展示
/// 的本端行 updated_at；不匹配失败关闭。`accept_remote=false` = 保留本端现状
/// （删除态则物理删除，编辑态则重新捕获广播）。
pub fn resolve_wiki_page_conflict(
    op_id: String,
    expected_local_updated_at: Option<String>,
    accept_remote: bool,
) -> Result<()> {
    let config = AppConfig::load()?;
    let store = Store::open(&config.database_path)?;
    store.resolve_wiki_page_conflict(&op_id, expected_local_updated_at.as_deref(), accept_remote)
}

// ---------------------------------------------------------------------------
// S05: 待办/目标/规则同步冲突审阅。通用化 wiki 页审阅：候选 reason
// （todo_deleted / goal_deleted / rule_deleted / base_revision_changed /
// goal_active_limit）并列展示双端状态，裁决显式二选一；绑定失败关闭。
// goal_active_limit 的「采纳对端」被显式拒绝（活跃上限是业务不变式）。

#[derive(Clone, Debug)]
pub struct ObjectConflictDto {
    pub op_id: String,
    pub object_kind: String,
    pub object_id: String,
    pub reason: String,
    pub sender_device_id: String,
    pub sender_device_name: Option<String>,
    pub incoming_summary: String,
    pub incoming_status: String,
    pub incoming_deleted: bool,
    pub incoming_updated_at: String,
    pub local_summary: Option<String>,
    pub local_status: Option<String>,
    pub local_updated_at: Option<String>,
    pub local_head_deleted: bool,
}

/// 列出待审阅的待办/目标/规则同步冲突（按流内序）。UI 必须保存
/// `local_updated_at`（行已物理删除为 null）并在裁决时原样回传。
pub fn list_object_conflicts() -> Result<Vec<ObjectConflictDto>> {
    let config = AppConfig::load()?;
    let store = Store::open(&config.database_path)?;
    let reviews = store.list_object_conflicts()?;
    let roster = store.current_signed_roster()?;
    Ok(reviews
        .into_iter()
        .map(|r| {
            let sender_device_name = roster.as_ref().and_then(|roster| {
                roster
                    .roster
                    .members
                    .iter()
                    .find(|m| m.device.device_id == r.sender_device_id)
                    .map(|m| m.device.name.clone())
            });
            ObjectConflictDto {
                op_id: r.op_id,
                object_kind: r.object_kind,
                object_id: r.object_id,
                reason: r.reason,
                sender_device_id: r.sender_device_id,
                sender_device_name,
                incoming_summary: r.incoming.summary,
                incoming_status: r.incoming.status,
                incoming_deleted: r.incoming.deleted,
                incoming_updated_at: r.incoming.updated_at,
                local_summary: r.local_summary,
                local_status: r.local_status,
                local_updated_at: r.local_updated_at,
                local_head_deleted: r.local_head_deleted,
            }
        })
        .collect())
}

/// 显式裁决一条待办/目标/规则冲突。`expected_local_updated_at` 原样回传审阅
/// 时展示的本端行 updated_at；不匹配失败关闭。`accept_remote=false` = 保留
/// 本端现状（head 活跃则重新捕获广播，对端看到反向候选对称收敛）。
pub fn resolve_object_conflict(
    op_id: String,
    expected_local_updated_at: Option<String>,
    accept_remote: bool,
) -> Result<()> {
    let config = AppConfig::load()?;
    let store = Store::open(&config.database_path)?;
    store.resolve_object_conflict(&op_id, expected_local_updated_at.as_deref(), accept_remote)
}

/// 应用升级检查结果。has_update 只在远端提交序号**严格大于**本地时为
/// 真——hash 相同/不同都不足以分辨方向，`git rev-list --count` 才单调。
#[derive(Clone, Debug)]
pub struct AppUpdateDto {
    pub has_update: bool,
    pub latest_hash: String,
    pub latest_seq: i64,
    pub local_hash: String,
    pub local_seq: i64,
    pub version: String,
    pub changelog: String,
    pub apk_size: i64,
    pub apk_sha256: String,
}
