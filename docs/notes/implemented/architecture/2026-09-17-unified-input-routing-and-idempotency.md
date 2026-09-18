# Agent Note: 统一输入路由与幂等边界

Status: implemented

## Problem

主输入同时承载个人陈述和外部 URL。若所有内容都先写成对话消息与事件，URL 会污染个人日流；若先抓取再落盘，断网或解析失败会丢失用户实际提交。另一方面，数据库虽支持可选幂等键，但 UI 不传 key 时，提交成功后响应超时再重试仍会制造重复消息、事件或素材入口。

## Decision

普通主输入在一个本地事务中写入 message、不可变 event、analysis job 和 input record。独立的 `http/https` URL 则先只写 `url_import` input record，状态进入 `needs_confirmation`，随后复用现有抓取与预览流程；用户保存预览后才关联 `wiki_page_slug` 并转为 `routed`。抓取失败标为 `failed`，但不删除原始输入。

一次 UI 提交在成功前持有一个稳定幂等键。相同文本的传输重试复用该 key；成功后清除，下一次主动提交生成新 key。因此“用户再次记录相同内容”仍是新事实，而“同一次提交因响应不确定而重试”返回原对象。

AI 回复重试只重新生成 assistant 回复，不重新提交 user input。确认型工具继续以 pending action 身份为执行边界：重放确认不得绕过已有确认记录。URL 抓取本身可以重试，但保存结果按 `source_url` 更新同一来源页，input record 按 key 和 `wiki_page_slug` 保持单一关联。

## Invariants

- 幂等键标识一次提交意图，不由正文哈希充当；相同正文允许被用户多次主动记录。
- 网络和 AI 不参与个人输入的首次持久化事务。
- URL 在用户确认保存前不得进入个人事件日流，也不得静默创建素材页。
- 已经提交成功但客户端未收到响应时，重试必须返回原 message / event / input 关联。
- assistant 生成失败不得导致 user input 再写一次。

## Confirmation boundary

| 输入或动作 | 首次提交行为 | 是否另需确认 |
| --- | --- | --- |
| Capture 个人记录 | 立即写 input record、event、analysis job | 否 |
| 主对话中的个人陈述 | 立即写 message、event、analysis job，并继续生成回复 | 否 |
| 独立 URL | 立即只写 input record，并打开导入预览 | 保存为素材前需要用户点击确认 |
| AI assistant 回复重试 | 只重跑生成，不重写 user input | 否 |
| 新建待办、改名、归档、人物关系等写操作 | AI 只创建 pending action | 执行前需要用户确认 |
| 查询、搜索、列表等只读工具 | 直接执行 | 否 |

统一入口只负责可靠接收和路由，不提升工具权限。工具重放必须继续服从各自的 read / direct-write / confirm-gated policy；input record 的 `routed` 状态不能被当成业务写操作已经获批。

## Alternatives considered

### 用正文哈希作为幂等键

实现简单，但会错误合并用户在不同时间主动提交的相同句子或相同 URL，丢失真实发生次数。

### URL 同样先写个人事件

能复用普通输入路径，但收藏链接不是个人经历，会污染“今天”和后续个人事实分析。

### 抓取成功后才创建 input record

数据库更干净，但网络失败时无法证明用户提交过什么，也违背“先可靠保存”的主循环原则。

### 每次重试生成新 key

无法处理“服务端已提交、客户端响应丢失”的典型不确定结果，等同于没有端到端幂等。

## Consequences

收益是普通陈述、URL、AI 回复重试拥有清晰且可测试的写入边界，断网不会抹掉原始 URL，响应不确定也不会重复制造权威对象。代价是 Flutter 必须在一次提交的失败与成功之间维护临时 key；预览被用户长期放弃时，input record 会保留在 `needs_confirmation`，后续需要在输入历史或清理策略中显式呈现，而不能静默删除。
