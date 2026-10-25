# NOTICE

## 项目许可证

elsewhen 本体（`src/`、`ui/lib/` 中除第三方 vendored 之外的代码）以
**GNU AGPL-3.0** 发布（仅此版本，`AGPL-3.0-only`），见根目录 [`LICENSE`](LICENSE)。该声明与
[`Cargo.toml`](Cargo.toml) 中的 `license = "AGPL-3.0-only"` 一致。

选用 AGPL-3.0 是因为本项目的编辑器底座 vendored 自 AppFlowy Editor
（见下节），其 copyleft 及网络服务源码可得性要求在本项目层面保持一致。

第三方组件与依赖各自的许可证以源文件 / 包元数据为准。要点如下：

## vendored 编辑器：ui/third_party/appflowy_editor

该目录 vendored 自上游 AppFlowy Editor，上游以 **AGPL-3.0 OR MPL-2.0**
双许可发布（见其目录内 [`LICENSE`](ui/third_party/appflowy_editor/LICENSE)）。

- 本项目选择 AGPL-3.0 分支，与项目整体许可一致；
- 修改过的 `ui/third_party/appflowy_editor/` 下文件继续满足
  AGPL-3.0 的源码可得性要求（整个仓库即源码，随分发提供）；
- 若有分发方只想单独复用该 vendored 目录，仍可按 MPL-2.0 分支执行。

## Rust / Dart 依赖

依赖包以宽松许可为主（MIT、Apache-2.0、BSD、Zlib、ISC、Unlicense 等）。
个别声明为「X OR Y」多选许可的包在本项目中选择宽松分支：

| 包 | 声明许可 | 本项目使用分支 |
|---|---|---|
| `self_cell` | Apache-2.0 OR GPL-2.0-only | Apache-2.0 |
| `r-efi` | MIT OR Apache-2.0 OR LGPL-2.1-or-later | MIT / Apache-2.0 |
| `option-ext` | MPL-2.0 | MPL-2.0（文件级弱 copyleft，无传染） |

如需重新核对依赖许可清单，可运行：

```bash
cargo metadata --format-version 1
flutter pub deps
```