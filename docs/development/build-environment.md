# 构建环境配置（Windows / macOS / Linux）

本文档覆盖**从零开始把 elsewhen 编译起来**所需的全部环境配置，按平台分节。

事实来源：本文档中标注 **[仓库]** 的结论来自本仓文件（CMake 声明、pubspec、脚本、CI 配置），
标注 **[通用]** 的是 Flutter/Rust 官方工具链的通用要求。凡是本仓能自己证明的，都不写成通用经验。

---

## 0. 先理解：这是三条独立的链

初学者最容易卡在这里 —— 它们**各自独立安装、互不替代**，缺哪条都会以看似无关的报错出现。

| 链 | 工具 | 产物 | 缺了会怎样 |
|---|---|---|---|
| **Rust 核心** | `cargo` + C 编译器 | `target/release/libelsewhen.{so,dylib,dll}` | `cargo build` 失败 |
| **Flutter 桌面 UI** | Flutter SDK + FVM + 平台 SDK | `ui/build/<platform>/...` | `flutter run` / `flutter build` 失败 |
| **桥接代码生成** | **libclang**（第三套工具） | `ui/lib/bridge/generated.dart/` | `regen` 失败，桥接层无法更新 |

第三条最容易被忽略：**libclang 既不是 Rust 的依赖，也不是 Flutter 的依赖**，它是 ffigen
（`flutter_rust_bridge` 内部使用）的解析引擎。VS 的 clang、Flutter 自带的 clang 都不能替代它。

### ⚠ CI 只验证了第一条链 **[仓库]**

`.github/workflows/ci.yml` 的全部内容是：

```yaml
- uses: dtolnay/rust-toolchain@stable
- run: cargo test --all-targets
- run: cargo build --release
```

**不构建 Flutter、不跑 regen、不跑 219 个 Flutter 测试。** 也就是说第二、三条链在任何平台上
都没有自动化门禁 —— 出问题只会在你本机首次构建时暴露。本文各节的「验证」小节就是为此准备的。

---

## 1. 三个平台共同的部分

### 1.1 Rust

CI 用 `dtolnay/rust-toolchain@stable`，仓库里**没有 `rust-toolchain.toml`**，所以跟随 stable。

```bash
rustup default stable
cargo --version
```

Rust 侧不需要任何系统库 **[仓库]**，两个容易误判的地方已核实：

- `rusqlite` 带 `features = ["bundled"]`（`Cargo.toml`）→ **不需要装系统 SQLite**
- `reqwest` 用 `rustls-tls` 而非 `native-tls`（`Cargo.toml`）→ **不需要装 OpenSSL**

但**需要一个 C 编译器**：`ring`（加密）和 `rusqlite`（bundled sqlite3 源码）都走 `cc` crate 编译 C。
Linux 靠 `build-essential`，Windows 靠 MSVC，macOS 靠 Xcode 命令行工具。

### 1.2 Flutter + FVM

