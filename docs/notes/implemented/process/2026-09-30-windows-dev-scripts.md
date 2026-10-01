# Agent Note: Windows 开发脚本

Status: implemented

## Problem

`scripts/` 下原本只有 bash 脚本，唯一的 Windows 入口是 `package-msi.ps1`，而它只负责打包。
Windows 上做日常开发（跑起来、带诊断日志启动、重建 frb bridge、验证集成、打 APK、修旧库）
必须逐个手工翻译命令，而 `regen.sh` 那套 CPATH 与 cc/cxx shim 的前置条件在 Windows 上
并不显然成立。

CLAUDE.md 声明 CI 已在 Windows 上运行（`ci.yml`），说明 Windows 是受支持平台，开发循环
却没有对应的可执行入口——这是支持声明与工具链之间的缺口。

## Decision

为 7 个 `.sh` 各提供一个同名 `.ps1`，保持一一对应：

| `.sh` | `.ps1` | 关键移植点 |
|---|---|---|
| `elsewhen.sh` | `elsewhen.ps1` | `-d linux` → `-d windows`；保留 `--target` 必带的警告 |
| `elsewhen-capture.sh` | `elsewhen-capture.ps1` | 同上 |
| `debug-run.sh` | `debug-run.ps1` | 数据目录改 `%LOCALAPPDATA%`；`stat -c %s` → `(Get-Item).Length`；`tee -a` → `Tee-Object -Append` |
| `test_bridge.sh` | `test_bridge.ps1` | `libelsewhen.so` → `elsewhen.dll`；`ps aux` → `Get-Process`；数据库路径改为遵循 `ELSEWHEN_DATA_DIR` |
| `build-apk.sh` + `build-android-rust.sh` | `build-apk.ps1` | NDK host tag → `windows-x86_64`；NDK 版本排序改用 `[version]`（字符串序会把 `9.0.0` 排到 `25.2.9519653` 前） |
| `stamp-legacy-db.sh` | `stamp-legacy-db.ps1` | 结构指纹比对逻辑一字未改；`diff -u` → `git diff --no-index` |
| `regen.sh` | `regen.ps1` | 见下 |

Flutter 解析顺序统一为 `ui\.fvm\flutter_sdk\bin\flutter.bat` → 全局 `fvm` → 全局
`flutter`，与既有的 `package-msi.ps1` 一致。

### 全部 8 个 ps1 加 UTF-8 BOM

`package-msi.ps1` 及新增的 7 个原先都是无 BOM 的 UTF-8。Windows PowerShell 5.1
（`powershell.exe` 默认版本）把无 BOM 文件按系统 ANSI 代码页读取，中文注释与
`Write-Host` 输出会整体乱码。8 个文件现已统一带 BOM，PowerShell 5.1 与 7 均可正确读取。

这是对既有文件的修复，不是新脚本的约定。

### regen.ps1 与 regen.sh 的三处有意分歧

**libclang.dll 硬依赖前置检查。** 查 ffigen 22.0.0 源码（`pubspec.lock` 锁定的版本）
确认它在 Windows 上的 libclang **默认**查找范围只有两处：

