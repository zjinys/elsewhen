# 知识库消化洞见浮现与行动闭环（Surfacing Digest Insights & Action Loops）

- 日期：2026-11-05
- 状态：`implemented`（2026-11-05 代码复核确认 P0–P2 工程完成，P3 真实模型/真机验收待做；Backlog 移入收尾）
- 范围：后台知识消化与洞察生成后的用户感知与转化闭环；涵盖主对话流轻量气泡卡片（即时感知与四重行动出口）与 `system_context` 系统提示词画像切片（长效记忆唤醒）；频控与防扰纪律；多设备同步与主设备写入规则
- 性质：产品体验 + 知识闭环扩展（数据表/状态机 + 5s worker 频控调度 + 主对话 `DigestInsightBubble` 组件 + `system_context.rs` 画像注入）
- 已拍板决策（2026-11-05）：
  1. **坚决排除顶部全局横幅**：避免与 `GoalCompassNudgeBanner`（目标航向校准）等形成双横幅视觉轰炸与横幅疲劳。
  2. **主推「对话流新沉淀气泡（即时感知与行动闭环）」+「系统上下文自然唤醒（长效记忆）」双轨组合**。
  3. **严苛频控纪律**：仅限 `insight`（高阶认知洞察）、`habit`（新固化习惯）、`recurring-cost`（重复固定成本）等高价值类型；全局**每日最多浮现 1 条**未读洞见卡片；夜间静默；永不强制阻断用户操作。
  4. **四重闭环出口**：在卡片上一键提供「采纳为规则」、「开启习惯问候」、「转为待办」、「知道了」，让提炼成果直接转化为系统行为或长效规范。
  5. **多设备主写副显**：沿用 `habit_nudges` 规范，仅由主设备（Owner）向主对话历史写入问候卡，副设备同步展示与响应。
- 关联：
  - `src/wiki/digest.rs`（后台知识消化队列与批量落库）
  - `src/knowledge_background.rs`（四透镜认知洞察后台生成与 `insights` 表落库）
  - `src/storage/habit_nudges.rs`、`ui/lib/widgets/message_area_parts/habit_nudge_bubble.dart`（习惯主动问候交互与存储参考）
  - `src/ai/memory/system_context.rs`（主对话系统提示词组装与背景记忆）
  - `docs/notes/proposed/product/2026-10-22-proactive-habit-checkins.md`（习惯主动问候提案）
  - `docs/roadmap/2026-11-02-cognitive-goals-and-long-term-alignment-roadmap.md`（认知型目标与长程航向对齐）

---

## 1. 痛点分析：知识消化的「写入即封存」（Write-and-Forget）

### 1.1 现状与割裂
Elsewhen 拥有非常成熟且精密的后台知识挖掘与消化能力：
1. 用户在日常中记录无结构化的真实事件（如散步、加班、饮食、情绪、工作）。
2. 分析队列（`trigger_analysis`）异步提取客观实体、情绪和指导要点。
3. 知识消化队列（`trigger_knowledge_digest`）在后台将事件聚合成 Wiki 页面（如 `habit/after-lunch-walk`、`recurring-cost/...` 等）。
4. 认知洞察后台（`tick_knowledge_insights` / `run_automatic_insights`）运用四透镜（固定成本、闲置产能、已有动作叠加收益、案例类比）提炼出高阶认知洞察。

**然而，在用户体验侧存在严重的「写入即封存」断裂：**
- 所有的 Wiki 页面和洞见记录，都在静默写入 SQLite 数据库（`wiki_pages` 与 `insights` 表）。
- 用户绝大多数时间都在**主对话与记录流**中工作，几乎很少主动去左侧栏点击 Wiki、展开树形列表逐页排查“AI 昨天帮我总结了什么”。
- **典型案例**：
  > 用户连续记录散步和简单拉伸，后台洞察引擎敏锐地提炼出：
  > **「散步已固化，叠加拉伸比新增运动块更省成本」**
  > （透镜3：在已有动作上增加收益；行动建议：散步后直接就地进行 5 分钟小腿拉伸）。
  >
  > 这条极其精准的个人认知杠杆，仅仅因为静默存入 Wiki 页面，用户**完全不知情**，散步后依然忘记拉伸；而后台发现的「午饭后散步」新习惯，也因为用户没看到页面，而无法触发 `habit_nudges`（习惯问候必须用户在习惯页详情内手动点「开启」）。

