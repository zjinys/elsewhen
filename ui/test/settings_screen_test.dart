import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:elsewhen_ui/bridge/rust_bridge_repository.dart';
import 'package:elsewhen_ui/data/storage_repository.dart';
import 'package:elsewhen_ui/bridge/api.dart' as api;
import 'package:elsewhen_ui/providers/settings_provider.dart';
import 'package:elsewhen_ui/screens/settings_screen.dart';

import 'support/isolated_bridge.dart';

/// 设置页行为验证（真实 Rust 桥接）。
/// 同一 isolate 内 RustLib.init 只能初始化一次，故所有用例共享一个 repo 实例。
void main() {
  late RustBridgeRepository repo;

  setUpAll(() async {
    repo = await createIsolatedBridge();
  });

  testWidgets('settings screen prefills real provider from bridge', (
    tester,
  ) async {
    await tester.runAsync(
      () => repo.saveAiProviderConfig(
        api.AiProviderConfigDto(
          id: '',
          name: '隔离测试 Provider',
          providerType: 'openai-compatible',
          baseUrl: 'https://isolated.example.com/v1',
          model: 'isolated-test-model',
          apiKeySource: '',
          apiKey: 'sk-isolated-test',
          isActive: true,
          temperature: 0.7,
          maxTokens: null,
        ),
      ),
    );

    // 真实桥接读取当前生效的 provider（demo 副本里有 hub.oaifree.com / gpt-4o）
    final expected = await tester.runAsync(() => repo.getAiProviderConfig());
    expect(expected, isNotNull, reason: '测试库副本应含 provider 配置');
    final expectedUrl = expected!.baseUrl;
    final expectedModel = expected.model;

    await tester.pumpWidget(
      ProviderScope(
        overrides: [storageRepositoryProvider.overrideWithValue(repo)],
        child: const MaterialApp(home: SettingsScreen()),
      ),
    );

    // 等 post-frame 的 _loadProviders 跑完真实 FFI
    await tester.runAsync(
      () => Future<void>.delayed(const Duration(milliseconds: 400)),
    );
    await tester.pump();

    // 模型 tab（默认）：AI provider 是卡片列表（多配置 + 单激活），
    // 活跃卡片应展示库中的真实配置，而非硬编码默认值（openai.com / gpt-3.5-turbo）。
    expect(find.text('添加配置'), findsOneWidget, reason: '多 provider 入口按钮');
    expect(find.text('激活中'), findsOneWidget, reason: '库中配置应处于激活状态');
    // 卡片上展示真实 baseUrl / model
    expect(
      find.textContaining(expectedUrl),
      findsWidgets,
      reason: 'baseUrl 应预填真实配置而非默认值',
    );
    expect(
      find.textContaining(expectedModel),
      findsWidgets,
      reason: 'model 应预填真实配置而非默认值',
    );
    expect(
      find.textContaining('api.openai.com/v1'),
      findsNothing,
      reason: '不应出现硬编码默认 baseUrl',
    );
    expect(
      find.textContaining('gpt-3.5-turbo'),
      findsNothing,
      reason: '不应出现硬编码默认 model',
    );

    // 记忆策略默认「简单记忆」：模型 tab 仅有 1 个文本输入（最大消息数），
    // AI provider 字段移入「编辑」对话框，不直接铺在页面上。
    expect(
      tester.widgetList<TextField>(find.byType(TextField)).length,
      1,
      reason: '模型 tab：provider 为卡片布局，仅记忆策略有文本输入',
    );

    // 切到「数据」tab 应看到每日 Token 使用区块（纯展示，不依赖是否有记录）
    await tester.tap(find.text('数据'));
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 300));
    expect(find.text('每日 Token 使用'), findsOneWidget);
    expect(find.text('事件分析队列'), findsOneWidget);
    // 「数据」tab 上有两个队列区块（事件分析队列 / 知识消化），两者都有「待处理」
    // 「已完成」「失败」三枚同名指标，故这三条按 2 枚断言；下面几条是事件分析
    // 队列独有的指标，仍要求恰好一枚。
    expect(find.text('待处理'), findsNWidgets(2));
    expect(find.text('已完成'), findsNWidgets(2));
    expect(find.text('失败'), findsNWidgets(2));
    expect(find.text('处理中'), findsOneWidget);
    expect(find.text('等待重试'), findsOneWidget);
    // 知识消化区块存在且读到了状态（读失败会显示「知识消化状态读取失败」）
    expect(find.text('知识消化'), findsOneWidget);
    expect(find.text('知识消化状态读取失败'), findsNothing);
    expect(find.text('跳过'), findsOneWidget);
  });

  testWidgets('外观 tab：正文字号滑块存在并驱动设置状态', (tester) async {
    final container = ProviderContainer(
      overrides: [storageRepositoryProvider.overrideWithValue(repo)],
    );
    addTearDown(container.dispose);
    await tester.pumpWidget(
      UncontrolledProviderScope(
        container: container,
        child: const MaterialApp(home: SettingsScreen()),
      ),
    );
    await tester.pump();

    // 切到「外观」tab
    await tester.tap(find.text('外观'));
    await tester.pumpAndSettle();

    expect(find.text('正文字号'), findsOneWidget);
    expect(find.byType(Slider), findsOneWidget, reason: '外观区应有字号滑块');
    // 默认 16（AppFonts.defaultFontSize）
    expect(container.read(settingsProvider).fontSize, 16);

    // 拖到最右 → divisions 吸附到最大值 24，状态实时更新
    await tester.drag(find.byType(Slider), const Offset(500, 0));
    await tester.pumpAndSettle();
    expect(container.read(settingsProvider).fontSize, 24);
    expect(find.text('24 pt'), findsOneWidget, reason: '滑块右侧应显示 24 pt');
  });

  testWidgets('读取失败时展示失败原因，而不是伪装成空状态', (tester) async {
    final container = ProviderContainer(
      overrides: [
        storageRepositoryProvider.overrideWithValue(_FailingRepository()),
      ],
    );
    addTearDown(container.dispose);
    await tester.pumpWidget(
      UncontrolledProviderScope(
        container: container,
        child: const MaterialApp(home: SettingsScreen()),
      ),
    );
    await tester.pump();

    await tester.tap(find.text('数据'));
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 300));

    // 失败原因要露出来
    expect(
      find.textContaining('每日 Token 用量读取失败：'),
      findsOneWidget,
      reason: 'Token 用量区块应展示失败原因而非「暂无 AI 调用记录」',
    );
    expect(
      find.textContaining('规则库读取失败：'),
      findsOneWidget,
      reason: '规则库区块应展示失败原因而非「还没有规则」',
    );
    // 空状态文案绝不能出现：把故障说成「没数据」比直接报错更误导
    expect(find.textContaining('暂无 AI 调用记录'), findsNothing);
    expect(find.textContaining('还没有规则'), findsNothing);
  });
}

/// 全部方法都抛的假仓库。
///
/// 设置页内部把仓库硬转 `RustBridgeRepository`，所以这个假实现会在 cast 处就抛
/// TypeError——但走的仍是同一批 catch 与渲染分支，正是用例要锁定的行为。
class _FailingRepository implements StorageRepository {
  @override
  dynamic noSuchMethod(Invocation invocation) {
    throw StateError('模拟读取失败');
  }
}
