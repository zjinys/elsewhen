# Agent Note: code 块降级只读组件（不升级 vendor）

Status: implemented

## Problem

正文 markdown 常含代码块（M2 起 `CodeBlockNodeParser` 保证编解码保真）。但 vendor `appflowy_editor`（commit 01eccc6）**没有 `code` 块组件**——`code` 节点在编辑器里渲染成 30px 高的 placeholder 占位框，只读走查实测。不处理的话所有知识页代码块在 GUI 里都是空白占位。

## Decision

注册自定义降级块 `WikiCodeBlockComponent`（`ui/lib/wiki/wiki_code_block.dart`），加入 `WikiContentEditor` 与生产注册集：等宽字体只读文本 + 语言角标（取自 code 节点 `language` 属性）+ 「复制」按钮（`Clipboard.setData` 写代码全文，去围栏与语言行）；配色随 `AppTheme` 深浅主题。选择**不升级 vendor**——升级会引入上游 code 块组件的体积、快捷键与行为漂移，降级组件只负责展示与复制。

## Alternatives considered

- 升级 vendor 到带 code 块的版本：行为漂移 + 体积增加，且上游 01eccc6 是本地固定基线，弃。
- 保持 placeholder 占位：代码内容不可见，知识页展示严重降级，弃。
- 让 code 块可编辑：降级组件只读，编辑 code 内容不属于 v1 正文编辑范围，弃。

## Consequences

知识页代码块以等宽格式可读、可一键复制；复制走系统剪贴板经集成测试断言。code 块的 markdown 往返保真已有常驻测试，展示层不再阻断。若未来升级 vendor 获得原生 code 块，可移除降级组件——`WikiCodeBlockKeys.type` 注册集是唯一接线点。