提炼如果不被感知，智能就等于不存在；洞察如果不形成闭环，记录就只是负担。

---

## 2. 方案对比与选型决策

针对如何让后台洞察“被看见、能闭环”，我们评估了四种可能路径：

| 方案 | 形式与机制 | 优点 | 缺点 | 结论 |
|---|---|---|---|---|
| **方案一：上下文自然唤醒** | 将最新洞见切片注入 `system_context.rs`，聊天时让 AI 自然提及 | 极度温和、无界面杂音、体验拟人 | 纯被动：如果用户这几天只聊工作或不聊相关话题，洞察可能数月无法浮现；无结构化行动闭环按钮 | **采纳为长效记忆底座** |
| **方案二：对话流新沉淀气泡** | 后台生成高价值洞见后，在主对话流末尾生成一张轻量卡片（类似 `HabitNudgeBubble`） | 即时感知、在主路径可见、可直接附带结构化闭环操作按钮（采纳/开启/待办/忽略） | 需严格频控防刷屏 | **主选方案（即时感知与闭环主干）** |
| **方案三：顶部拾贝横幅** | 屏幕顶部弹出可折叠 Banner，提示「发现 1 条新的生活洞察」 | 醒目、跨界面常驻 | **严重冲突**：现有顶部已有 `GoalCompassNudgeBanner`（航向脱靶校准）及待办提醒 Banner，双横幅会导致界面逼仄和严重的横幅盲视与烦躁 | **明确坚决排除** |
| **方案四：每日回顾强制呈现** | 仅塞入每日回顾报告（Daily Review）中 | 集中消费、不打扰平时 | 依赖用户有查看回顾的习惯；缺乏将洞见一键“转化为系统行为”的直接按钮 | 作为辅助展示，不作主闭环 |

### 选型决策：双轨协同架构
- **轨道 A（即时感知与行动闭环）**：以**方案二**为主干。后台沉淀出高价值洞见后，在主对话流中呈现轻量气泡卡片，提供明确的 4 个行动出口。
- **轨道 B（长效记忆与背景画像）**：以**方案一**为底座。将近期活跃的核心习惯与高价值洞察作为记忆画像注入 `system_context.rs`，让主对话 AI 随时具备上下文感知。

---

## 3. 防打扰纪律与频控原则

为了守住「不打扰、不羞辱、不抢注意力」的核心哲学，建立以下铁律：

1. **高价值类型准入过滤（信噪比红线）**：
   - 并非所有 Wiki 更新都浮现。普通的实体更新（`entity/`）、普通主题聚合（`topic/`）、摘录素材（`source/`）**绝对不浮现**。
   - 仅对以下具备直接指导意义的类型开放候选池：
     - `kind = 'insight'`（四透镜提炼的认知洞察，尤其带 `action` 建议的）；
     - `kind = 'habit'`（首次由事件归纳成立的新习惯页）；
     - `kind = 'recurring-cost'`（识别出的隐性重复固定成本）。
2. **严苛频控：全局每日最多 1 条**：
   - 本机每日（本地自然日 00:00 ~ 23:59）最多只允许向主对话流写入 **1 条** 认知洞见气泡。
   - 绝不因后台一次性处理了 3 个洞见而连发 3 张卡片。多余候选保留在队列中，按置信度排队或平滑推迟到后续日期。
3. **打开可见，不抢系统通知**：
   - 绝不弹窗、不发操作系统通知、不响铃。
   - 只在用户打开 Elsewhen 或回到主对话界面时在历史流末尾平滑显示。
4. **时效性与自动静默**：
   - 洞见卡片产生 72 小时后若用户未作任何操作，自动标记为 `expired/dismissed`，不再作为未读卡片置顶呈现，不形成“待办债务”。
   - 同一洞见或同一 Wiki 页面终身最多浮现 1 次。

---

## 4. 四重行动闭环设计（Action Loops）

洞见卡片不仅要“展示”，更关键的是“促成闭环行动”。卡片底部根据洞见内容动态提供 4 个轻量操作：