Flutter 版本用 [FVM](https://fvm.app/) 锁定。所有 Flutter 命令都应通过 `fvm flutter` 调用。

```bash
# 安装 FVM（见下方各平台小节的具体命令）
cd ui/
fvm install          # 装 ui/.fvmrc 里钉住的版本
fvm flutter pub get
fvm flutter doctor -v   # 确认所有平台工具链就位
```

**Dart SDK 约束：`^3.13.3`** **[仓库]**（`ui/pubspec.yaml`）。

> ### ⚠ `.fvmrc` 版本不一致 **[仓库]** — 尚未修复
>
> ```
> .fvmrc       → flutter 3.47.4
> ui/.fvmrc    → flutter 3.47.5
> ```
>
> 两个文件钉在不同的 patch 版本上。`CLAUDE.md` 里记的也是 3.47.4。实际以哪个为准取决于你在哪个
> 目录执行 FVM，**这可能导致同一份代码在两个目录下解析出不同的 SDK**。
>
> 本文档不做判断（哪个才是意图，需要你确认），但建议统一后同步修正 `CLAUDE.md`。

### 1.3 桥接代码生成（libclang）

**只有改了 Rust 侧公开 API 之后才需要这一步** **[仓库]**（`CLAUDE.md:200`）。
各平台的具体装法见下文对应小节 —— 这是三平台差异最大的部分。

```bash
./scripts/regen.sh          # Linux / macOS
powershell -File scripts\regen.ps1   # Windows
```

---

## 2. Linux

### 2.1 系统依赖

本仓实际需要哪些库，可从两个 CMake 声明直接读出（不是猜的）：

| 依赖 | 来源 | 用途 |
|---|---|---|
| GTK 3 | `ui/linux/CMakeLists.txt:58` — `pkg_check_modules(GTK REQUIRED ...)` | Flutter 桌面 UI 本身 |
| X11 | `cnativeapi-0.4.0/cxx_impl/src/CMakeLists.txt:36,94` | 窗口管理（nativeapi） |
| XInput (Xi) | 同上 `:37,94` | 窗口/输入事件 |
| pkg-config | 两个 CMakeLists 都用 `find_package(PkgConfig REQUIRED)` | 上述查找的前提 |

加上 Flutter Linux 工具链与 Rust 的 C 编译器：

**Debian / Ubuntu**
```bash
sudo apt update
sudo apt install -y \
  build-essential clang cmake ninja-build pkg-config \
  libgtk-3-dev libx11-dev libxi-dev liblzma-dev \
  sqlite3
```
> 老版本 Ubuntu 还可能需要 `libstdc++-12-dev`（新版本已由 `build-essential` 覆盖）。

**Fedora / RHEL**
```bash
sudo dnf install -y \
  gcc clang cmake ninja-build pkgconf-pkg-config \
  gtk3-devel libX11-devel libXi-devel xz-devel \
  sqlite
```

**Arch / CachyOS**（本文档撰写时所在的开发机环境）
```bash
sudo pacman -S --needed \
  base-devel clang cmake ninja pkgconf \
  gtk3 libx11 libxi xz \
  sqlite
```

**已在该开发机上实测通过的组合** **[仓库]**：
`clang 22.1.8` / `cmake 4.4.3` / `ninja 1.13.2` / `pkgconf 3.0.7` /
`gtk3 3.24.52` / `libx11 1.8.13` / `libxi 1.8.3` / `gcc 16.2.1` / `sqlite 3.53.4`，
并已成功产出 `dist/Elsewhen-0.1.0-x86_64.AppImage` 与
`dist/elsewhen-0.1.0-1-x86_64.pkg.tar.zst`。

### 2.2 libclang（本平台最麻烦的一处）

ffigen 需要 clang 的 **C API 共享库**。Linux 上的路径：

```bash
sudo apt install clang libclang-dev        # Debian/Ubuntu
sudo pacman -S --needed clang              # Arch：clang 包已含 libclang
```

### ⚠ Debian/Ubuntu 的 stdbool.h 坑 —— 本仓踩过 **[仓库]**

Debian/Ubuntu 的 clang 打包会让 libclang 找不到自己的内置头文件，于是 ffigen 把 cbindgen 生成的

```c
typedef bool (*DartPostCObjectFnType)(...);
```

误解析成 `typedef bool = ffi.NativeFunction<...>`，**直接遮蔽 Dart 的 `bool` 类型**，
表现为 `ui/lib/bridge/generated.dart/frb_generated.io.dart` 里出现污染性代码，
随后大批 `bool` 类型编译报错。

`scripts/regen.sh` 已经处理了这个：把 clang 完整资源头目录放进 `CPATH`，再用 `cc`/`cxx`
包装脚本把 `CPATH` 从真正的 C 编译里摘掉。**Linux 上照做即可，不要手动绕过。**

两个不要：
- **不要**只 `CPATH` 指向一个孤立目录里的单个 `stdbool.h` —— clang 会因此找不到自己的
  `stddef.h`，ffigen 报 SEVERE 后**半途产出残缺的 Dart**。症状同样是大批 undefined，
  但 Rust 侧照样编过，**极易误判成成功**。必须给整个 clang include 目录
  （仓库 `flutter_rust_bridge.yaml:23-25` 记了这条教训）。
- **不要**用 VS/其他来源的 clang include 目录 —— 必须是当前 clang 版本自己的。

### 2.3 验证

```bash
# Rust 侧
cargo build --release

# Flutter 侧
cd ui && fvm flutter doctor -v
fvm flutter pub get
fvm flutter build linux --release --target=lib/main_desktop.dart
```

> **⚠ 排查问题时务必带 `--target=lib/main_desktop.dart`** **[仓库]**。
> 不带时 `flutter build/run` 默认构建 `lib/main.dart`（不含窗口 chrome 初始化），
> 用它验证会得到「全部正常」的假结论。桌面入口用 `main_desktop.dart`。

### 2.4 打包 **[仓库]**

各脚本会在缺工具时报错并退出，工具名如下：

| 产物 | 命令 | 额外需要的工具 |
|---|---|---|
| `.deb` | `./scripts/package-deb.sh` | `dpkg-deb` |
| `.rpm` | `./scripts/package-rpm.sh` | `rpmbuild` |
| `.pkg.tar.*` | `./scripts/package-pacman.sh` | `makepkg` |
| `.AppImage` | `./scripts/package-appimage.sh` | `appimagetool`（**缺失时脚本自动下载**到缓存目录） |
| 桌面元数据 | `./scripts/install-linux-desktop.sh` | 可选：`update-desktop-database`、`gtk-update-icon-cache`、`kbuildsycoca5/6` |

AppImage 打包会把 `target/release/libelsewhen.so` 放进 `bundle/lib/` 并由 `AppRun` 设置
`LD_LIBRARY_PATH` —— frb 在 Linux 上按裸名 `dlopen`，这一步不能省。

---

## 3. macOS

### 3.1 工具链

```bash
xcode-select --install          # 命令行工具：提供 C 编译器 + codesign/hdiutil/sips/iconutil
rustup default stable
```

`nativeapi` 在 macOS 上只链接系统框架（`Cocoa` / `QuartzCore` / `Carbon` / `ServiceManagement`
**[仓库]**，`cnativeapi-0.4.0/cxx_impl/src/CMakeLists.txt:96-99`），**不需要额外装任何东西**。

libclang 方面，**装好 Xcode 命令行工具就够了**。ffigen 在 macOS 上的查找顺序
**[仓库]**（`ffigen-22.0.0/lib/src/config_provider/spec_utils.dart:479-503`、
`strings.dart:255-265`）：

1. `/Library/Developer/CommandLineTools/usr/` ← **Xcode 命令行工具的默认安装位置，排第一**
2. `/usr/local/opt/llvm/lib/`、`/opt/homebrew/opt/llvm/lib/`（Homebrew）
3. `xcodebuild -find-library libclang.dylib`（兜底）
4. `clang -print-file-name=libclang.dylib`（全局兜底）

> 列表里完整版 `Xcode.app` 的路径**排在最后**，ffigen 自己的注释说明了原因：它默认不含
> ObjectiveC 头文件。所以装了命令行工具就够，不必为了 ffigen 装完整版 Xcode。

若真遇到与 Linux 相同的 `stdbool.h` 问题，`scripts/regen.sh` 的 CPATH 逻辑同样适用
（它会自动探测 `/usr/lib/clang/*/include` 与 `/usr/local/lib/clang/*/include` **[仓库]**）。

### 3.2 验证

```bash
cargo build --release
cd ui && fvm flutter pub get
fvm flutter build macos --release --target=lib/main_desktop.dart
```

### 3.3 打包与签名 **[仓库]**

```bash
./scripts/package-dmg.sh
```

- **默认 ad-hoc 签名**（`codesign --sign -`），本机自用即可
- **对外分发**必须设 `CODESIGN_IDENTITY="Developer ID Application: ..."` 并重跑，
  且此后还需做 Apple 公证（notarization）—— 脚本不代办这一步

---

## 4. Windows

### 4.1 MSVC Build Tools

**这一项很多人已经有了**：如果你现在能成功跑 `package-msi.ps1`（它会执行
`cargo build --release`），说明链接器已经就位。**[仓库]**

需要 **Visual Studio 2022** 的「使用 C++ 的桌面开发」工作负载：

```powershell
winget install Microsoft.VisualStudio.2022.Community `
  --override "--wait --passive --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended"
```

### 4.2 LLVM —— 本平台唯一容易踩的坑

**必须单独安装 LLVM，且 Visual Studio 自带的 clang-cl 不能替代。**

原因（均从 ffigen 22.0.0 源码核实 **[仓库]**）：

1. VS 的「适用于 Windows 的 C++ Clang 工具」装的是 `clang-cl.exe` / `clang.exe` 两个**驱动**，
   位置在 `...\VC\Tools\Llvm\`，**既不提供 `libclang.dll`，也不在 ffigen 的搜索路径里**。
2. ffigen 在 Windows 上**默认只搜两个位置**：
   - `ffigen/lib/src/strings.dart:254` → `C:\Program Files\LLVM\bin\`
   - `ffigen/lib/src/config_provider/spec_utils.dart:465-474` →
     `%USERPROFILE%\scoop\apps\llvm\current\bin`
3. **不做 PATH 搜索**；`LIBCLANG_PATH` 也无效 —— ffigen 的 `lib/` 全量 grep 零命中。

但**并非只能装到这两个位置**。ffigen 有一等配置项 `llvm-path`
（`strings.dart:24`），优先级**高于**默认位置（`yaml_config.dart:335-340`）：配了就先试
用户路径、全不命中才回落默认；没配才直接走默认。

| 入口 | 写法 |
|---|---|
| 命令行 | `--llvm-path <LLVM 根目录>` |
| `flutter_rust_bridge.yaml` | `llvm_path:` |

路径拼接是 `<你给的路径>\bin\libclang.dll`（`strings.dart:27` 的
`dynamicLibParentName` 在 Windows 上是 `bin`），所以**给根目录**，也接受直接给
`libclang.dll` 完整路径（`spec_utils.dart:545-551`）。

装法二选一，装在默认位置就什么都不用配：

```powershell
# A. llvm.org 官方安装器（当前发行版是 .msi，需管理员权限）
winget install LLVM.LLVM
#    → C:\Program Files\LLVM\bin\libclang.dll

# B. scoop（无需管理员权限，体积更小）
scoop install llvm
#    → %USERPROFILE%\scoop\apps\llvm\current\bin\libclang.dll
```

验证：

```powershell
Test-Path "C:\Program Files\LLVM\bin\libclang.dll"
```

装在别处时用 `regen.ps1` 的 `-LlvmPath` 透传，**不要改 `flutter_rust_bridge.yaml`** ——
那份文件三平台共享，frb 没有条件键，写死 Windows 路径属于机器特定污染：

```powershell
powershell -File scripts\regen.ps1 -LlvmPath 'D:\tools\llvm'
powershell -File scripts\regen.ps1 -LlvmPath 'D:\tools\llvm\bin\libclang.dll'
```

> `scripts\regen.ps1` 的**第 0 步**就是这项检查，会在 `generate` 之前确认并给出处置；
> 给了 `-LlvmPath` 就只校验那一个路径，不传才复查上面两个默认位置。
>
> **注意与第 1 步区分**：CPATH / clang 头文件注入在 Windows 上**通常不需要**（那个
> `stdbool.h` 缺失是 Debian/Ubuntu 的打包问题），`regen.ps1` 因此做成「探测到才启用」，
> 默认不注入。libclang.dll 则是**硬依赖**，与它无关。

### 4.3 打包工具 **[仓库]**

| 用途 | 工具 | 说明 |
|---|---|---|
| MSI | WiX Toolset **v3.11** | `heat` / `candle` / `light`；通过 `%WIX%\bin` 定位 |
| APK | Android SDK + NDK | `ANDROID_HOME` 或 `ANDROID_SDK_ROOT`；还需 `rustup target add aarch64-linux-android` |

### 4.4 验证

```powershell
cargo build --release
cd ui
fvm install
fvm flutter pub get
fvm flutter build windows --release --target=lib/main_desktop.dart
```

### 4.5 Windows 脚本对照表 **[仓库]**

`scripts/` 下每个 `.sh` 都有同名 `.ps1`。**所有 `.ps1` 需 PowerShell 5.1+，且文件带 UTF-8 BOM**
（无 BOM 时 5.1 按系统 ANSI 代码页读取，中文注释与输出会整体乱码）。

| 用途 | PowerShell |
|---|---|
| 启动主应用 | `powershell -File scripts\elsewhen.ps1` |
| Capture 模式 | `powershell -File scripts\elsewhen-capture.ps1` |
| 带诊断日志启动 | `powershell -File scripts\debug-run.ps1` |
| 重建 frb bridge | `powershell -File scripts\regen.ps1` |
| 校验 bridge 集成 | `powershell -File scripts\test_bridge.ps1` |
| 打 Android APK | `powershell -File scripts\build-apk.ps1` |
| 打 Windows MSI | `powershell -File scripts\package-msi.ps1` |
| 旧库版本标记修复 | `powershell -File scripts\stamp-legacy-db.ps1` |

`Makefile` 是 `/bin/sh`，**在 Windows 上不可用**；根 `Makefile` 的 `msi` 目标除外
（它调的就是 `package-msi.ps1`）。

---

## 5. 已知问题汇总

### 5.1 三个平台共通

| 问题 | 性质 | 处置 |
|---|---|---|
| CI 不构建 Flutter、不跑 regen、不跑 Flutter 测试 **[仓库]** | 门禁缺口 | 本地按各节「验证」逐项自检 |
| `.fvmrc` 3.47.4 与 `ui/.fvmrc` 3.47.5 不一致 **[仓库]** | 待定 | 需确认意图后统一，并同步 `CLAUDE.md` |
| `flutter build` 不带 `--target` 会验证到错误的入口 **[仓库]** | 易误判 | 始终带 `--target=lib/main_desktop.dart` |

### 5.2 Linux 专属

| 问题 | 处置 |
|---|---|
| Debian/Ubuntu 的 `stdbool.h` 缺失污染生成代码 | 用 `scripts/regen.sh`，勿手动绕过（第 2.2 节） |
| frb 按裸名 `dlopen` 找不到桥接库 | `flutter run/build` 会把 `.so` 放进 bundle，正常无需干预；**自建 bundle 或手动搬 `.so` 时**才需设 `LD_LIBRARY_PATH`（AppImage 脚本已代劳） |

### 5.3 macOS 专属

| 问题 | 处置 |
|---|---|
| ad-hoc 签名无法对外分发 | 设 `CODESIGN_IDENTITY` + Apple 公证（第 3.3 节） |

### 5.4 Windows 专属

| 问题 | 处置 |
|---|---|
| 用 VS 的 clang-cl 顶替 LLVM → ffigen 找不到 libclang | 另装 LLVM。默认位置（第 4.2 节）或用 `-LlvmPath` 指定 |
| 试图用 `LIBCLANG_PATH` 修上面那个问题 | **无效**，ffigen 全量 grep 零命中。用 `--llvm-path` / `-LlvmPath` |
| PowerShell 5.1 下中文乱码 | 所有 `.ps1` 必须带 UTF-8 BOM |

---

## 6. 最小可用路径速查

只求**把 Rust 核心编译出来**（CI 验证的范围内）：

| 平台 | 装什么 |
|---|---|
| Linux | `build-essential` |
| macOS | `xcode-select --install` |
| Windows | MSVC Build Tools |

要**跑起完整应用**，在上述基础上追加：

| 平台 | 追加 |
|---|---|
| Linux | `clang cmake ninja pkgconf gtk3 libx11 libxi xz` + FVM + Flutter |
| macOS | 同 Linux 的 Flutter 部分（系统依赖省掉，只需 Xcode CLT） |
| Windows | Flutter + **LLVM（libclang）** + MSVC Build Tools |

要**改 Rust API 后重新生成桥接**，再追加：Linux `libclang-dev` / Windows 官方 LLVM 或 scoop。

---

## 附：本文档的事实来源

| 结论 | 出处 |
|---|---|
| CI 只跑 `cargo test` + `cargo build --release` | `.github/workflows/ci.yml` |
| GTK 3 / pkg-config 依赖 | `ui/linux/CMakeLists.txt:57-58` |
| X11 / XInput 依赖 | `cnativeapi-0.4.0/cxx_impl/src/CMakeLists.txt:34-37, 89-104` |
| macOS 无需额外系统库 | 同上 `:96-99`（仅系统框架） |
| Windows 无需额外系统库 | 同上 `:104`（均为 Windows SDK 库） |
| 无系统 SQLite / 无 OpenSSL | `Cargo.toml` 的 `rusqlite` bundled、`reqwest` rustls-tls |
| Dart SDK 约束 | `ui/pubspec.yaml` |
| Flutter 版本钉定 | `.fvmrc`、`ui/.fvmrc` |
| libclang 默认搜索路径与环境变量不可覆盖 | `ffigen-22.0.0/lib/src/strings.dart:254`、`spec_utils.dart:465-474`；`LIBCLANG_PATH` 全量 grep 零命中 |
| `llvm-path` 可覆盖默认路径且优先级更高 | `strings.dart:24,27`、`yaml_config.dart:335-340`、`spec_utils.dart:534-552` |
| `libclangOverridePaths` 是 fork 钩子而非用户配置项 | `overrideable_utils.dart:41-43` 注释原文为 "Downstream **clones** can use" |
| macOS 命令行工具即默认查找位置 | `ffigen-22.0.0/lib/src/strings.dart:255-265`、`spec_utils.dart:479-503` |
| Linux `stdbool.h` 坑 | `flutter_rust_bridge.yaml:18-25`、`scripts/regen.sh` |
| 打包工具依赖 | `scripts/package-{deb,rpm,pacman,appimage,dmg}.sh` |
| Windows 脚本与 BOM 要求 | `scripts/*.ps1`、`CLAUDE.md` |
| CachyOS 实测组合 | 本机 `pacman -Q` 与 `dist/` 产物 |
