# Agent Note: 编辑器窄屏溢出修复（vendor 默认桌面 padding）

Status: implemented

## Problem

移动端走查（M4，390×844 视口专项测试）抓出 `RenderFlex overflowed by 106 pixels`：wiki 聊天面板在窄视口只剩 142px 宽。根因是 `EditorStyle.desktop` 默认 `padding = EdgeInsets.symmetric(horizontal: 100)`——块内容左右各扣 100px，宽屏阅读栏（760px）内仍够用，窄屏 390 − 48(页边) − 200 = 142px 直接压垮，聊天面板头部 Row 溢出。

## Decision

`WikiContentEditor` 显式传 `EditorStyle.desktop(padding: EdgeInsets.symmetric(horizontal: AppTheme.space6))`，把块级 padding 收窄到左右各 24px。宽屏仍有外层阅读栏（`_kReadingMaxWidth`）兜底，窄屏不再溢出。新增移动端走查集成测试常驻验证（窄视口不溢出 + 编辑器/卡座/聊天面板可用 + 编辑保存正常）。

## Alternatives considered

- 保留 vendor 默认 100px：窄屏必然溢出，弃。
- 改 vendor 默认值：影响第三方全局、破坏本地基线一致性，弃。
- 只在窄屏条件放宽（LayoutBuilder 分支）：编辑器不感知外层视口宽度，且复杂化，弃。

## Consequences

窄屏下聊天面板恢复 294px 可用宽度、无横向溢出；宽屏行宽由阅读栏主导不受影响。此改动是行为修复而非 vendor 升级——vendor `EditorStyle.desktop` 默认 100px padding 是上游基线，不在本仓库修改。