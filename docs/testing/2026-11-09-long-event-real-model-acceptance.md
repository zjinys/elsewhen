# 超长事件真实模型验收记录

2026-11-09；关联[路线图 E6](../roadmap/2026-11-09-long-event-digestion-roadmap.md)。本记录只使用公开合成场景和临时数据库，没有读取或发送个人事件。请求与输出统计见[结构化证据](2026-11-09-long-event-real-model-results.json)。**E6 已通过长事件、最终提交、恢复与短事件隔离验收；仍保留单真实 Provider 账户和设备范围限制，不代表多设备实机验收。**

## 环境与限额

Provider：已有测试配置的 OpenAI-compatible 网关 `hub.oaifree.com`；请求及响应模型 `gpt-4o`。本地配置窗口 4096 tokens、输出预留 1536、输入安全余量 512；这是验收主动设置的小窗口，不代表服务端模型最大窗口。每次发送都通过具体 Provider 的完整序列化预检。只有一个真实配置，本次没有证明真实多 Provider 故障切换。

材料为 100,663 Unicode 字符的合成 Project Aster 事件。首段：2026-11-09 有17个原型；中段：2026-11-15 未批准购买42个传感器，预算待批；尾段：2026-12-01审查、仅3次验证、未授权发货。中间重复公共文档整理背景，原文由 `src/digest_tests/real_long_event.rs` 确定性生成，结构化证据保存原文 SHA-256。

显式验收测试默认 `#[ignore]`，只有手动指定 `--ignored` 才调用真实模型。三个独立试验分别为1、2和32次有限调用，合计39次（前置35次 + 跟进4次）；第三组的32次额度包含后续恢复，额度用完后停止，没有为凑通过继续付费。usage 已报告合计51,498 tokens；网络错误没有返回 usage，不能将它们算作零成本或声称得到了完整账单。

## 已取得的真实证据

- 第三组完成17个叶、3个合并、最高2层，完整覆盖原文。根状态 ready，未生成任何业务页面。首／中／尾事实及否定、日期和数量在根摘要保留。
- 根的三条引文分别定位在 `[12,100)`、`[50274,50378)`、`[100550,100663)`；三条逐字原文校验通过，均来自原事件。
- 每个 tick 在独立 OS 子进程执行，进程退出后从同一隔离库继续；已确认节点不重复调用。过程中改为中文指令偏好，新任务设置没有替换正在进行任务的英文 bundle。后台真实 Bridge 的存储重新初始化另有回环 HTTP 验证，不能替代本段真实进程证据。
- 根完成后重试只调用最终消化，没有重新执行20个已完成节点。最终网关超时后，跟进验收在注入本地 503 的首渠道失败后切换真实 Provider，最终事务成功提交。
- 前置两次真实最终响应保留17／42／审查日期／三次验证／未授权发货，但因 UUID 来源协议旧校验被拒。跟进第3次最终请求提交 `project/project-aster`，页面 evidence_count=1，来源只有原事件 UUID；同一事务确认 reading task 与 digest job。

## 失败与修复

首次调用复制了正确引文但坐标误算；系统拒绝为 `event_reading_quote_forged`。第二组完成首节点后又因重复背景引文的坐标歧义失败。修复见[引文位置记录](../notes/implemented/bug-fix/2026-11-09-event-reading-quote-locations.md)：叶内逐字定位，原坐标有效则保留，否则采用本段第一个逐字位置作规范定位；不允许跨叶找文本、改写引文或合并新增引文。重复文本的规范位置不声称唯一。

阅读策略新增具体事实优先、背景段可不摘录的中英文指导，已有 bundle 继续保持固定。第三组保留旧有效节点后跨进程继续，未清空缓存规避失败。

一次验收父进程因并行 cargo 重新生成测试二进制而无法再按旧可执行路径派生子进程；验收 harness 改为在隔离目录保留稳定可执行副本，再恢复同一数据库，未重新消费已完成节点。

最终 UUID 来源问题已补严格映射：仅最终路径允许本批真实事件 UUID 作为编号别名，其他来源继续拒绝，底层页面来源仍为原事件 ID；相关阅读专项39通过。**额度耗尽后未再真实调用，因此该修复后的最终真实提交尚无证据。**

网关多次连接／发送超时。已使用 `proxy-pick` 只读检查，当前代理节点154ms且为最快可用节点，因此没有切换节点或路由；代理测速不能证明模型网关稳定。最终请求共用既有60秒单渠道超时。

## 实际范围与已知限制

- 没有实际 owner／多设备实机验收，继续保留该范围限制。
- 短事件本次真实返回空提议，完成了路径与延迟隔离，但没有把短事件语义质量算作通过。
- 真实配置只有一个账户；故障切换使用注入本地 503 + 真实 Provider，已验证切换路径，不是第二个真实账户。

## 复跑入口

```sh
cargo test --lib digest_tests::real_long_event::real_model_long_event_acceptance -- --ignored --nocapture
```

命令读取测试环境配置或仓库 `.env` 中的测试 Provider，不打印密钥。每个新试验创建独立临时库；同组恢复通过 `ELSEWHEN_ACCEPTANCE_RESUME_DIR` 指定 helper 命名的临时试验目录，仍受整组32次调用上限。已经耗尽的第三组只能离线检查，不应继续调用。后续是否开启新一组付费验收，应明确保留本记录的失败与成本，不能把新通过覆盖旧失败。

## 跟进验收与最终页面复核

