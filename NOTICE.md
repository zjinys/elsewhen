# NOTICE

## 项目许可证

elsewhen 本体（`src/`、`ui/lib/` 中除第三方 vendored 之外的代码）以
**MIT License** 发布，见根目录 [`LICENSE`](LICENSE)。该声明与
[`Cargo.toml`](Cargo.toml) 中的 `license = "MIT"` 一致。

第三方组件与依赖各自的许可证以源文件 / 包元数据为准。要点如下：

## vendored 编辑器：ui/third_party/appflowy_editor

该目录 vendored 自上游 AppFlowy Editor，上游以 **AGPL-3.0 OR MPL-2.0**
双许可发布（见其目录内 [`LICENSE`](ui/third_party/appflowy_editor/LICENSE)）。

本项目的 **本地补丁与新加入代码选择 MPL-2.0 条款**：

- MPL-2.0 为文件级（弱）copyleft，仅约束被修改的 MPL 文件本身；
- 修改过的 `ui/third_party/appflowy_editor/` 下文件继续以 MPL-2.0 发布，
  源码随本仓库提供，满足 MPL-2.0 的源码可得性要求；
- 项目其余部分不受影响，可继续采用 MIT。

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