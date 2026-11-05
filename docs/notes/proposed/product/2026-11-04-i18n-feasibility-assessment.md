# 多语言支持可行性评估（i18n：对话块为重点）

- 日期：2026-11-04
- 状态：`analysis`（评估结论，未立项；先做用户定位决策再决定是否推进）
- 范围：全项目（Flutter UI + Rust core）多语言现状盘点与改造量级评估；重点考察对话（chat）相关链路
- 性质：可行性分析，不产生直接待办
- 结论摘要：**当前为纯中文单语言硬编码实现，无任何 i18n 基础设施**；对话块的特殊性在于中文不只渗透 UI 层，还渗透进 Rust AI 提示词层与错误回流层，后两者是真正的成本所在。建议先做产品定位决策（目标用户是否需要非中文版本）再决定是否投入。

## 1. 现状盘点

### 1.1 基础设施

| 项 | 现状 |
|---|---|
| `l10n.yaml` / ARB 文件 | ❌ 无（`third_party/appflowy_editor` 自带 30+ 语言 ARB，但只是编辑器依赖，主应用未接入 delegate） |
| `MaterialApp` 配置 | 只有 `DefaultMaterialLocalizations` + 中文 AppFlowy delegate（`lib/main.dart:167-177`），未设 `supportedLocales`，跟随系统 locale 但 UI 不响应 |
| 设置项语言切换 | 无（`settings_screen.dart` 及 `settings_parts/` 下无 locale/language 相关配置） |
| 复数/性别处理 | 无（`Intl.plural` 零使用） |
| 日期格式 | 已用 `DateFormat` 但符号写死中文（`'yyyy年MM月dd日'`、`'M月d日'` 与 ISO 混用），需 locale 感知初始化 |

### 1.2 硬编码中文分布

| 层 | 规模 |
|---|---|
| Dart UI（单引号字符串） | 约 **2,300+ 处**：`lib/screens` 286、`lib/widgets` 1,737、providers 36 |
| Rust 用户可见中文 | 约 **~5,700 行**；其中 `anyhow!/bail!/format!` 错误消息 **571 处** |
| AI 提示词/工具描述 | `src/ai/` 含中文行 3,427：`SYSTEM_PROMPT_BASE` 全中文人格、4 条协议修复 nudge、工具 schema description（`write_tools.rs` 26 / `read_tools.rs` 71 / `mod.rs` 131）、`CITATION_INSTRUCTIONS`、摘要 `INSTRUCTIONS` |
| 测试断言 | **53 个测试文件、578 处** `find.text('中文')`——i18n 改造的隐性耦合大头 |

## 2. 对话块分层现状（重点）

对话块比其他模块多两层暴露面：Dart UI（可常规 ARB 化）之外，还有 **Rust AI 提示词层** 与 **错误回流层**。

### 2.1 Dart UI 层（难度低，标准迁移）

- `message_area_parts/`（输入框 hint「AI 正在思考…」、now_status「今天」、message_bubble 菜单「复制/存为主题/纳入记录/重新分析」等 ~127 处）
- `goal_chat_panel.dart` 51 处、`chat_run_status.dart` 58 处、`chat_proposal_parts/`（proposal_detail/batch/review 等 28+ 处）

### 2.2 Rust AI 提示词层（❗最难最特殊）

- **文本工具协议的解析器只认中文标记**：`[工具调用]`、`[规则提议]`、`[规则本质]` 是运行时解析的协议格式而非展示文案。提示词换语言而解析器仍匹配中文串时，工具调用会**静默失效**。两者必须同步改造，协议标记本身应语言无关化（或解析器双语兼容）。
- **AI 输出语言无控制机制**：提示词全中文 → 模型自然回中文。若 UI 换英文而提示词不变，出现「英文 UI + 中文 AI 回复」割裂。需引入「回复语言跟随用户消息语言」指令。
- **知识库内容语言漂移**：wiki 页生成语言由模型输出决定，选材指令（`CITATION_INSTRUCTIONS` 等）也是中文。需产品决策：知识库固定中文还是跟随用户。
- **工具 schema description 全中文**：发给模型的功能描述，翻译影响模型理解质量，需真模型回归测试。

### 2.3 错误回流层（跨 FFI）

Rust 侧 571 处中文错误（「已达 3 条上限，请先归档一条」等）经 flutter_rust_bridge 原样透传，Dart 侧 `toast(e.toString())` 直显。两条路：

- **错误码化**（彻底）：Rust 返回结构化 `{code, params}`，Dart 查 ARB 渲染——改造量大
- **接受单语言**（务实）：多数桌面工具的做法

### 2.4 深水区：token 预算与语言耦合

`src/ai/budget.rs` 中文按 0.6~0.8 token/字估算。提示词换语言后字节率完全不同，预算校准参数需按语言区分——i18n 估算时最容易漏的一项。

## 3. 若决定做：分层路线与量级

| 阶段 | 内容 | 量级 |
|---|---|---|
| 0 | **决策**：目标用户/市场是否需要非中文版；知识库内容语言策略 | 决策 |
| 1 | 基建：`l10n.yaml` + ARB + `supportedLocales` + 设置页语言项；先迁对话块 UI 层（message_area_parts + goal_chat_panel + chat_run_status + proposal 系列） | 小~中，~400 处 |
| 2 | 错误码化：Rust 错误改结构化 code，Dart 渲染 | 中，571 处 + 桥协议 |
| 3 | 提示词层：SYSTEM_PROMPT_BASE 语言包化、协议标记与解析器语言无关化、「回复语言跟随用户」指令、工具 schema 翻译 + 真模型回归 | 大，风险最高 |
| 4 | token 预算按语言分参数；日期 locale 化；测试断言迁移（578 处） | 中 |

## 4. 总体判断

对话块 UI 文案的可迁移性没有问题；真正的多语言成本集中在 Rust AI 层——**提示词、协议标记解析、工具 schema、token 预算四者相互咬合**，且没有现成测试安全网（现有测试全部假设中文交互，如 `acceptance_controls.rs` 合成验收提示词直接用中文写验收指令）。

产品定位上，docs 全中文、产品人格（情绪回应、知识库分区名）均为中文语境设计。若目标用户明确是中文用户，**i18n 的 ROI 很低**——建议先用阶段 0 的用户定位决策否决或推迟整个计划，避免为一个不存在的需求背上长期双语协议维护负担。
