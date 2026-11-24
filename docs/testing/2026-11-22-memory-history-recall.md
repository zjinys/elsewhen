# 同会话历史召回：工程基线与验证记录

日期：2026-11-22。关联 [FR-PES-002 v0.4](../requirements/product/FR-PES-002-记忆系统.md)、[记忆增强提案](../notes/proposed/architecture/2026-11-21-agent-memory-anthropic-research.md)。本记录衡量工程行为与证据可达性，不作为 M0–M5 的整体质量签收。

## 实现边界

- 主对话自动召回当前 `conversation_id` 的旧讨论；知识页/目标讨论不自动注入主对话历史。既有 `search_knowledge_base` 仅搜索调用会话自身的聊天历史，沿用已有知识/事件检索权限。
- 复用冻结消息快照、词法查询与相邻轮次，限制候选、消息数、摘录和总字符预算；不增加检索模型调用、数据库表、公开 Rust API 或 FRB/Dart 变更。
- 历史包含消息角色、日期、版本与未读标记；提示模型区分讨论、确认和当前事实。每轮模型请求前检查实时来源、版本与关联事件资格，清理缓存历史及撤回事件副本；原工具配对和其他结果保留。
- 已有完整原文不重复注入；历史包作为一个可选背景段进入请求预算，可整体移除。没有命中、资格检查失败或旧冻结提示目录缺少新指令时可跳过自动召回。

## 固定合成集与基线

[pes-memory-history-v1.json](fixtures/pes-memory-history-v1.json)固定 24 题，历史偏好、相关经历、时间变化、纠正、冲突、同源重复、来源失效、跨多来源共八类，每类三题；全部为临时数据库中的合成材料。21 题为正例，3 题为关联事件撤回后的排除场景。夹具在历史与当前问题间加入无关轮次，让旧证据离开直接消息窗口。

对照使用 `SimpleMemory::new(1)` 的直接聊天窗口，只统计非 system 消息是否含预期旧证据；增强组调用生产历史检索函数，核对预期片段及禁用片段。

| 指标 | 直接聊天窗口 | 同会话词法检索 |
|---|---:|---:|
| 正例旧证据可达 | 0 / 21 | 21 / 21 |
| 已撤回来源排除 | 不作质量推论 | 3 / 3 |
| 真实模型调用 | 0 | 0 |

该对照没有计入 system 摘要、既有背景或模型主动工具调用，不能称为整个旧产品的问答基线，也不能将 21/21 称为模型回答正确率。夹具覆盖角色与冲突证据，但未验证模型是否正确理解冲突、采纳状态及来源独立性；没有人工质量标注、真实材料集、等模型等预算回答对比或跨会话授权基线。

## 执行结果

1. 改动前 `cargo test --lib ai::memory::tests -- --nocapture`：32 passed。
2. 最终 `cargo test --lib ai::memory:: -- --nocapture`：46 passed，0 failed。包括 14 项新增历史测试：24 题集、完整原文去重、三类范围隔离、冻结快照排除后续输入、近期指代、无命中、五种工具结果包装的撤回清理、修改/删除失效、未读与长度限制、预算整体裁剪、中英文指令、冻结快照不复活删除来源、真实工具分发及生产 agent 两轮请求之间撤回。
3. `cargo test --all-targets` 首次在沙箱内运行：1277 passed、39 failed、28 ignored。单元目标失败后，其他集成目标未继续执行。35 项失败涉及禁止本地回环端口/网络；另 4 项是现有工作区断言，见下表。
4. 为排除沙箱影响，在允许回环端口的环境重跑协议、generation、streaming、compass HTTP、历史与提示测试：67 passed、5 failed。四项为并行压力下的夹具时限，另一项为原提示断言。四项随后用 `--test-threads=1` 单独复跑全部通过：三个 `native_vs_text::http_*` 协议场景及 `observable_run_checkpoints_and_cancels_without_final_message`。没有据此宣称全量通过。

| 现有失败 | 观察到的差异 |
|---|---|
| `migration_applies_goals_table` | 断言 schema 版本 90，当前工作区已有迁移 91 |
| `english_fixed_contract_retains_machine_protocol_and_approval_boundaries` | 既有英文 contract 缺少断言要求的字面量 `[事件 #123]` |
| `snapshot_carries_every_table_or_explicitly_excludes_it` | 当前工作区的 `wiki_tags` 尚未列入快照表登记 |
| `cognition_profile_background_injects_recent_high_value_pages_and_isolates_modes` | 原断言与工作区已有按查询选择背景的行为不一致 |

本次未修改这些断言或对应的其他工作区功能。测试日志位于 `/tmp/elsewhen-history-final-tests.log`、`/tmp/elsewhen-history-all-tests.log`、`/tmp/elsewhen-history-network-tests.log`、`/tmp/elsewhen-history-serial-tests.log`；日志是本机验证产物，不属于持久产品数据。

## 尚未验收

未调用真实服务商、未使用个人数据库、未做 GUI/Android 或长期观察；尚无大规模历史性能与实际 token/延迟收益测量。词法扫描不保证语义相近、跨语言或远距离无共同关键词的纠正被找回。来源检查发生在 agent 每轮请求准备时，不声称覆盖 provider 内部网络重试期间的所有竞态。完整主张生命周期、独立确认/否定重建、跨会话聊天授权、模型回答质量及 M3–M5 继续列为待办。
