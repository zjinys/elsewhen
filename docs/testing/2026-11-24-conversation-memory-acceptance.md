# 对话记忆验收（2026-11-24）

## 范围

对应 `docs/roadmap/2026-11-23-conversation-memory-consolidation-roadmap.md`。新记忆存储于 `personal_memory_claims.content`，来自用户对话；`profile/me` 保持个人档案职责。候选默认 pending，用户确认或修改后才可用于默认召回。

## 要求与证据

| 要求 | 实现与验收证据 |
|---|---|
| 正文独立，显示与召回一致 | `personal_memory/search.rs`、`mod.rs`、`management.rs`、`review.rs` 优先读取 content；`direct_memory_edit_changes_recall_without_a_wiki_page` 验证无知识页编辑和搜索 |
| 历史迁移 | 94 增列、95 仅回填 NULL、96 增量游标；迁移套件 37 项通过，覆盖已有正文不覆盖、缺页、重复初始化、决定及回执保留 |
| 对话自动提取 | `knowledge_background.rs` 与 `memory.rs` 接后台 tick；只接受用户消息、校验原文引用、限制批次及正文长度 |
| 增量与恢复 | 游标、候选及规则账本同事务；语义租约领取、提交前复核、失败冷却和过期恢复；management/review 测试覆盖 |
| 去重、补充、冲突 | 精确与词法归一去重；模型判断同主题关系；只替代未确认有效候选，保留双方依据；冲突标记新候选 |
| 用户控制 | 删除/否定/停用抑制，人工编辑版本校验及审计；列表/详情隐藏移除正文及历史编辑内容；不覆盖确认或人工正文 |
| 范围隔离 | 其他会话全局重复记录不返回 ID/状态，不被关系分析更新；设置管理列表可查看所有会话来源，召回权限单独校验 |
| 设置页 | 实际正文、搜索、分页、编辑、取消、审批及异步保护；Flutter 页面/真实桥接/设置入口合计 14 项通过，定向 analyze 无问题 |
| 同步与恢复策略 | 记忆表与游标明确排除于跨设备业务快照，本机策略专项通过；本轮没有引入记忆同步能力 |

## 全量回归及限制

`cargo test --lib --quiet` 在允许本地网络后执行：1389 passed、9 failed、28 ignored。该轮编译发生在修复迁移断言之前，不能当作最新代码全绿证据。

- 两项迁移断言仍写死 93：已修复，完整迁移套件 37 项通过。
- 四项 HTTP 协议测试在并发全量中报 missing HTTP request/connection refused，四项分别独立复跑全部通过：`short_new_conversation_fits_conservative_32k_budget`、`short_main_stream_input_survives_424_with_32k_fallback`、`http_missing_function_call_recovers_executes_once_and_persists_answer`、`natural_replies_never_execute_or_discard_unbound_proposals`。
- 英文固定契约缺 `[事件 #123]`，独立复跑仍失败。
- 快照全表分类门禁发现 `wiki_tags` 未分类；记忆表本机策略专项通过。
- 旧认知画像背景测试期望新增习惯页即自动注入，当前知识选择器不满足该预期，独立复跑仍失败。

后三项涉及已有知识页、固定提示词或同步分类行为，未在本轮改动它们来制造全量通过。全量测试结果保留为有失败，发布前应另行处理这些基线问题。

## 最终专项复核

最新 `cargo test --lib storage::personal_memory --quiet`：73 passed、0 failed。覆盖完整后台语义合并流程及不可用会话跳过后游标继续推进；提取规则/关系模块 10 项通过。本机快照策略专项通过，Flutter 14 项及 analyze 已通过。

本路线图的实现项与专项验收完成；仓库全量回归仍有上述明确记录的基线失败，不代表全部测试通过。
