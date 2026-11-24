# UI/UX 工匠级重构路线图 (UI/UX Craft Redesign Roadmap)

分支：`feat/ui-ux-craft-redesign`。
目标：彻底消解“功能驱动型工程界面（Engineering-driven UI）”带来的系统性杂乱感与认知负荷，将 Elsewhen 打造成兼具静谧呼吸感、严谨空间物理与连贯交互心智的现代消费级桌面生产力工具（Craft-quality Desktop App）。

---

## 一、 核心设计原则

1. **视线锚定（Anchor Focus）**：主对话流是用户的核心思考空间，任何面板的呼出均不得挤压、重排或推移主对话区。
2. **轻量伴随（Lightweight Companion）**：待办、目标、提议等上下文以轻量悬浮卡片形态伴随，支持原地即时切换。
3. **严密尺度（Token Discipline）**：拒绝任意手写字号与边距，依靠 4 级字阶与材质明度差建立层次，告别“线条盒式界面”。
4. **单主线心流（Continuous Stream）**：强化个人终身单主线时间轴心智，将 AI 提议、阶段翻篇与即时捕获沉浸式闭环。

---

## 二、 阶段推进与任务清单

### Phase 0: 基础设施与规范收敛 (100% 已完成)
- [x] **全库字阶收敛（Typography Scale Tokens）**：清理全库 44 个文件中散落的 80 处 `fontSize: 13` 和 11 处 `fontSize: 15`，收敛至 4 级标准 tokens（11/12/14/16）。
- [x] **状态栏防折行与紧凑单行化（NowStatus Anti-wrap）**：按钮尺寸规范化，自然宽度降至 380px，分屏或小窗绝不折行。
- [x] **悬浮伴随工作区与无缝切换（In-place Companion Workspace）**：右侧抽屉升级为 14px 圆角悬浮磨砂卡片；引入 `activeCompanionIdProvider` 状态追踪，顶部按钮升级为 Active Pill 状态胶囊；修复事件冲突，实现待办 ↔ 目标原地平滑单跳切换。
- [x] **伴随工作区自动化单测套件**：补充 `switching directly from todo to goal updates panel and active companion` 单元测试，全量测试与 `dart analyze` 全绿。

### Phase 1: 核心视线流与呼吸感重构 (100% 已完成)
- [x] **P1-1 通栏沉浸式阅读流（无人工强制留白）**：
  - 内容区与输入框保持通栏自然展开，彻底移除两侧人工大留白限制，让多行长文与代码完整利用全屏空间。
- [x] **P1-2 底部输入区微悬浮胶囊设计（Floating Composer Dock）**：
  - 在 `composer_field.dart` 中赋予常态环境光微投影与 16px 底部留白，全宽自然舒展，聚焦感与呼吸感兼备。
- [x] **P1-3 时间轴分割线与翻篇记录质感优化**：
  - 微调 `_DayDivider` 为微胶囊芯片搭配 0.5px 发丝微光分割线，提升章节印记感。

### Phase 2: 交互内嵌卡片与信息闭环 (进行中)
- [x] **P2-1 对话流内提议卡片工匠化（Inline Proposal Card）**：
  - 在 `proposal_review_dialog.dart` 中去除 Material 3 原生粗硬卡片，引入 10px 圆角、`DesignTokens.hairlineSide` 发丝微光描边，高亮项采用微透明主题强调色衬底（`accentPrimary.withValues(alpha: 0.18)`）。
- [ ] **P2-2 采纳提议与伴随卡片数量联动微动效**：
  - 点击采纳后，提议卡片平滑淡出或收拢，右上角伴随卡片的待办计数与清单无感实时同步。
- [x] **P2-3 悬浮伴随卡片内容细节打磨**：
  - 在 `todo_item.dart` 与 `todo_view.dart` 中为待办条目引入 8px 悬停圆角反馈、统一 11/12/14px 标准字阶，优化复选框交互密度。

### Phase 3: 全局导航与侧边栏微交互 (100% 已完成)
- [x] **P3-1 6 大一级导航选中态标准化**：
  - 在 `app_theme.dart` 中建立标准 `navigationDrawerTheme`：8px 圆角矩形胶囊、微透明强调色背景 `accentPrimary.withValues(alpha: 0.14)`、选中文字与图标主题色统一。
- [x] **P3-2 对话态「最近主题」面板精致化**：
  - 在 `left_sidebar.dart` 中统一条目行距、15px 规范图标尺寸与 6px 圆角高光悬停反馈。
- [x] **P3-3 底部系统设置与配对状态指示器对齐**：
  - 在 `sidebar_destinations.dart` 中将设置入口圆角标准化为 8px，图标规格与一级导航对齐（20px），消除跑道圆角突兀感。

### Phase 4: 全局回归、多分辨率验证与合并 (待推进)
- [ ] **P4-1 窗口缩放与响应式断点回归**：
  - 校验大屏（>= 1200px）、中屏（840px ~ 1200px）、窄屏分屏（< 840px）及移动端形态。
- [ ] **P4-2 浅色与深色双主题明度对比度校准**：
  - 确保深色与浅色模式下的半透明磨砂与文字对比度均符合 WCAG AA 标准。
- [ ] **P4-3 测试套件全绿与分支合并**：
  - 运行全量 `flutter test` 与 `dart analyze`，保持零 Warning，合入目标分支。
