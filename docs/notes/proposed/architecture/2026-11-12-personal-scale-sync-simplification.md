# 个人规模同步简化（稳定性优先的重构提案）

**提出**: 2026-11-12
**状态**: **Proposed（待评审，未排期）**——不改动现有已交付能力，只提出删减目标
**关联**: [多设备同步 S06](../architecture/2026-09-30-multi-device-sync.md)、[同步 Handoff](../../../roadmap/2026-10-07-multi-device-sync-handoff.md)、[生命周期边界路线](../../../roadmap/2026-10-27-sync-lifecycle-boundaries-roadmap.md)

## 1. 动机：一次真实互锁暴露的重量

2026-11-12 现场事故（细节见 §2）不是传输层 bug，而是**两个各自测试通过的子系统
组合出的死锁**：GC 的重快照标记 × 配对流程的 request id 校验。任何一方单独看都
正确，组合起来把手机配对永久锁死 17 小时，直到人工做数据库手术。

量化当前同步子系统的重量：

| 指标 | 当前值 |
|---|---|
| 同步相关 Rust 代码 | `src/sync` + `src/storage/sync` 合计约 **44,500 行** |
| 失败关闭点（`ensure!`） | **711 处** |
| `sync_*` 状态表 | 9 张（members / member_state / operations / outbox / gc_state / resnapshot_requests / snapshot_exports / snapshot_export_wires / wire_rejections …） |
| 自动化测试 | 192 项（`storage::sync`）+ 341 项（`sync`） |

而这套协议服务的最常见真实场景是：**1 台桌面 + 1 台手机，同一局域网，owner 就是
用户自己**。当前设计假设的是「不可信对端、任意多设备、长期演化」——远超个人
本地优先应用的需要。

用户明确的三条简化目标：**同步稳定性 > 性能 > 可用**。核心判断是：当前配对与
同步的成功**更接近运气而非可靠设计**——因为状态机的组合态（而非单个状态）无人
系统覆盖，任何跨子系统交互都可能产生未测试的死锁。

## 2. 事故复盘（2026-11-12，作为简化必要性的证据）

**现象**：Android 配对停在「等待成员授权与初始同步」，日志每 5 秒刷一次
`sync_snapshot_resnapshot_request_mismatch`。

**链条**：

1. 手机作为新成员加入 `sync_members`（`active=1`），此时**还没有** `sync_member_state`
   水印行（首份快照尚未送达）。
2. 桌面端 GC 把「`confirmed_sequence` 视为 0」的新成员判定为落后，打上
   `needs_resnapshot=1`（GC floor 803 > 256 追赶窗口）。
3. 重快照流程抢先建了一份 `resnap-…` 导出（`state='building'`），等手机 ack。
4. 但 **blob 分片上传不在 outbox 里**——没有任何路径会重推这份 building 导出。
   手机此时还在 enrollment 阶段，不会去 ack 这份重快照。
5. `claim_sync_resnapshot` 看到 `building` 就直接返回 `None`，**永久空转**。
6. 与此同时 enrollment 流程每 tick 尝试 staging 快照，撞上 id 校验
   （`src/storage/sync/delivery.rs`）抛 `sync_snapshot_resnapshot_request_mismatch`。
7. 两条流程互锁，直到人工删除 `resnap-%` 行、复位水印才恢复。

**根本教训**：这类 Bug 无法靠「多写测试」根治，只能靠**减少状态机组合面**根治。

## 3. 简化主张（三条）

### 3.1 用「快照永远可用」替代「增量追赶 + GC」

**删减对象**：`src/storage/sync/gc.rs`、`src/storage/sync/resnapshot.rs`、
`delivery.rs` 中的导出状态机大部分、三张表（`sync_member_state`、
`sync_gc_state`、`sync_resnapshot_requests`）。

**替代方案**：落后设备直接拉全量快照，不区分「增量追赶」与「重快照」两条路径。
个人数据集规模（数百 MB 量级）下，全量快照在局域网内的成本是分钟级，远低于
维护 GC floor / 追赶窗口 / 重快照请求 / 导出续传这一整套复杂度的代价。

**收益**：§2 事故的整条链路（GC 标记 → 重快照导出 → building 死锁）**不复存在**。

**代价**：设备长时间离线后重连需要重新传输全量快照。个人场景可接受。

### 3.2 设备生命周期：名册恒等于实际设备数

