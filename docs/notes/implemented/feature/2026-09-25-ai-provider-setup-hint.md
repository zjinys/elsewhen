# 首次运行 AI Provider 配置引导横幅

日期：2026-09-25
类型：feature
任务：系统首次运行（未配置 AI Provider）时提示用户去设置界面配置。

## 结论

主屏（MainScreen）标题栏下方新增一条可关闭的引导横幅，未配置任何
AI Provider 时出现，配好或手动关闭后消失。

## 设计决策

- **判定依据**：`listAiProviderConfigs()` 非空。Rust 运行时在零配置时会直接
  bail（`No active AI provider configuration`，src/ai/conversation.rs），
  AI 对话与自动分析全部不可用——所以「配置列表非空」就是就绪判据。
- **横幅而非对话框**：对话框要持久化「已看过」标记（写 app_meta），且
  首次之后仍可能出现新用户场景；横幅零持久化——探测为真就消失，
  探测为假就自然在（不依赖任何「见过」状态），语义更贴「引导配置」。
- **fail-safe**：探测进行中 / 查询失败一律不显示，避免桥未就绪时闪横幅。
- **刷新链路**：三个入口（横幅「去配置」、MainScreen 齿轮、时间线页齿轮）
  从设置页返回后 `ref.invalidate(aiProviderConfiguredProvider)` 重新探测，
  配置完成返回主屏横幅立即消失。

## 实现

- `lib/providers/app_provider.dart`：新增 `aiProviderConfiguredProvider`
  （FutureProvider<bool>，bridge 查询配置列表非空）。
- `lib/widgets/ai_provider_setup_hint.dart`：横幅本体（ConsumerStatefulWidget，
  局部 `_dismissed` 只管手动关闭，不跨会话）。
- `lib/screens/main_screen.dart`：标题栏下挂横幅；齿轮 onPressed 改 async，
  返回后 invalidate 探测源。
- `lib/screens/conversation_timeline_screen.dart`：齿轮同款 invalidate。
- `test/ai_setup_hint_test.dart`：4 条（未配置显示 / 已配置隐藏 / 关闭消失 /
  去配置进设置页），全部 provider override 隔离，不依赖真桥。

## 验证

analyze 0 error；测试 +159 -11（基线 +155 之上新增 4 条全过），失败名单
md5 与此前基线逐题一致；flutter build linux --debug 通过。
改动未提交（等并行提交批次落库）。