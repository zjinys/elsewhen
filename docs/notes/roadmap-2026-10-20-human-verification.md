# Roadmap：待人工确认事项（2026-10-20 整理）

来源：2026-10-18 / 2026-10-19 事故修复与摘录孵化提案的收尾盘点。
机器可完成的项已全部落地（见各档案「进展」节）；本文件只登记**必须由人
拍板或真人操作**的事，每条写明验收方式。完成一条就删一条。

## 1. memo 功能真人验收

- 状态：代码侧自标完成（`7eb15e1` 一带，用户 merge `beeb16f`）。
- 待做：真机跑一轮 memo 流程——创建、编辑、被 AI 引用。机器无真感，
  「好不好用」只有人能回答。
- 通过标准：日常顺手；引用出现在对话且归属正确。
- 结果动作：通过 → 从提案测试清单删条目；不通过 → 在
  `docs/notes/proposed/product/` 下开缺陷或改动提案。

## 2. `p1_replay_production_summary_failure` 手动验收

- 状态：实验测试已写但标 `#[ignore]`（`src/ai/summary/tests.rs` 尾部）。
- 阻塞原因：需要 app 解锁密钥态 + 生产会话副本数据库。
- 操作：`cp` 生产 db 到临时目录 → `P1_REPLAY_DB=<副本路径> cargo test
  --lib p1_replay -- --ignored --nocapture`。
- 通过标准：重放能命中当时的归并失败路径，且修复后的行为表现符合
  P1 预案描述（预算内分批追平、账本留痕）。之后删除或保留实验均有
  价值：保留需移出 `#[ignore]` 并脱敏 fixture。

## 3. 其他设备的 migration 50 错位风险

- 状态：本机主库 `user_version=49`，无错位；但**其他设备**若曾在
  migration 50 旧编号（不同 DDL）下落库，会缺 `reject_reason` 列且
  version 已≥50，导致新 migration 被跳过。
- 待做： findAll 活跃设备跑一次 app，查
  `PRAGMA table_info(wiki_pages)` 是否含 `reject_reason`；缺则手动补
  （或提供一次性修复 migration）。确认无第二设备后即可删除本条。

## 4. 知识页重构 + 关系提案过目

- 状态：`docs/notes/proposed/architecture/` 下有
  `2026-10-19-knowledge-page-reconsolidation-and-relations.md`，尚未
  被任何人评审。
- 待做：读完、拍板「做 / 不做 / 改了做」。若做，转成正式提案并排期；
  若不做，写两行否决理由归档（保持提案目录里每篇都有结论）。

## 5. 摘要管线观察窗（签名触发即回访）

非立刻验收，登记观察契约。P1 修复后以下两个签名一到，按对应预案
回访（细节在
`docs/notes/incidents/2026-10-19-summary-pipeline-permanent-failure.md`
的「缺陷留观察」小节）：

- 缺陷①多会话单选：第二会话 `summary_ready` 迟迟不出现。
- 缺陷⑥千条级会话：summary-preheat 账本每批命中数远低于 64
  （说明选择器没挑中最重的树）。
- 观察入口：回顾页知识管道健康条 → summary-preheat 账本（本次 UI
  收口新增的可见性）。
