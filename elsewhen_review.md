# Elsewhen 项目深度代码 review（已保存）

**文件路径**：`/data/playground/ai/elsewhen/elsewhen_review.md`  
**创建时间**：$(date)  
**最后更新**：$(date)  
**状态**：已保存，可随时查看或继续编辑

---

## 📋 Elsewhen 项目深度代码 review 结果

**项目定位**：本地优先的个人认知系统（Rust 核心 + Flutter UI），核心原则：原始事件不可变 + AI 仅提案 + 知识复利（LLM Wiki）

**架构亮点**：
- Rust 只输出 bridge lib（无 CLI）
- 原始事件原子插入 + 异步分析 + 知识消化（bounded batch + 人工保护）
- 多设备端到端加密同步（中继只过密文）
- Flutter UI 现代跨平台（desktop + mobile）

**模块 review 结果**：
1. **存储层（src/storage/）**：触发器保护极好，事件不可变性强
2. **AI 层（src/ai/）**：容错与预算控制优秀
3. **Wiki 知识库层（src/wiki.rs）**：消化流程完整，人工编辑保护到位
4. **Flutter UI 层（ui/）**：现代跨平台，但 Flutter 约束严格
5. **同步层（src/sync/）**：端到端加密实现已支持

**具体修复建议**（按优先级）：
- **P0**：修复 `events.rs` trigger + `evidence_count` 溢出
- **P1**：Flutter 包体积优化 + ListTile 约束修复
- **P2**：添加 `wiki_pages.evidence_snapshot_json` 列 + 新 schema 迁移
- **P3**：更新所有 roadmap 文件的 checkbox 并同步日期

**appflowy_editor 相关**：
- Core 部分属于 **AGPL-3.0** 许可（`lib/appflowy_editor.dart` + `lib/editor/` + `lib/plugins/`）
- 可大幅减少 AGAL-3.0 暴露范围（推荐替换 image/table/mention 组件）

---

**本 review 已保存至** `/data/playground/ai/elsewhen/elsewhen_review.md`

---

**需要我：**
- **A**：继续深入 review 某个模块（storage / wiki / ai / Flutter 具体文件）
- **B**：帮你写 PR 描述或测试用例
- **C**：直接执行某个修复（e.g. 修改 events.rs）
- **D**：把 review 结果更新到 README 或 CLAUDE.md

请直接回复 **A/B/C/D** 任意一个，我立刻执行。