2026-11-09 用户要求继续后，另开最多4次真实请求的跟进组。使用 `VACUUM INTO` 从第三组合成库复制完整根，保留全部任务身份、bundle、原始失败记录和20个已完成节点，没有重读材料。该组实际4次调用、已报告5,133 tokens；总计39次、已报告51,498 tokens。没有密钥进入本记录或结构化证据。

前三次均为最终消化，先经过实际 HTTP 503 注入渠道，再切到真实 `gpt-4o`。前两次正文带 Markdown 围栏，严格 JSON 入口拒绝且没有页面写入；第3次合法数组正常提交 `project/project-aster`。页面核查：17个原型、2026年11月9日、11月15日未批准42个传感器／预算待批、12月1日审查、仅三次验证、未授权发货均保留。日期自然语言表示与原文 ISO 日期一致。页面仍明确该项目是合成场景。来源只含原事件 `8890679a-de5c-41d7-a14d-2d884fc86c52`，证据计数1；任务 succeeded、无遗留租约；再次 tick 为 Idle，没有新网络调用。

离线逐项比较最终库与父试验库：20个节点 ID、范围、输入／输出 hash、JSON输出完全一致；叶覆盖连续到100,663，原文 SHA-256不变；任务 bundle 未替换。最终 dataset 输出策略仍为英文，虽然用户指令偏好已切为中文。最终生成内容与根引文分别记录，模型生成摘要不当成原文引文。

| 对比 | 网络响应耗时 | 已报告 tokens | 结果 |
|---|---:|---:|---|
| 第三组全部成功阅读响应（含后续被校验拒绝的响应） | 累计192.191秒，中位5.871秒 | 38,416 | 17叶／3合并形成完整根；含失败成本，不能称为最优成本 |
| 跟进最终失败1／2 | 15.664／12.291秒 | 1,386／1,376 | Markdown围栏被拒绝，根保持可用 |
| 跟进最终成功（只复用根） | 9.271秒；含渠道切换tick为10.147秒 | 1,328 | 成功发布、证据去重、无重读 |
| 短事件直接消化 | 10.911秒；tick为11.556秒 | 1,043 | 无阅读任务；模型返回空提议，只有调度／延迟验收，短事件语义质量不算通过 |

这些是同一网关的一次有限样本，不代表稳定延迟或货币价格。失败连接的计费不可由usage缺失推断。阅读结果占13,338字节；完成后的再次tick零调用。100,663字场景在4096配置窗口下，需要多个后台tick，实际应用还会有5秒调度间隔；此处未将子进程立即推进的时长当作应用端到端耗时。

固定资源边界：原文2,000,000 Unicode字符、叶输入最多6,000字符（仍受实际完整请求预算约束）、4,096节点、合并扇入2–8、层级16、每节点16,384字节输出、任务缓存16MiB、600秒租约、现有单渠道60秒超时。达到限额明确退避并保存原文；若固定指令、二元合并或既定范围放不进候选窗口，需调整真实窗口／输出预留／指令或等待渠道恢复。分块不会消除所有配置错误，也不解决另一个长事件分析worker。

## 完成审计证据映射

| 要求 | 已核验的证据 |
|---|---|
| 原文／ID不变、范围完整、最终来源真实 | 实际合成库hash和逐节点比较、完整根三条原文引文、实际最终页面来源；`mixed_unicode_leaves_cover_every_scalar_once_across_ticks_and_restart` |
| 普通路径、预算／failover、固定指令拒绝 | `digest_tests::dynamic_budget`、`real_http_leaf_and_final_failover_respect_each_serialized_window_and_output_reserve`、`strict_reader_does_not_resend_after_server_context_rejection`；真实503注入后切真实模型 |
| 固定bundle、策略／语言隔离 | `settings_changes_do_not_replace_task_bundle_or_completed_results`、`old_digest_bundle_uses_frozen_reading_contract_in_its_original_locale`、真实首节点后设置变化与最终英文产物 |
| 有界tick、缓存重用、公平性 | `background_ticks_discover_read_merge_and_commit_without_charging_preparation`、`multiple_long_events_rotate_and_configuration_failure_leaves_short_work_eligible`、真实独立子进程tick及最终缓存复用 |
| 租约、恢复、权限撤销与迟到结果 | 存储／最终租约专项、`final_receipt_cannot_cross_processor_or_membership_generation`、真实逐tick进程重启；实机多设备另记未覆盖 |
| 原子提交、人工保护、不重复确认 | `final_transaction_failure_rolls_back_pages_log_and_both_acknowledgements`、`final_digest_respects_human_body_and_missing_coverage_cannot_be_ready`、实际页面evidence_count=1及再次Idle |
| 追加迁移／旧库保留／本机缓存不出同步 | v77升级、`v78_to_v79_preserves_event_bundle_identity_and_confirmed_node_bytes`、snapshot完整排除登记与22项回归 |
| 只读中英文进度／无手动触发 | `knowledge_digest_progress_test.dart`、`long_event_digest_bridge_test.dart`、ARB／AST门禁、Flutter全量 |

本路线真实质量验收范围是长事件Pipeline和上述合成场景，已达到路线要求；单真实账户、注入故障、短事件空提议和无多设备实机证据仍保留，不推广为所有事件／设备的质量保证。

工程最终门禁：Rust库1185、集成2通过／22显式忽略，release通过；Flutter602通过，analyze无问题；ARB2026对key、AST39处既有已归属例外、Python负例3、格式／diff与7篇变更笔记限定校验均通过。全库Agent Note历史格式不兼容仍保留E1记录，不把限定校验宣称为全库笔记通过。