**当前问题**：每次重装 App = 全新设备身份，旧身份作为历史成员**永久留在名册**
（协议安全约束：`sync_membership_history_removed` 禁止删除历史成员，
`sync_membership_revoked_identity_reused` 禁止撤销身份复活）。实测当前名册 7 成员
中 5 台是死设备，名册**单调膨胀**。

**替代方案**：重配对时用新身份**顶替**旧身份行（在同一数据集锚下记 epoch 变化），
而不是靠名册保留全部历史。名册大小恒等于当前活跃设备数。

**约束检查**：这需要重新审视两条安全约束的**真实防护目标**——它们防的是「已解绑
设备伪造存活」，而非「保留历史审计」。若审计需求由 `memory` 型的本地日志满足，
名册本身可以只保留活跃成员。

**代价**：名册链的完整性证明从「全历史签名链」简化为「当前 epoch 的签名锚点」，
需要安全评审确认不削弱防伪造能力。

### 3.3 处理器移交：单 owner 场景降级

**当前问题**：后台处理职责移交 + yield proof 协议在单 owner + 单手机场景下
几乎没有实际使用面。

**替代方案**：处理器 = owner（固定），去掉移交协议。评估重点是**延迟分析边界**：
需要确认「手机记录、桌面分析」这一最常见路径不依赖该协议。

**代价**：多 owner 场景（当前产品未定义）需要重新引入。

## 4. 不能砍的部分（本地优先的根基）

- **端到端加密 + 签名名册的存在本身**——relay 是不可信中继
- **事件不可变、原始记录先行、AI 失败不阻塞**——产品根基约束
- **快照 + op 双层数据面**——op 增量做日常同步（性能），快照做引导（正确性）

## 5. 分阶段路径

### P0（已完成，2026-11-12）

即使整条 GC/重快照机制将来被删，当下版本也必须不再「靠运气」。已交付两个
失败关闭点的修复（迁移 v81）：

1. **GC 不再标记从未确认过的设备**（`src/storage/sync/gc.rs`）：无水印行的成员
   （正在配对）不参与落后判定，floor 仍按 0 参与最小值以保护其历史。§2 事故的
   第 2 步不再发生。
2. **卡死的 building 导出可被接管**（`src/storage/sync/resnapshot.rs`）：新增
   `progress_at` 列（迁移 v81）记录最后进展时刻；`claim_sync_resnapshot` 见到
   超过 35 分钟（略大于 relay blob 的 30 分钟无进展 TTL）无进展的 building 导出
   时 abort 它，让下一拍走 aborted 续传路径。§2 事故的第 5 步不再永久空转。

证据：`storage::sync` 192 项通过（新增 3 项回归：
`stalled_building_export_is_aborted_by_claim_so_the_request_can_resume`、
`fresh_exports_without_progress_stamp_are_not_aborted`、
`missing_watermark_row_counts_as_unconfirmed_not_excluded` 更新），全量
`cargo test --lib` 1233 通过（唯一失败为既有的预算测试，与本改动无关）。

### P1（本提案，待评审）

按 §3.1 → §3.2 → §3.3 顺序推进，每步独立可回退。建议先做 §3.1（收益最大、
风险最局部），验证稳定性改善后再动名册语义（涉及安全评审）。

### P2（可选）

「设备重装恢复旧身份」——当前是**设计缺口**（重装 = 新身份，见 §3.2）。中期方案：
设备密钥可导出/恢复，让重装复用旧身份而不新增名册成员。需独立提案。

## 6. 验收标准（简化生效的判据）

不以「测试全绿」验收，以**真实场景稳定性**验收：

- [ ] 连续 3 次「手机卸载重装 → 重新配对」，每次配对成功且名册成员数不增长
- [ ] 手机离线 24h 后重连，同步自动收敛，无需人工干预
- [ ] 桌面端 GC 运行期间不影响任何在途配对/同步（当前事故场景）
- [ ] 配对/同步失败时的日志能直接指出**单一**根因状态，而非跨子系统互锁

## 7. 开放问题

1. §3.2 名册语义简化的安全评审结论（是否削弱防伪造）
2. §3.1 全量快照在**移动网络**（非局域网）下的成本是否可接受
3. §3.3 处理器移交协议的实际使用统计（是否有真实调用面）

## 参考

- 事故现场与数据库手术记录：本文 §2
- 现有安全约束：`src/sync/membership.rs`（`validate_successor`）
- GC 实现：`src/storage/sync/gc.rs`
- 重快照实现：`src/storage/sync/resnapshot.rs`
- 导出状态机：`src/storage/sync/delivery.rs`
