# Agent Note: 保存乐观锁与关 tab 保存档位（v1.5 + §11 Q3）

Status: implemented

## Problem

两个遗留风险点：(1) 编辑会话期间后台 digest 写回同一页时，「完成」保存会静默覆盖 digest 的新内容——双方都留 revision 可回滚，但用户无感知；(2) 关闭有未保存修改的 tab 只有「取消 / 放弃修改并关闭」两档，想保留改动需先取消、回页面点「完成」、再关 tab；且缺少编辑器惯例的鼠标中键关 tab。

## Decision

**乐观锁（§11 Q3）**：`save_wiki_page_content`（storage + api + bridge）增加可选 `expected_updated_at`。UI 保存时携带加载快照的 `page.updatedAt`；Rust 侧与当前值不一致则拒绝并报「编辑冲突」前缀错误。UI 捕获后弹三选对话框：重新加载（放弃本地改动、刷新页面）、强制覆盖（跳过锁再存，reason 标注「冲突后覆盖」）、取消（留在编辑态，不显示错误条——通过内部 `_SaveCancelled` 异常区别于真实失败）。

**时间戳比较按毫秒精度而非字符串**（关键实现细节）：Dart 侧 `DateTime.parse` 把纳秒截断为微秒并转本地时区，rfc3339 字符串无法精确往返（"Z" vs "+00:00"、精度位数）。Rust 解析两边为时间戳后比较 `timestamp_millis()`——floor 复合性质保证同一时刻 Dart 截断后毫秒值不变；解析失败按冲突处理（fail-closed）。

**关 tab 三档（v1.5）**：新增 `wikiSaveCallbacksProvider`（slug → `Future<bool>` 保存回调），详情页 body 挂载时注册、卸载时经 microtask 延迟注销（dispose 处于 widget tree 卸载期，同步改 provider 会抛「Tried to modify a provider while building」；且 dispose 后 `ref` 不可用，notifier 须提前缓存）。关闭脏 tab 对话框加第三档「保存并关闭」：保存成功才关闭，失败/冲突取消保持打开。

**中键关 tab**：tab chip 外包 `Listener` 识别 `kMiddleMouseButton` 按下，与「×」按钮共用 `_closeTab` 脏检查入口。

## Alternatives considered

- 字符串精确比较 updated_at：Dart 往返格式漂移必然误判冲突（要么永远冲突、要么传 null 形同虚设），弃。
- Dart 模型加 `updatedAtRaw` 透传原始字符串：需改 15 处 WikiPage 构造点，弃；毫秒精度比较同等可靠且零模型侵入。
- 冲突时自动「重新加载」不询问：可能丢弃用户未保存的长篇编辑，弃。
- dispose 里同步注销保存回调：触发 provider-during-build 异常（集成测试实证），改 microtask 延迟。
- 乐观锁做版本号/etag：updated_at 已够用，不引入新列。

## Consequences

编辑期间被后台更新的页面保存时得到显式冲突提示，不再静默互相覆盖；两个版本都留在 `wiki_revisions` 可恢复。关 tab 可一步「保存并关闭」，中键关闭符合编辑器惯例。代价：保存路径多一次对话框分支；`reason` 默认值顺手从 `'[human] GUI 编辑'` 修为 `'GUI 编辑'`（Rust 侧会自行拼 `[human]` 前缀，原默认值导致双重前缀——顺带修复）。Rust 新增 1 项乐观锁测试；Flutter 新增 5 项集成测试（中键关闭 / 保存并关闭 / 冲突三选各路径），全量 97 项绿、cargo 144 项绿。