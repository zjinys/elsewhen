# Agent Note: 归档对话可删除

Status: proposed

## Problem

对话目前只有「归档 / 取消归档」与「改名」两种操作（`ui/lib/widgets/left_sidebar.dart` 每个对话的 PopupMenu 只有 rename / archive 两项；`api.rs` 也只有 `set_conversation_archived` 等），**没有任何删除能力**。归档对话只能被隐藏，会永久留在归档视图里，无法真正清理。

用户需要：归档的对话应该可以删除（真正释放/清理，与「事件可记录性过滤」里的"讨论可收纳但可清理"配套），并且**像「将 XXX 的对话归档」一样支持指令驱动**——用户说「把 XXX 的对话删除」时由 AI 工具完成，而不只是 UI 手动操作。

## Proposal

给「归档视图」内的对话增加删除能力，删除入口只出现在归档态（`archived=1`），避免误删活跃对话，UI 与指令两条路径共用同一落库函数：

1. **存储层** `src/storage.rs`：新增 `delete_conversation(conversation_id) -> Result<()>`，一条 `DELETE FROM conversations WHERE id=?` 即可——`messages`（`ON DELETE CASCADE`）与 `pending_actions`（`ON DELETE CASCADE`）会级联清理，无悬挂引用（已核对建表 SQL：`messages.conversation_id`、`pending_actions.conversation_id` 均为 CASCADE）。
2. **API/Bridge 层** `src/api.rs`：新增 `delete_conversation(conversation_id: String)`，并重新生成 frb binding（`frb_generated.rs` / `.dart` / `.io.dart` / `.web.dart`）。
3. **UI 层** `left_sidebar.dart`：归档视图的对话菜单（现有 `PopupMenuButton` 的 rename / archive 两项旁）加「删除」项，红色 `Icons.delete_outline`；点击后弹**二次确认对话框**（说明"删除后对话与消息不可恢复"），确认后调用 bridge 并刷新列表。
4. **指令层（AI 工具）**：新增 `delete_conversations_by_title`，与 `archive_conversations_by_title`（`src/ai/tool/mod.rs`）同构：
   - 参数同样为 `title`（精确匹配）/ `contains`（子串、大小写不敏感），空标题按「新对话」参与匹配；
   - **只匹配已归档对话**（走 `list_archived_conversations()`，与 UI 一致，且天然排除知识页内聊天）；没有匹配时 bailing 并给出当前归档对话标题样例提示；
   - `ToolPolicy::WriteConfirm` 草拟确认制：调用后列出匹配标题，回复「好」才真正删除——删除不可恢复，确认门必须与归档同级；
   - 待确认动作执行器（`src/ai/tool/mod.rs` 的 pending action 分发）新增该 action，逐条调用 `delete_conversation`；
   - `src/ai/memory.rs` system prompt 补一句指令说明：「删除已归档对话：用户说『把 XX 对话删除』→ 用 delete_conversations_by_title（只匹配已归档对话），草拟确认后执行」。
5. （可选）活跃对话不显示删除入口；如未来需要，可加"删除需长按/设置开关"。

## Alternatives considered

1. 软删除（新增 `deleted` 标记）：可恢复，但对话本来就靠 `archived` 分级，再叠一层软删语义重复，且消息仍需保留——收益低。
2. 只允许归档、不允许删除：无法真正清理，不合用户需求。
3. 删除只清消息保留空对话：没有任何价值，反而留下脏数据。
4. 指令工具允许直接删活跃对话：与 UI 的"只删归档"不一致，误删风险高；用户想删活跃对话时先走归档，保持两条路径同一语义。

## Consequences

归档对话可以被真正清理；级联删除保证无孤儿消息/待确认动作。UI 与指令两条路径共用同一 `delete_conversation` 落库函数，语义一致。代价：删除不可恢复，UI 二次确认 + 指令 WriteConfirm 确认门必须都到位；frb 需要重新生成（改动涉及 bridge 契约，注意 flutter_rust_bridge 版本一致性）；memory.rs 需要补指令说明。删除范围限定已归档对话，把误删活跃对话的风险降到最低。