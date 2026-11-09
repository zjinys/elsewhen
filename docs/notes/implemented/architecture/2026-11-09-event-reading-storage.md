# Agent Note: 超长事件阅读的本地持久任务与租约

Status: implemented

## Problem

普通知识消化在单条完整事件超窗时只能保留原文并退避。持久分块需要独立缓存与并发边界；否则重启重复付费、正常分块推进计成整事件失败，或旧处理设备的迟到结果污染进度。

## Decision

[超长事件路线](../../../roadmap/2026-11-09-long-event-digestion-roadmap.md)的 E1 使用追加迁移 `78-event-digest-readings`，由 `src/storage/event_readings.rs` 提供本地存储 API。此记录只描述存储，不宣称模型阅读、自动调度或最终消化已经接线。

- `event_reading_tasks` 固定事件 ID、原文 SHA-256、Unicode 总字符数、消化版本、原批次完整 prompt bundle、阅读策略及 bundle 依赖指纹。bundle 已包含阅读／消化指令及语言策略，设置变化不能改写任务身份。
- `event_reading_nodes` 使用 Unicode `[start,end)`；叶范围从 0 连续追加。合并节点只引用有序连续、已完成节点。`event_reading_dependencies` 固定子输出 hash，并限制一个子节点只有一个父节点，构成森林。重复准备返回原节点；不同依赖不能冒充相同范围。
- 节点领取在短 `IMMEDIATE` 事务内完成，返回 token、任务代数、处理权限代数和固定节点定义。调用方在事务结束后调用网络。每次领取递增任务代数；有效租约续期，过期回收复用原节点。节点失败退避为 120/240/480/960 秒，第五次起 6 小时，与原消化 job 的 attempts 分离。
- `event_reading_authority` 是本机权限代数；replica 身份、成员权限或 processor 移交状态变更均递增（普通同步序号／时钟更新不递增）。领取、续期、失败和成功提交重新校验处理权限。权限先失效再恢复也不能接受旧调用结果。
- 人工重新分类触发任务取消并递增代数、撤销租约；再次成为可记录事件后，新的有效原批次 claim 可显式恢复缓存。已完成节点不可更新；旧租约仍无效。
- 完成节点验证原文范围、逐字引文、输出上限、权限与租约，再将结果和释放租约原子提交。相同租约／输出的回执幂等；冲突回执拒绝。节点准备和完成均不修改原事件、不创建页面、不确认消化 job。

### 固定限额与边界

策略 `event-reading-v1`；单任务原文最多 2,000,000 个 Unicode 字符，最多 4,096 节点，合并扇入 2–8，最高 level 16；单节点序列化输出最多 16,384 字节、摘要最多 1,600 字、引文最多 8 条且每条最多 400 字；单任务输出缓存最多 16 MiB；租约每次有效期 600 秒。超限返回明确的 `event_reading_*_limit` 原因，不能缩短原事件后确认。

迁移只建本地表和约束，不扫描原事件、不创建阅读任务、不执行模型调用。后台仍由 `Store::open_background` 检查版本，旧库返回等待前台初始化。同步启用的 quiescence 检查包含阅读租约。此阶段未新增 FRB 或 UI API，未改变业务同步的实体集合。

## Alternatives considered

- 借用 source snapshot 或 chat message 身份：混淆来源与事件身份，无法固定原事件消化 bundle。
- 只保存内存进度：重启丢失已确认阅读，且无法验证依赖版本。
- 只检查当前 owner：权限移交再移回后，旧网络结果仍可能通过；因此另存本机权限代数。
- 阅读直接写页面或确认 job：未完整覆盖时会产生部分消化；存储模块只提交节点，最终事务属于 E3。

## Consequences

重启后可以复用已完成节点，租约不受打开数据库动作影响，回收后的旧响应无法覆盖新结果。代价是本地缓存与额外表，权限变更会保守地废弃进行中调用的提交权；接线后的产品行为与调度证据见后续实施记录。E2 的[请求预算与单节点执行](2026-11-09-event-reading-budget.md)已另行落地；[最终事务](2026-11-09-event-reading-finalization.md)、[调度与进度](2026-11-09-event-reading-worker.md)及[真实模型验收](../../../testing/2026-11-09-long-event-real-model-acceptance.md)也已独立留证。

## Testing

隔离合成库专项 `cargo test --lib storage::event_readings -- --test-threads=4`：11 通过。覆盖 Unicode 连续范围与重复准备、无提前确认／页面写入、重启与续期／过期恢复、重复／冲突回执、引文和范围伪造、节点独立退避、分类撤销再恢复、成员撤销／processor 往返、合并依赖缺口与父写入回滚、设置变化固定 bundle、提交失败事务回滚及 v77 旧库升级／后台拒绝升级。迁移专项 32 通过，既有处理权限专项 6 通过。新增 Rust 文件 rustfmt 与 `git diff --check` 通过。全库通用 Agent Note 校验因既有目录和历史头块不符合技能格式而失败，新增笔记的限定范围 tree／format 校验均通过（1 note），不将全库校验记作通过。
