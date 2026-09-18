# Elsewhen Personal Event System Roadmap

> 历史路线图：记录项目早期 MVP 分阶段设想。2026-09-17 起，产品实施顺序由
> [个人认知主循环 Roadmap](2026-09-17-personal-cognition-main-loop-roadmap.md) 接管；
> 本文定义的本地优先、Raw Event 不可变、AI 失败不阻塞记录等约束继续有效。

## Goal

验证“随手记录 → AI 理解 → 规则提醒 → 每日回顾”的核心价值闭环，并保持本地、轻量、可恢复。

## Phases

| Phase | 范围 | 退出证据 |
|---|---|---|
| 0 | Cargo workspace、配置、SQLite migrations、平台边界 | daemon 可启动，schema migration 可重复执行 |
| 1 | Linux daemon、双击快捷键、Slint Capture、Raw Event | 任意应用中双击 Ctrl 后可提交并可靠落库 |
| 2 | Durable queue、AI Provider、Analysis schema、retry | AI 不可用不影响记录；恢复后可重试 |
| 3 | Entity / Project resolution | 别名和低置信度不合并测试通过 |
| 4 | Rule Engine、Task、Attention、Clarification | 外部合作场景生成确定性任务/提醒 |
| 5 | Scheduler、Morning、Evening、notification | 早晚视图由事实动态生成并可重算 |
| 6 | Search / Timeline、reprocess CLI、跨平台 adapter | 单事件/全量重分析不改 Raw Event |
| 7 | Windows / macOS 打包与安装 | 平台验收、数据目录和快捷键行为一致 |

## Delivery gates

- 每个 Phase 独立可运行和验证；不跨阶段引入未使用框架。
- 任何 schema 或状态语义变更必须更新 FR、TDD、migration 和测试。
- Raw Event 不可变、AI 不直接写业务状态、AI 失败不阻塞 Capture 是全程硬门禁。
- MVP 完成前不做云同步、多用户、自动外部数据采集和复杂 Dashboard。

## Risks

- Linux global hotkey 权限和不同桌面环境行为可能不一致，需早期建立 platform adapter 测试。
- 过度提醒会损害留存；Hard Attention 必须由明确规则产生。
- AI 解析质量不足时，系统应退化为可靠记录，而不是扩大自动化权限。
