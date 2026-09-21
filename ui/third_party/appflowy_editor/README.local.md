# appflowy_editor (vendored, with local patch)

Vendored copy of **AppFlowy-IO/appflowy-editor** at commit
`01eccc6ee36bd07698bd80915289fe7070478cd2` (main, 2026-08-06).

Why vendored instead of a pub/git dependency:

1. **pub 版 6.2.0（2025-12）与 Flutter 3.47 不兼容**：`TextInputClient` 新增的
   `onFocusReceived()` 成员未实现（`delta_input_service.dart`），编译不过。
2. **Linux 中文输入法（IME）多字节合成 bug**：某些 Linux 引擎（fcitx/ibus）
   合成期间把光标放在合成区**起点**，上游只在 Windows 上做了归一化
   （`_normalizeComposingSelection`），导致第二个字起插入位置错乱。
   本地补丁将其扩展到 Linux 并在触发时打日志。

## 本地改动（相对上游）

`lib/src/editor/editor_component/service/ime/non_delta_input_service.dart`
- `_normalizeComposingSelection()`：`isWindows` → `isWindows || isLinux`，加 debug 日志。

## 维护提示

- 升级上游时请重新 diff `git log -p` 中该文件与上游的差异，别丢了这个补丁。
- 若上游正式修复（含 Linux 分支），可回退到 pub 依赖并删除本目录。

## 结构

保留发布所需最小集合：`lib/`、`pubspec.yaml`、`LICENSE`、`assets/`。
由 `ui/pubspec.yaml` 以 path 依赖引用。