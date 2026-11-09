# Agent Note: 事件分块的完整请求预算与单节点执行

Status: implemented

## Problem

[事件阅读存储](2026-11-09-event-reading-storage.md)提供固定范围和租约，但不能用固定字符数代替实际模型窗口。旧 `AiProvider::fits_request` 对未实现预算的自定义 Provider 默认放行；直接复用会让长事件发生无界发送。失败渠道与较小窗口渠道还需要在同一节点身份下切换。

## Decision

`src/knowledge/event_reading.rs` 提供 E2 的 `advance` 执行函数。调用一次最多完成一个叶节点，只有全体叶覆盖后才返回 `LeavesComplete`；此状态不表示原事件消化成功。[E3 合并和最终页面提交](2026-11-09-event-reading-finalization.md)已另行实现，自动调度见[E4实现](2026-11-09-event-reading-worker.md)。

### 请求与范围

`AiProvider::visit_request_budgets` 只访问具体候选渠道的纯本地预算检查，无网络调用。默认不提供预算；OpenAI-compatible 和 Ollama 使用与实际发送相同的完整序列化包装、模型计数器、输出预留和安全余量。Failover 访问当前有限候选链，忽略无预算渠道，保留既有顺序和冷却规则。

叶规划从已完成叶的末尾开始，在最多 6,000 个 Unicode 字符内二分探测完整请求。先检查不带原文的固定请求；空请求也不容纳时返回 `event_reading_fixed_instructions_exceed_budget`，不创建空节点。没有任何可验证预算时返回 `event_reading_provider_budget_unavailable`。窗口／输出预留配置无法计算预算时排除该候选；若无候选可用则保留配置原因。

固定范围前选择所有可用候选所容纳正长度前缀的最小值，为较小窗口的故障切换保留空间。网络执行只调用一次严格的 `generate_budgeted_prepared`，每个候选发送前再次完整预检，并校验／续期同一节点租约。没有预算的渠道不能走旧默认放行。已持久化范围不因下一 tick 的窗口变化重命名或冒充不同覆盖；不容纳该范围的渠道跳过发送，并保留可操作的等待原因。

### 冻结指令与协议

prompt catalog 升为 `prompts-v10`。`task.processing.fixed_contract` 增加中英文 `event_reading_contract`、`event_reading_leaf`、`event_reading_merge`、`event_reading_context` 资源，全部随原 digest bundle 固定。旧 bundle 使用独立冻结的 `event-reading.v1.{en,zh-Hans}.json`，按 bundle 原来的后台语言选择，不能借用当前设置。原 `reading.policy` 和通用 reading contract 仍随任务固定。

输入为带原事件 ID、绝对 Unicode `[start,end)`、总字符数和原文的 JSON。输出仅接受严格 `ReadingOutput`：生成摘要与带绝对位置的逐字引文，拒绝未知字段、额外来源和尾随杂质。输入明确要求保留否定、时序、日期、数量、条件、分歧及不确定性，不能声称读过范围外材料。

网络响应须无未执行工具调用，明确正常结束（`stop`／`eos`／`end_turn`）；缺失结束状态、输出耗尽、过滤结束、坏 JSON、伪造引文和输出超限均不进入已完成缓存。序列化输出上限 16,384 字节，正文校验和缓存限额沿用存储层。usage 记入既有 token 统计。失败按节点退避，不修改原事件或确认原消化 job；重启继续复用已确认节点。

## Alternatives considered

- 使用旧默认 `fits_request=true`：缺少实际协议与窗口证据，不能用于自动分块。
- 按字符数切 6,000 字后直接发送：中文、代码、包装及输出预留使字符数无法证明请求可容纳；6,000 仅作为额外资源上限。
- 第一个渠道确定大范围，较小渠道读取前半段后复用节点 ID：会把不同覆盖缓存为同一节点。当前固定前取候选最小可用范围，固定后严格重检。
- 在一个 tick 递归读完整事件或加 JSON 修复调用：突破有界后台调用约束。格式失败留待下一次节点重试。
- 把新版阅读协议直接塞进旧 bundle：改变进行中任务的策略身份；独立冻结 v1 资源提供兼容路径。

## Consequences

所有实际发送均经过具体渠道预算，重启／失败不重复消费已确认叶节点，原文与事件身份保持不变。较小可用渠道可能增加分块数量和费用；未完成范围已经固定后，若后续只剩更小渠道，须等待足够窗口的配置／渠道恢复，不能伪装缩小后的覆盖。无网络调用时也保留明确配置失败，调度层的可见等待和公平性由 E4 实现。

完整覆盖是确定性存储／请求证据，不证明模型保留了所有语义；真实模型质量仍属于 E6。E2 不发布页面，不宣称长事件已经能在产品中自动完成消化。

## Testing

- `cargo test --lib event_reading -- --test-threads=4`：19 通过（11 存储、8 执行／资源）。包括多 tick 中英／emoji／代码／长引文精确重组原文、首中尾覆盖、重启复用、单节点调用、大小窗口／输出预留差异、旧 bundle 原语言兼容、无预算／空候选、固定指令超窗、失败节点范围不可改写、8 类无效输出、无网络写锁及重新分类后的迟到响应拒绝。
- `cargo test --lib storage::prompts::tests -- --test-threads=4`：18 通过。
- `cargo test --lib ai::provider::failover::tests -- --test-threads=4`：9 通过。
- `cargo test --lib digest_tests::dynamic_budget -- --test-threads=4`：3 通过，普通路径仍保持完整输入与动态批次保护。
- 合成 Provider 复用真实 OpenAI-compatible 完整序列化预算，只脚本化网络响应。这些是E2阶段证据；后续回环HTTP与真实模型验收见[最终验收记录](../../../testing/2026-11-09-long-event-real-model-acceptance.md)。

修改文件 rustfmt 与 `git diff --check` 通过；本次两篇实现笔记的限定范围 tree／format 校验通过（2 notes）。无效窗口候选可被跳过后再次运行执行专项：8 通过。既有全库笔记格式缺口沿用 E1 记录，不将限定范围校验称为全库通过。
