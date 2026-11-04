# Goal 升级一级菜单后的 AI 侧定位 Review

**文件路径**：`docs/review_logs/2026-11-02/review_goal_first_menu_positioning.md`
**创建时间**：2026-11-02
**输入**：用户提问「goal 单独提到一级菜单了，之前这个工具是个人认知相关的，定位如何修改」；通读 `src/ai/memory/prompts.rs`、`src/ai/memory/system_context.rs`（G2 注入段）、`src/ai/tool/goal_tools.rs`、`ui/lib/widgets/left_sidebar.dart`（769b378 双入口拆分）、`docs/roadmap/2026-11-02-cognitive-goals-and-long-term-alignment-roadmap.md`
**审查方式**：只读 review，未修改任何文件
**状态**：已落地并验证（2026-11-04）——三项文案与断言测试已写入代码（落点修订见文末「落地记录」）；验证：`cargo test` **1011 passed / 0 failed / 20 ignored**（含新增断言），`fvm flutter test` 全量 **521 passed / 0 failed**，`fvm flutter analyze` 0 error

---

## 总结论

**产品侧已经把 goal 升级成「航向坐标系」，但 AI 侧还停在「战略背景附件」——定位差了一整级。**

一级菜单（769b378：左 tab 完整工作台 + 对话弹窗只读速览）给了 goal 产品级的家；roadmap 定性为「价值坐标系与战略指南针」「AI 是幕僚长」。但 AI 运行时上下文里，goal 只是被动参考的背景板。

---

## 逐层落差（代码实物）

| 层 | 现状表述 | 问题 |
|---|---|---|
| 基础角色（`prompts.rs` `SYSTEM_PROMPT_BASE`） | 「个人事件与知识库**助理**」；功能概览列了记录/知识库/待办/联系人/规则/搜索……**唯独没有目标** | AI 的自我认知里目标不存在，只在 G2 注入段临时出场 |
| 目标注入段（`system_context.rs` ~L80-135，G2 交付） | 「作为理解其长远意图与近期重心的**战略背景**」；准则 1「不强行把话题往目标上拽」、准则 2「用户迷茫/取舍/卡点时才结合」 | 定位是**被动的背景板**：不主动对照，用户不迷茫就不出场 |
| 工具引导（`prompts.rs` `TOOL_GUIDES`） | 有 web_search / dir_import / people / events / archive / rename / topic_flow / derivative 八条 | **goals 没有条目**——立项/拆解/推进抓手/校准偏离这些幕僚核心动词，AI 只在被点名时才用工具 |
| Roadmap 定性（认知型目标 roadmap 不变式） | 「价值坐标系与战略指南针」「幕僚长」「零打扰基线」 | 文档雄心与 prompt 现实脱节 |

---

## 定位主张

Elsewhile 认知主循环：**捕获事件 → 沉淀知识 → 回顾 → 对齐航向**。前三环 AI 都有明确角色（记录助理、知识导师、回顾统计），唯独第四环——整个循环的**收敛点**——AI 没有身份。

> **目标不是 AI 的一个功能域，而是 AI 所有工作的坐标系。**
> 事件记录回答「发生了什么」，知识库回答「学到了什么」，目标回答「这一切为了什么」。AI 的角色应从「事件与知识库助理」升级为「**围绕 3 个活跃目标工作的个人幕僚**」。

---

## 落地修改清单（文案层，不动机制）

1. **`SYSTEM_PROMPT_BASE`**：
   - 角色句升级：助理定位补「围绕活跃目标工作」语义；
   - 功能概览补一行：「目标与航向：用户最多 3 个活跃目标（核心动力/当前抓手/卡点/完成判据），是你理解用户一切活动的坐标系；你的建议和整理工作应自然服务于当前抓手。」