```
┌──────────────────────────────────────────────────────────────┐
│ 💡 认知洞察 · 在已有动作上增加收益                             │
│                                                              │
│ 散步习惯已稳定，叠加拉伸比新增运动块更省成本                     │
│                                                              │
│ 观察：午饭后散步已连续记录 12 天；与其单开运动块，不如在散步后     │
│ 顺势做 5 分钟小腿与跟腱拉伸。                                  │
│ 来源依据：[[habit/after-lunch-walk]]、3 条日常事件             │
│                                                              │
│ [ 采纳为规则 ]  [ 开启习惯问候 ]  [ 设为待办 ]    [ 知道了 ]     │
└──────────────────────────────────────────────────────────────┘
```

### 4.1 出口一：采纳为规则（Adopt as Personal Rule）
- **触发意图**：用户认可该认知，希望将其作为长期的自我准则。
- **动作响应**：
  - 调用 `create_rule`，内容写入 `rules` 表（如：“散步结束后顺势进行 5 分钟拉伸，叠加微习惯”）。
  - 该规则正式生效，自动赋予 `essence` 语义标签，并进入 `system_context` 的个人规则提示词库。
  - 卡片状态原地切换为「已采纳为个人规则 ✓」，操作按钮收起。

### 4.2 出口二：开启习惯问候（Enable Habit Nudge）
- **触发意图**：该洞察涉及具体习惯（或关联了 `habit` 页），用户希望由系统定时关心进度。
- **动作响应**：
  - 后端调用 `get_habit_nudge_suggestion(page_slug)`，自动依据习惯页生成定制问法与时间（如：每天 13:00 问「午饭后走步拉伸了吗？」，按钮「走了拉了/没拉」）。
  - 弹出轻量确认弹窗（复用现有的习惯确认卡交互规范），用户一键确认即落入 `habit_nudges` 表。
  - 卡片状态原地切换为「已开启习惯定时问候 ✓」。

### 4.3 出口三：设为待办（Create Todo）
- **触发意图**：洞察包含具体的短期行动建议（`action`），用户想将其安排进近日日程。
- **动作响应**：
  - 一键将 `action` 内容生成为一条待办事项（`todos` 表），关联至该知识页。
  - 卡片状态原地切换为「已加入待办清单 ✓」。

### 4.4 出口四：知道了 / 忽略（Acknowledge / Dismiss）
- **触发意图**：用户了解了，但暂不需要系统采取行动；或认为该推断并不适用。
- **动作响应**：
  - 更新该洞见浮现记录为 `dismissed`。
  - 卡片淡化并原地收拢为简短只读展示（「已了解」），不再干扰后续阅读。

---

## 5. 存储架构与数据模型

### 5.1 浮现记录表：`digest_insight_surfacings`

为了严谨管理频控、多设备同步与生命周期，新增专门的调度与状态跟踪表：

```sql
CREATE TABLE digest_insight_surfacings (
    id TEXT PRIMARY KEY,                       -- UUID
    insight_id TEXT,                          -- 关联 insights.id（若来自洞察）
    page_slug TEXT NOT NULL,                  -- 关联 wiki_pages.slug
    kind TEXT NOT NULL,                       -- insight / habit / recurring_cost
    title TEXT NOT NULL,                      -- 洞见简短标题
    observation TEXT NOT NULL,                -- 观察正文
    action_suggestion TEXT,                   -- 可选建议动作
    status TEXT NOT NULL DEFAULT 'pending'    -- pending -> surfaced -> acted / dismissed / expired
        CHECK(status IN ('pending', 'surfaced', 'acted', 'dismissed', 'expired')),
    surfaced_at TEXT,                         -- 写入主对话的时刻 (ISO-8601)
    surfaced_date TEXT,                       -- 本地日历日 (YYYY-MM-DD)，用于日频控去重
    message_id TEXT,                          -- 对应 messages.id
    action_type TEXT                          -- 最终用户采取的闭环操作: rule / habit_nudge / todo / dismiss
        CHECK(action_type IN ('rule', 'habit_nudge', 'todo', 'dismiss')),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE INDEX idx_digest_insight_surfacings_date
    ON digest_insight_surfacings(surfaced_date, status);
CREATE INDEX idx_digest_insight_surfacings_slug
    ON digest_insight_surfacings(page_slug);
```