- `ffigen/lib/src/strings.dart:254` → `C:\Program Files\LLVM\bin\`
- `ffigen/lib/src/config_provider/spec_utils.dart:465-474` →
  `%USERPROFILE%\scoop\apps\llvm\current\bin`

**不做 PATH 搜索，`LIBCLANG_PATH` 也无效**（ffigen 的 `lib/` 全量 grep 零命中）。

> ⚠ **勘误（2026-09-30）。** 本节初版写的是「没有任何入口可改，LLVM 必须装在上述
> 两个位置之一」，论据是 `spec_utils.dart:438` 遍历的 `libclangOverridePaths` 来自
> `overrideable_utils.dart:43` 的 `const <String>[]`。**这个推论是错的**——
> `libclangOverridePaths` 的注释原文是 "Downstream **clones** can use a non-null value"，
> 它是给 fork ffigen 的人用的钩子，不是使用者配置项；我把「没有 fork 级覆盖」错当成
> 「没有用户级入口」。
>
> 用户级入口是有的：ffigen 有一等配置项 `llvm-path`（`strings.dart:24`），且优先级
> **高于**默认位置（`yaml_config.dart:335-340`：配了走 `llvmPathExtractor`，全不命中才
> 回落默认；没配才直接走默认）。命令行 `--llvm-path`、配置 `llvm_path:` 两个入口都有。
> 路径拼接为 `<你给的路径>\bin\libclang.dll`（`strings.dart:27`，
> `dynamicLibParentName` 在 Windows 上是 `bin`），也接受 `libclang.dll` 完整路径
> （`spec_utils.dart:545-551`）。
>
> 该勘误同时暴露了 `regen.ps1` 第 0 步的一个真实缺陷：预检硬编码那两条路径且不可
> 绕过，用户若把 LLVM 装在别处并已正确配置 `llvm_path`，预检会**误报「没装」**。
> 已修：新增 `-LlvmPath`，指定时只校验该路径并透传 `--llvm-path`，不指定才复查默认两条。

由此得出两条与直觉相反、但对使用方式影响很大的结论：

1. **Visual Studio 自带的 clang-cl 不能替代。** 那是 `clang-cl.exe` / `clang.exe`
   两个**驱动**，位置在 `...\VC\Tools\Llvm\`，既不提供 `libclang.dll`，也不在
   ffigen 的搜索路径里。必须另装 llvm.org 官方安装器或 `scoop install llvm`。
2. **这与 MSVC Build Tools 是两件独立的事。** MSVC 提供 cargo 链接 `.dll` 所需的
   链接器，LLVM 提供 ffigen 的解析引擎，互不替代。`cargo build --release` 能成功
   不代表 ffigen 能用，反之亦然。

`regen.ps1` 把这项检查放在第 0 步并复述 ffigen 的两条默认路径：缺 libclang 时 codegen 会在
完全无关的位置报错，用户很难联想到是 LLVM 没装。

**clang 头文件改为探测而非硬失败。** `regen.sh` 第 1 步源于一个 Linux 专属问题：
ffigen 找不到 `stdbool.h`，把 cbindgen 生成的 `typedef bool (*DartPostCObjectFnType)(...)`
解析成 `typedef bool = ffi.NativeFunction<...>`，遮蔽 Dart 的 `bool` 类型。
该缺失是 Debian/Ubuntu 的 clang 打包问题，官方 LLVM 安装器的 `libclang.dll` 自带
编译期 resource dir 路径、会自行找到 `lib\clang\<ver>\include`，Windows 上通常不复现。
所以 `regen.ps1` 在探测不到头文件时打印提示并继续，而不是 `exit 1`。

同时**不实现 cc/cxx shim**：cargo/cc-rs 对非 `.exe` 的 `CC` 有引号处理差异，这层封装
在 Windows 上比 POSIX 脆弱得多；且 `CPATH` 是 gcc/clang 的搜索路径变量，MSVC 的
`cl.exe` 根本不读它，`regen.sh` 那个「CPATH 污染 cc 编译」的问题在 Windows 上无对应物。
`-ForceClangInclude` 可强制启用探测。

**新增 codegen hash 静态校验。** `regen.sh` 仅靠「脚本最后一步总是重编」来保证
Dart 侧 `rustContentHash` 与 Rust 侧 `FLUTTER_RUST_BRIDGE_CODEGEN_CONTENT_HASH` 一致；
若 codegen 因工具版本不匹配而静默失效，会安静地产出一对不匹配的库，直到 app 启动时
`RustLib.init()` 抛 hash mismatch。`regen.ps1` 在收尾处直接正则读取两个源文件比对，
不一致就地抛错并说明处置。

## Alternatives considered

- **给全部 15 个脚本都写 Windows 版**：拒绝。`package-{deb,rpm,pacman,appimage,dmg}.sh`
  与 `install-linux-desktop.sh` 平台绑定，Windows 上本就不该存在；`flutter-wrapper.sh`
  硬编码了 `/home/pp/.fvm/versions/3.47.4` 某一台机器的路径，移植它等于把个人环境
  噪声复制一份。
- **用 Makefile 统一跨平台**：拒绝。根 `Makefile` 声明 `SHELL := /bin/sh`，在 Windows 上
  本就不可用；为它改造等于重写整个构建入口，远超需求。改为在 CLAUDE.md 里给出
  `.sh` / `.ps1` 对照表。
- **把 `regen.ps1` 的 clang 步骤照抄 `regen.sh`（缺就报错）**：拒绝。Windows 上大概率
  探测不到，那是必然失败的假阳性。改成探测到才启用，并在注释里写明该段可删。
- **只写 `regen.ps1` 一个**：拒绝。regen 是最痛的，但日常最高频的是启动和带日志启动。

## 未移植清单（有意为之）

`package-deb.sh` / `package-rpm.sh` / `package-pacman.sh` / `package-appimage.sh` /
`package-dmg.sh` / `install-linux-desktop.sh` / `flutter-wrapper.sh`，
以及根 `Makefile`（`/bin/sh`）。理由见上。

## Verification

**已验证：** 下载 PowerShell 7.4.6 linux-x64，用
`[System.Management.Automation.Language.Parser]::ParseFile` 解析 8 个脚本，全部通过；
以既有的 `package-msi.ps1` 作为对照确认解析器工作正常。PSScriptAnalyzer
（Error + Warning 级）初次运行报出 2 个真问题，均已修：

1. `PSUseBOMForUnicodeEncodedFile` × 8 —— 促成上面的 BOM 修复。
2. `PSUseDeclaredVarsMoreThanAssignments`（`regen.ps1` 的 `$shimDir`）——
   初稿遗留的死变量（最终决定不建 shim），已删。

复检结果：Error 0 findings，Warning 仅剩 `PSAvoidUsingWriteHost`（72 处）。
该规则针对非交互宿主，而这些是交互式构建脚本，且与既有 `package-msi.ps1` 的写法一致，
故保留。

libclang 前置检查的**逻辑**也做了隔离执行验证（把两条搜索路径参数化后在 Linux 上跑
4 个用例：USERPROFILE 未设 / 为空串 / 已设但无 libclang / scoop 路径命中）。这一步抓到
了 3 个纯靠阅读发现不了的缺陷：

1. **`Join-Path $null` 会终止脚本。** `USERPROFILE` 未设时抛
   `ParameterBindingValidationException`，在 `$ErrorActionPreference = "Stop"` 下
   中断，用户看到的是「Cannot bind argument to parameter 'Path'」而非可执行的处置。
   ffigen 自己判了 `userHome != null` 才 join（`spec_utils.dart:466`），照搬时漏了这个守卫。
2. **`Test-Path` 遇到不存在的驱动器会抛**「Cannot find drive」而非返回 false。
   改用 `[System.IO.File]::Exists`——.NET 版本内部吞掉全部 IO 错误、只返回 false，
   语义正好是这里要的「这个文件在不在」。
3. **`Where-Object` 只命中一条时返回标量 String**，此时 `$x[0]` 取到的是**首字符**而非
   路径（实测得到 `"C"`）——`@()` 包裹强制数组语义。而单机上只有一处 LLVM 恰恰是
   最常见情况，等于这条诊断信息在最需要它的场景下必然失效。判断逻辑 `-not $x` 不受影响，
   错的只有输出。

这三个都靠「读代码看不出来、必须真跑」才暴露，也说明第 0 步这种看起来只有 3 行的
前置检查同样需要执行验证。

**勘误修复后又抓到一个同类缺陷（2026-09-30）。** 加 `-LlvmPath` 时我第一版按
「有扩展名 → 当文件名；否则 → 当根目录」二分，**没有照抄 ffigen 的真实判定**。
读 `spec_utils.dart:534-552` 才发现 ffigen 的顺序与条件是：

```dart
for (final path in value) {
  final dylibPath = findLibclangDylib(p.join(path, dynamicLibParentName));  // ① 先试根目录
  if (dylibPath != null) return dylibPath;
  if (p.extension(path).isNotEmpty && File(path).existsSync()) return path;  // ② 再试完整路径
}
```

我的二分只看扩展名，于是 `D:\tools\llvm-18.1` 这类**带版本号的根目录**会被误判成
文件名，直接放弃 `…\bin\libclang.dll` 的查找 → 明明装对了却报「找不到 libclang.dll」。
ffigen 自己不踩这个坑，是因为它第二个条件还要求**文件确实存在**。修正后按 ffigen 的
顺序与条件实现，并在 Linux 上用 `plain` / `llvm-18.1` / `llvm.18.1` 三种目录名
（新旧逻辑对照）实测确认：旧逻辑在带版本号的两种情形下 `Exists=False` 误报失败，
新逻辑正确解析到 `<根目录>\bin\libclang.dll`。

教训与前三条一致：**转写别人的判定逻辑时，要连"顺序"和"多个条件的合取"一起抄**，
只抄其中一个条件就会引入对方本来没有的缺陷。

**PowerShell 7 会把未捕获异常里的多行提示压成一行（2026-10-01）。** 在 Linux 上
实跑 `regen.ps1` 时发现，第 0 步那两段用 `throw @"..."` 抛出的排版提示，在
PowerShell 7 下全部塌成一堵字墙：

```
C:\Program Files\LLVM\bin\   %USERPROFILE%\scoop\apps\llvm\current\bin
它不做 PATH 搜索...两条出路：    1) 装到默认位置   2) 装在别处
```

原因是 PS 7 对**未捕获**异常的默认视图 `ConciseView` 会折叠连续空白；PS 5.1 的
`NormalView` 不折叠，所以这个坑只在 7+ 暴露——5.1 用户完全看不到，7 用户看到的是
完全读不懂的提示，而脚本注释还写着「让用户看到可执行的处置」。

对照实测确认了两种写法的差异：多行内容被 `try/catch` 捕获后打印时换行是保留的，
**只有未捕获时**才塌。所以不是 here-string 本身的问题。

已加 `Stop-WithGuidance` 辅助函数：`Write-Host` 逐行输出提示，`throw` 只带单行摘要。
两个版本都正常。验证了退出码仍为 1（CI 依赖非零）、`finally` 的 `Pop-Location`
仍生效（无目录泄漏）。同批把 L262 那处用反引号拼接的多行 `throw` 也一并改了。

**教训：**跨 PowerShell 大版本写交互式脚本时，异常消息的**渲染**和**内容**是两件事。
内容写对了不代表用户看得到——5.1 与 7 的默认错误视图行为不同，只在一个版本上验证
过就等于没验证。

**未验证：** 全部 8 个脚本的运行时行为。这些脚本依赖 Windows 专有工具链
（libclang、`sqlite3` CLI、WiX、Android NDK、`flutter`）与 Windows 路径布局，
在 Linux 上无法执行。语法与静态规则通过不等于运行正确——首次在 Windows 上使用时
应逐个实跑，重点是 `regen.ps1` 的 clang 探测分支和 `build-apk.ps1` 的 NDK 定位。

## Consequences

Windows 上的开发循环不再需要手工翻译命令，且与 bash 版逐个对应，切换平台时心智成本低。

代价是多维护 7 个脚本，且它们**没有 CI 覆盖**——`.github/workflows/ci.yml` 只跑
`cargo test` 与 `cargo build --release`，PowerShell 脚本不在门禁内。同一处逻辑改动
需要在两个文件里各改一次，这是双份脚本的固有成本，接受它换来的是平台可运行性。

`build-apk.ps1` 只编 arm64 这一既有约束在两个平台上一致：Rust 核心仅提供
arm64-v8a 的 `libelsewhen.so`，裁掉多余 ABI 可把 APK 从约 72MB 降到约 31MB。