2. **`system_context.rs` 目标注入段重新措辞**：
   - 从「战略背景（被动参考）」改为「航向（主动对照基准）」；
   - 保留零打扰准则（单日琐事不触发）；
   - 准则 2 从「用户迷茫时才结合」改为「**涉及取舍、规划、知识沉淀方向时默认对照**当前抓手，帮用户判断这件事在不在航向上」。
3. **`TOOL_GUIDES` 补 goals 条目**：list_goals / create_goal / update_goal_facets 的使用时机引导（立项、拆解、抓手推进、校准偏离）。

---

## 红线（roadmap 自定，不能破）

- **零打扰基线不变**：对照发生在「取舍/规划/沉淀」时刻，不是每句话挂靠；单日琐事与闲聊绝不触发目标话题。
- **AI 是参谋与镜子**：客观陈述事实 + 设问选择，不评判不指路。
- **上限 3 条**的注意力保护语义在文案中保留。
- 偏离检测仍走 G4 引擎（3~5 天滑动窗口），prompt 不做单对话越权判断。

---

## 执行状态与被拦原因（2026-11-02 原状；2026-11-04 已落地，见文末「落地记录」）

- **未落地**：上述三个文件（`prompts.rs`、`system_context.rs`、`TOOL_GUIDES` 所在文件）与 `goal_tools.rs` 同属并行会话 G2/G4 活跃工作区（goal_tools.rs untracked、system_context.rs modified 未提交，当日曾因其半成品产生过 5 个编译错误后自行修复）。为避免写撞，本轮只存档不改码。
- **触发条件**：并行会话 G4 收尾提交后，按本清单落地（纯文案 + prompt 断言测试，预计半小时内）。
- **顺带项**：`SYSTEM_PROMPT_BASE` 的功能概览已落后于实际功能面（缺目标、缺目标工作台入口引导），落地时一并核对一遍功能清单完整性。

---

## 落地记录（2026-11-04，G4 收尾后）

- **① `SYSTEM_PROMPT_BASE`**（`src/ai/memory/prompts.rs`）：角色句升级为「个人事件与知识库助理，也是围绕其活跃长程目标工作的幕僚」；功能概览新增「目标与航向」条目（3 条活跃上限、四切片、「目标」页工作台入口、坐标系语义与「日常闲聊不强行挂靠」）；顺带补「记事本」条目（不经 AI 分析、不进知识沉淀、AI 不代写）。
- **② 目标注入段**（`src/ai/memory/system_context.rs`）：段头改「航向基准——请主动对照」；准则 2 改「涉及取舍、规划、知识沉淀方向时默认对照当前抓手」；准则 1 零打扰护栏原文保留。
- **③ 工具时机引导落点修订**：TOOL_GUIDES（R2B）只渲染常驻（非延迟）工具，而目标六件工具全部延迟加载、由 `goal-management` 技能包覆盖——按原方案在 TOOL_GUIDES 加条目不会渲染，违背「不动机制」。实际落两处真实常驻面：目标段准则 3（enable_tools 启用时机 + 点名 list_goals / update_goal_facets / create_goal / pause_goal / complete_goal / reactivate_goal + 审阅确认纪律）；`goal-management` 技能包 brief / instructions（花名册常驻 + 加载时注入，补「取舍/规划时默认对照抓手、停滞或偏移时提议校准、不唠叨催办」）。
- **测试**：`memory_part2` 新增 `goal_block_states_navigation_baseline_and_tool_timing`；`memory_part4` 新增 `base_prompt_states_goal_coordinate_positioning`；测试注释同步（part2 / part3）。旧断言全部保留：零打扰护栏原文、无「对照检查」措辞、空态文案。
- **验证结果（2026-11-04 补跑）**：`cargo test` **1011 passed / 0 failed / 20 ignored**（含新增 `goal_block_states_navigation_baseline_and_tool_timing`、`base_prompt_states_goal_coordinate_positioning`）；`fvm flutter test` 全量 **521 passed / 0 failed**、`fvm flutter analyze` 0 error。