### 5.2 消息类型扩展：`messages.kind = 'digest_insight'`
- 扩展 `messages.kind` 枚举支持：
  - `chat`（普通聊天）
  - `habit_nudge`（习惯问候）
  - `habit_checkin`（习惯打卡回答）
  - `habit_nudge_suggest`（连续忽略建议）
  - **`digest_insight`（知识消化洞见气泡）** —— `ref` 存放 `surfacing:<surfacing_id>`。
- 协议层与同步检验：
  - 更新 `src/sync/protocol/records.rs:319`，在 `sync_message_kind` 白名单中合法放行 `digest_insight`。
  - 角色约束：`role == "assistant"`，具备合法的 `message_ref`。

### 5.3 多设备同步边界
- 遵循与 `habit_nudges` 完全一致的 **Owner 主设备写入原则**：
  - 仅在当前设备为同步 Owner（或单机独立设备）时，后台调度器才执行插入 `messages` 操作；
  - Joiner 设备不重复写消息，通过同步协议直接接收 `digest_insight` 消息；
  - 任何设备上点击闭环操作（如采纳为规则），生成的规则（`rules`）或打卡记录均作为一等公民双向同步。

---

## 6. 系统提示词上下文（System Context）长效记忆注入

在 `src/ai/memory/system_context.rs` 中，新增认知画像记忆注入段：

```rust
/// 知识库核心洞见与习惯画像注入：
/// 选取最近 30 天内活跃且置信度高的核心认知洞察与习惯要点（最多 3~5 条）；
/// 提炼为简短画像切片，让 AI 在主对话中自然具备对用户生活节奏的常识记忆。
pub(crate) fn user_cognition_profile_background(
    store: &Store,
    conversation_id: &str,
) -> Result<Option<ContextMessage>>
```

- **注入内容范式**：
  ```
  【用户近期生活与认知画像（来自经过验证的知识库提炼，供对话时自然参考，不必刻意复述或背诵）：】
  - 习惯规律：午饭后有固定散步习惯（连续性高）；习惯在散步后进行轻度拉伸。
  - 认知策略：倾向于在已有稳定动作上叠加收益，避免突兀开启高门槛新项目。
  - 决策偏好：重视睡眠规律，避免夜间长程脑力投入。
  ```
- **预算控制**：严格限制在 300 字符以内；`knowledge_mentor` 或 `goal_mentor` 模式下自动隔离，不掺入无关联上下文。

---

## 7. 实施计划（Roadmap 阶段规划）

### 7.1 P0：存储、模型与协议支持
- [x] 创建迁移 `71-digest-insight-surfacings`：建表 `digest_insight_surfacings` 与索引（编号 70 已被命名密钥库迁移占用，顺延为 71）。
- [x] 存储层 `src/storage/digest_insights.rs`：候选登记 `record_digest_insight_candidate`、浮现调度 `tick_digest_insights`、读面 `list/get`、闭环 `act_on_digest_insight`（`pop_due_digest_insight` 语义并入 tick 的单事务领取+插入）。
- [x] 协议层 `src/sync/protocol/records.rs` 放行 `messages.kind = 'digest_insight'` 与 `habit_nudge_suggest`（角色约束 assistant）。
- [x] Rust 存储单测 6 项（`src/storage/tests/digest_insights.rs`：同页终身一次/每日 1 条/静默窗与次日恢复/owner 单写/四动作幂等/72h 过期/副设备按正文重建台账）。

### 7.2 P1：后台频控调度与 System Context 记忆注入
- [x] `apply_digest_batch`（新 habit / recurring_cost 页）与 `run_automatic_insights`（带 action 的洞察，引用页含活跃 habit 时挂该 habit 页）提交阶段登记候选。
- [x] Dart 5s worker `_tickDigestInsights` 接入 `tick_digest_insights`：每日 ≤1、07:00–22:00 静默窗、Owner 判定、72h 过期、向主对话插入卡片消息（ref=`surfacing:<id>`）。
- [x] `src/ai/memory/system_context.rs` 新增 `user_cognition_profile_background`（≤300 字符、30 天窗、最多 5 条；knowledge_mentor/goal_mentor/知识页会话隔离）。
- [x] 调度频控与画像注入随 Rust 单测覆盖（含模式隔离断言）；注入段为可选背景，预算压力下整体可弃。

