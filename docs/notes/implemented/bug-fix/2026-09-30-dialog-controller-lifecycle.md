# Agent Note: 对话框控制器在退出动画期间被提前释放

Status: implemented

## Problem

在设置页打开 Provider 编辑对话框或知识库页的标签编辑框，按 Esc 关闭会抛：

```
A TextEditingController was used after being disposed.
Once you have called dispose() on a TextEditingController, it can no longer be used.
```

堆栈落在 `_AnimatedState.didUpdateWidget` → `_MergingListenable.addListener` →
`ChangeNotifier.addListener` → `debugAssertNotDisposed`。同源缺陷两处：

- `ui/lib/screens/settings_screen.dart` 的 `_openProviderEditor`：7 个控制器
- `ui/lib/widgets/wiki_page_detail_view.dart` 的 `_editTags`：1 个控制器

两处都是把 `TextEditingController` 建为**函数内的局部变量**，在 `await showDialog`
返回后立即 `dispose()`。第一处当时的注释写着：

> `// 对话框关闭后释放全部输入控制器（草稿值已在 pop 时读取完毕）。`

这个前提是错的。

## Decision

控制器改由对话框自身的 `State` 持有，在该 State 的 `dispose()` 中释放
（`_ProviderEditorDialog`、`_EditTagsDialog`）。生命周期与使用方对齐后，
释放时机由 Flutter 在真正卸载时决定，不再由调用方猜测。

`_SettingsScreenState._buildTextField` 与 `_buildDropdownField` 提为 `static`
供新对话框复用——两者只依赖 `AppTheme` 全局量，不触碰任何 State 字段，提为
static 不改变语义。

## 机制

`await showDialog` 在 `Navigator.pop` **被调用的那一刻**就返回，其 `Future` 完成；
而对话框的退出动画还要再跑约 200ms，其 widget 树仍然挂载。这段时间里任何重建
都会经由 `_AnimatedState.didUpdateWidget` 重新对 `widget.listenable`（一个包含该
controller 的 `_MergingListenable`）调用 `addListener`，此时 controller 已释放，
断言触发。

Esc 关闭触发概率最高，因为它同时发生三件事：失焦、键盘收起、route pop。
在退出动画进行中的那一帧，焦点变化本身就会引发重建。

`_editTags` 比 Provider 编辑更容易复现：它的输入框带 `autofocus: true`，
保证对话框一打开就发生一次聚焦重建，正落在动画窗口内。

## 范围确认

全库扫描了 30 余处 `showDialog`，确认只有这 2 处是「局部 controller + await 后
立即 dispose」的写法。其余 14 个文件（含 `todo_view.dart`、`goal_view.dart`、
`font_picker_dialog.dart`）都是正确模式：controller 为 `State` 字段，在该 State
的 `dispose()` 中释放。

## Alternatives considered

- **用 `addPostFrameCallback` 延后释放**：拒绝。回调只在下一帧执行，而退出动画
  横跨约 12 帧，仍会命中；且它把「等一帧」当成了「等动画结束」，后者没有对应关系。
- **用 `Future.delayed(Duration(milliseconds: 300))` 兜底**：拒绝。硬编码时长与
  动画时长耦合，动画时长一改就重新引入竞态，且这是把竞态变成定时器赌博。
- **删除 dispose，靠 GC 回收**：拒绝。`TextEditingController` 持有原生资源，
  不释放会泄漏；且这会让「忘记 dispose」变成一个不易察觉的问题。
- **让对话框内容改成无状态、每次 build 新建 controller**：拒绝。重建会丢输入
  光标位置与选区，且语义上每次 keystroke 都在丢弃状态。

## Consequences

Esc 关闭对话框不再抛异常，且 `flutter analyze` 对这两个文件零新增告警。

代价是两个对话框从内联闭包变成独立的 `StatefulWidget`，`showDialog` 的 builder
不再能就地读取表单值——改为点击「保存」时从 State 的字段构造 `_ProviderDraft`
弹出。这与原有行为一致（原本也是在 pop 时读取），但值的读取点从调用方移到了
对话框内部。

根因值得记下：这不是 Flutter 的问题，也不是竞态的偶发性，而是**注释断言了一个
未经验证的前提**（"pop 时值已读完"被顺理成章地推成"pop 后即可释放"），代码照着
错的前提写。注释解释了意图，但没验证意图成立。
