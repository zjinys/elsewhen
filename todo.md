# TODO

- [x] 完成 Phase 4B 实体确认与来源闭环：候选确认/忽略、别名与同名消歧、冲突纠正、安全合并/撤销、来源和关联待办一致性（见 `docs/notes/implemented/feature/2026-09-21-safe-entity-merge-and-split.md`）
- [x] 落地「事件可记录性过滤」第一版：分析后过滤、消息内人工纠正、存量重新分析和来源保留（写入前严格门禁暂不默认启用）：docs/notes/implemented/feature/2026-09-21-recordability-correction-loop.md
- [x] 落地「归档对话可删除」方案：docs/notes/proposed/architecture/2026-09-19-archived-conversation-deletion.md
- [ ] 落地「决策辅助闭环」定位补全方案：docs/notes/proposed/architecture/2026-09-19-decision-support-loop.md
- [ ] 实现 Today 每日回顾工作台：docs/notes/proposed/product/2026-09-20-today-daily-review-workbench.md（当前暂缓，不先上线空壳页面）
- [x] Rust edition 与 style edition 升级到 2024：docs/notes/implemented/process/2026-10-01-rust-edition-2024-migration.md（2026-10-03：全仓格式迁移完成；`cargo check --all-targets` / `cargo fmt --check` / release 构建通过；测试命令排除回环代理后 452 个单元测试 + 5 个配置集成测试通过，10 项按原设置忽略。本机 Rust/Cargo 已为 1.99.0，本次不更新全局 rustup）
- [x] 落地「单流对话 + 显式主题入口」基础信息架构：docs/notes/implemented/feature/2026-09-21-single-stream-topic-implementation.md（唯一主对话、日期/关键词定位、显式主题入口、页内聊天隔离与确认式沉淀；证据 `d6ca18d`）
- [ ] 完成单流主题后续演进：docs/notes/proposed/product/2026-10-05-single-stream-topic-follow-ups.md（主题生命周期与可追溯排序、分析队列摘要、同名候选确认、AI 建议归拢）