### 7.3 P2：Flutter UI 与四重行动闭环实现
- [x] FRB 接口：`tick_digest_insights`、`list_digest_insight_surfacings`、`pending_digest_insight_count`、`act_on_digest_insight(message_id, action)`（`src/api/digest_insights.rs`）。
- [x] `ui/lib/widgets/message_area.dart` 识别 `message.isDigestInsight` 并派发 `DigestInsightBubble`。
- [x] `DigestInsightBubble`：展示标题/观察/建议/来源（来源用新知识页预览弹窗打开）；副设备无本机台账时按卡片正文兜底渲染。
- [x] 四重闭环：采纳为规则（`add_rule`，essence 由既有回填管线补）/ 开启习惯问候（复用习惯推断确认对话框，确认后记录闭环）/ 设为待办（`create_todo`，关联来源页）/ 知道了（dismiss）；终态后卡片收拢为只读状态文案。
- [x] Flutter widget 测试 6 项（`ui/test/digest_insight_bubble_test.dart`：四出口渲染/习惯资格门控/已采纳终态/已了解只读/过期只读/副设备兜底解析）。

### 7.4 P3：端到端验证与回滚防线
- [ ] 验证后台消化产生洞见 -> 次日/当日主对话平滑浮现 -> 采纳为规则 -> 主对话后续回复自然引用的端到端链路（自动化部分已由存储/协议/组件测试覆盖，真实模型链路待真人复验）。
- [ ] 验证多设备主写副显与同步幂等性（真机）。
- [x] 更新 Backlog 登记与状态追踪（2026-11-05 复核，移入收尾）。

## 落地记录（2026-11-05）

- **落地范围**：P0 + P1 + P2。Rust：迁移 71、`src/storage/digest_insights.rs`、`src/api/digest_insights.rs`、协议放行、`apply_digest_batch` / `run_automatic_insights` 登记钩子、`user_cognition_profile_background` 三处调用点；Dart：5s worker tick、`digestInsightSurfacing(s)Provider`、`DigestInsightBubble` 与派发、`KnowledgePageDialog` 来源联动。
- **与方案的落地口径**：
  1. 台账表定位为**本机表**（对齐 `habit_nudge_fires`），不进快照、不参与同步；跨设备的「主写副显」由同步的卡片消息承担，副设备动作按卡片正文解析**就地重建同 id 台账行**（前向兼容将来提升为同步对象）。「任何设备都能点击闭环」因此成立：规则/待办作为一等公民各自同步。
  2. insight 候选的挂页策略：引用页含活跃 habit 页时挂该 habit 页（「开启习惯问候」出口可用），否则挂洞察页自身；同一页终身最多浮现 1 次由台账去重兜底。
  3. 无行动建议（action 为空）的洞察不入池：无法形成四重闭环，守住信噪比。
  4. 静默窗定为本地 07:00–22:00（与 habit_nudge 同口径）；每日额度按本地自然日。
- **证据**：Rust 存储 6 项 + 画像注入 1 项单测；`cargo test --lib digest_insight` 全绿；全量门禁与提交见后续记录。忽略项：真人多设备验收（P3）保留。
- **Flutter 回归补充（2026-11-05）**：`/tmp/elsewhen-fluttertest.log` 的唯一失败来自将 dismissed 与 expired 放在同一用例连续替换 `ProviderScope` override，后一次渲染仍读取前一次的缓存。两个终态现拆为独立用例，并分别断言四个动作全部隐藏；定向 6 项通过，测试文件 `fvm dart analyze` 无问题。`fvm flutter test --no-pub --reporter expanded` 全量 **534 passed / 0 failed**，结果见 `/tmp/elsewhen-fluttertest-fixed.log`。修复测试隔离，不变更卡片业务状态和生产代码；真人多设备验收仍待完成。

## 2026-11-05 代码复核

确认候选登记在 `src/storage/digest.rs` 与 `src/knowledge_background.rs`；5s worker 在 `ui/lib/bridge/rust_bridge_parts/worker_reminders.dart`；画像注入在 `src/ai/memory/providers.rs` 的三条组装路径；卡片与四出口在 `digest_insight_bubble.dart`。`src/storage/tests/digest_insights.rs` 有 7 项存储/画像测试，Flutter 卡片有 6 项组件测试。同步消息协议放行该 kind，本机台账显式排除快照；副设备重建台账已有测试。代码及自动化覆盖不替代真机与真实模型验收。本次仅复核与文档修正，未运行新测试。
