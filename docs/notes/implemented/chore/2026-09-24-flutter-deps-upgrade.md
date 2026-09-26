# 2026-09-24 Flutter 依赖升级：file_picker 13 / freezed 栈 / 传递依赖

## 结论
- **file_picker 10 → 13**：完成。v13 是重写版（静态方法 API、取消返回空列表、
  `PlatformFile` 抽象化、字节改 `readAsBytes()` 按需读、`saveFile` 语义变为
  传字节代存）。app 两处调用点（message_area / wiki_page_detail_view 的
  `FilePicker.platform.getDirectoryPath` → `FilePicker.getDirectoryPath`）+
  vendored 编辑器两个文件适配（详见 third_party/appflowy_editor/README.local.md）。
- **freezed_annotation 2→3 / freezed 2→4 / build_runner 2.5→2.16**：完成。
  纯版本号（lib/ 无 freezed 使用，无 .g.dart/.freezed.dart，不占编译路径）。
- **传递依赖**：`flutter pub upgrade` 收敛 35 个（win32 5→6、dbus、
  device_info_plus 13 等，随 file_picker 13 链路解锁）。
- **flex_color_scheme 停留 8.4.0**（已是 8.x 最新）：9.0.0 基于新 material_ui
  包重构，FlexThemeData 返回 material_ui.ThemeData，与 Flutter 3.47 自带
  material 的 ThemeData 是互不相干的类型，全app 主题赋值全灭——
  等 Flutter material 模块化生态稳定后再升。pubspec 已留注释。
- **flutter_riverpod 2.6.1 → 3.4.3**：已完成（见下节）。
- **flutter_rust_bridge 不动**：pin 2.14.0-beta.2 已领先 pub 最新稳定 2.13.0。
- intl / markdown / nativeapi / flutter_svg / url_launcher / provider /
  collection / flutter_lints / ffigen 均已在最新。

## vendored 编辑器联动（冲突链解法）
file_picker 13 → windows_file_picker → win32 ^6，与编辑器锁的
device_info_plus ^12 → win32 ^5 冲突。编辑器实际只用稳定 API，
放宽两个约束解决（pubspec 改 `>=x <y`，README.local.md 留痕）：
- file_picker: `>=10.3.10 <14.0.0`
- device_info_plus: `>=12.3.0 <14.0.0`

## Riverpod 2 → 3 迁移（用户明确要求）

官方迁移指南逐条核对后的实际命中面（用法盘点：39 文件用 riverpod，
但 0 处 `Ref<T>` 泛型 / 0 AutoDispose 接口 / 0 `.autoDispose` /
0 FamilyNotifier / 0 ProviderObserver）：

1. **legacy 入口**：`StateProvider`/`StateNotifierProvider`/`StateController`
   移入 `flutter_riverpod/legacy.dart`——4 个 provider 文件
   （event/conversation/wiki/settings）+ wiki_page_detail_view.dart 加 import，
   API 本身不变。
2. **`AsyncValue.valueOrNull` 移除 → `.value`**（v3 的 `.value` 出错返回 null，
   语义即旧 valueOrNull）：4 文件 24 处批量改名，全部位于
   `ref.watch(...)` 链上，无非 AsyncValue 误伤。
3. **自动重试（新默认行为）关闭**：v3 对失败 provider 指数退避重试；
   本 app 失败多为确定性（桥/DB 错误），重试只会刷日志且改变启动失败
   UX。`ProviderScope(retry: (c, e) => null)` 保持 v2 失败即停
   （main.dart + main_timeline_example.dart 两处）。
4. **审计后无需改动的项**：
   - ref-after-await 唯一命中 appInitializationProvider（app 期存活根
     provider，dispose 不可能发生在 init 中途）；
   - 7 处 `.future` 读取均为泛型 catch / 无 catch，不受 ProviderException
     包装影响；
   - == 过滤统一、离屏暂停（TickerMode）均为低险行为改进，接受默认。

验证：analyze 0 error；测试 +155 -11 与迁移前**逐题一致**；
flutter build linux --debug 通过。

## StateProvider/StateNotifier → Notifier 现代化（第二轮，用户明确要求）

legacy 兼容层整体移除，代码面 StateProvider/StateNotifier/StateController
引用清零：

- 新增 `lib/providers/state_holder.dart`：`StateHolder<T> extends Notifier<T>`，
  提供 `set(v)` / `update(f)` / `isMounted`。13 个 StateProvider 全部改为
  `NotifierProvider<StateHolder<T>, T>(() => StateHolder(初始值))`。
- `SettingsNotifier`：StateNotifier → Notifier（`build()` 返回
  `AppSettings.defaults()`，`_repo` 改 late final 从 ref 读），
  业务方法零改动。
- 调用点机械迁移 32 处：`.notifier).state = x` → `.notifier).set(x)`
  （含多行表达式，括号平衡脚本处理）；`.state++` → `.update((v) => v+1)`；
  测试 override `overrideWith((ref) => v)` → `overrideWith(() => StateHolder(v))`
  （9 个测试文件）。
- 写后读 helpers（addConversationNotice 等 3 个）收敛为单次 `update(...)`
  原子改写（顺带消掉「读-改-写」两步之间的竞态窗口）。
- **v3 行为差异实录**：销毁后写 state 的异常从 StateError（legacy
  StateController）变为 UnmountedRefException，且该类型在 riverpod 内部库
  （`src/internals.dart` "DO NOT USE"），不可捕获——wiki_page_detail_view
  的 dispose 注销回调改用官方 `ref.mounted` 守卫（经 StateHolder.isMounted
  暴露）。这是迁移中唯一一处真实行为差异，由 wiki_relations_ui_test 暴露。
- **补漏（9-25，analyze 抓出）**：两处「缓存 notifier 直读直写」漏网——message_area
  与 wiki_ai_chat_panel 的 finally 块用 `generatingNotifier.state = ...` 移除
  生成中集合；v3 下 state setter 受保护（编译 warning + 运行时风险）。改为缓存
  notifier 的 `update((c) => ...)` 原子改写，语义不变。

验证：analyze 0 error（touched 文件 0 新增 info）；测试 +155 -11，
失败名单与现代化前**逐题一致**；flutter build linux --debug 通过。
## 验证

- analyze 0 error（81 issue 基线 → 82，+1 为 legacy 兼容层的 info 提示）。
- flutter build linux --debug 通过。
- 测试 +155 -11：其中 6 个为既有基线失败；新增 5 个全部位于并行会话
  正在开发的功能面（字体系统重构 / wiki 面板重构 / 聊天块），
  断言均为 UI 内容期望（'AI对话' 面板、'模拟回复' 气泡、字体族名），
  不触达本次升级的任何包。Riverpod 3 迁移后失败名单**逐题不变**。

## 事故记录（并发工作树教训）
升级中途用 `git stash` 取基线对比，把并行会话未提交的工作一并 stash，
pop 时与其活改动冲突。恢复方式：逐文件 `git checkout stash@{0} -- <path>`
提取（冲突区保留工作区活版本），校验 28 个文件全部落位后 drop stash。
教训：共享工作树下禁止整体 stash；基线对比改用 `git worktree add` 隔离副本。
