# 事故档案：「保存」确认后 AI 空回复，兜底模板误导用户 + 「存为主题」按钮报错

**日期**：2026-11-02　**状态**：两处根因均已修复并有回归测试
**涉及代码**：`src/ai/conversation/continuity.rs`（failure_reply 措辞）、`src/wiki/tweet_import.rs`（save_text_page_with_kind endorse）

## 一、现象时间线（会话 93196d4f）

| UTC | 谁 | 内容 |
|---|---|---|
| 01:36 | assistant | 目标细化答复，结尾「说『保存』我就会草拟页面供你确认」 |
| 02:53 | assistant | nvzhuang 市场评估，结尾同样引导「说『保存』」 |
| 03:01:48 | user | 「保存」 |
| 03:01+ | assistant | gpt-oss:120b@wududu 连续 3 轮空回复（content_len=0, tool_calls=0，completion 全烧在 reasoning 被网关吞掉）→ 兜底消息：「关于你刚才的要求『评估一下 nvzhuang…』，这一轮还没有完成……你的问题还挂着，说『继续』」 |
| 03:04 | user | 改点消息气泡「存为主题」按钮 → 弹窗报错「只有采集素材页（source/note）可以表态评价，当前 kind=topic」 |

两个独立缺陷在同一串交互里先后命中。

## 二、根因

### 根因 1：确认词失败后兜底模板张冠李戴
`failure_reply` → `pending_request()`：`"保存"` 命中 `is_confirmation` 词表 → 回溯到上一条"真请求"（nvzhuang 评估）→ 套「问题还挂着说继续」模板。用户刚才是**确认动作**，不是提问；「说继续」完全无法理解。数据库佐证：`pending_actions` 无新行，草拟从未发生——不是草拟错了，是根本没执行。

### 根因 2：topic 页保存误入 endorse 通道
`save_text_page_with_kind(kind="topic")` 无条件调用 `set_wiki_opinion(endorse)`，而该接口白名单只有 source/note，对 topic 直接 bail。后果：**页面已入库**（topic-15e2de08 于 03:04:24 成功创建），但 API 返回错误，UI 显示异常——保存实际成功用户却以为失败。

## 三、修复

1. `pending_request` 返回 `was_confirmation`；确认词失败且此前无预告时改说「你刚才确认的『保存』这一轮没有生效……再说一次『保存』即可重试」，不再追加「说继续」尾巴。此前有预告（promised）时保留原「承认没做到」模板（2026-10-18 事故语义不变）。
2. `save_text_page_with_kind`：endorse 仅对 note/source 执行；topic 页入库即视为认可，不额外表态。

## 四、回归测试

- `src/ai/conversation/continuity_tests/progress_recovery.rs::confirmation_failure_says_save_not_applied_not_question_pending`
- `tests/topic_endorse_test.rs`（topic 保存不 bail + note 仍 endorse）

## 五、附带观察

- wududu 网关 gpt-oss:120b 当天至少 3 次同款空回复（01:34/01:35/03:01），reasoning tokens 烧掉但内容不回传。空回复重试日志计数显示 retry 始终为 1（`empty reply; retry=1 finalize_next=false/true`），值得后续核对计数逻辑。
- 另一并行工作流（goals Phase G1）恰好把修复 1 的 tweet_import.rs 改动顺带提交进 b55dde4；continuity.rs 修复保留在工作区。
