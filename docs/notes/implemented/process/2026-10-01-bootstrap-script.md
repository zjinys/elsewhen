# Agent Note: 新环境一键启动脚本

Status: implemented

## Problem

新 clone 一份代码、装好系统依赖与工具链之后，**没有办法用一条命令把 app 跑起来**。
`scripts/` 下最接近的入口是 `elsewhen.sh`，但它只有两行有效内容
（`cd ui` + `fvm flutter run -d linux`），而这一步依赖三样**都不在 git 里**的东西：

| 缺什么 | 为什么不在 git 里 | 直接跑 elsewhen.sh 的症状 |
|---|---|---|
| `target/release/libelsewhen.so` | `.gitignore` 的 `/target/` | Dart 侧 `RustLib.init()` 抛一句 dlopen 失败，看不出「该先编 Rust」 |
| `ui/.fvm/flutter_sdk` | `.gitignore` 的 `.fvm/`（库里只有 `ui/.fvmrc`） | fvm 报没有配置 SDK |
| `ui/.dart_tool/package_config.json` | 未提交 | 依赖解析失败 |

也就是说 `docs/development/build-environment.md` 讲清楚了「装什么」，但从「装完」到
「跑起来」之间那段固定的活只散落在文档各处，没有任何一处把它固化下来。

## Decision

新增 `scripts/bootstrap.sh` 与 `scripts/bootstrap.ps1`，五步：

1. 预检工具链与系统依赖
2. 解析 Flutter SDK
3. `pub get`
4. `cargo build --release --lib`
5. 校验桥接 codegen hash，然后启动

参数：`--setup-only`（只准备）、`--regen`（先跑 regen）、
`--device <id>`（换设备），其余原样透传给 `flutter run`。

### 不装任何东西

系统包要 sudo、Flutter/Rust 要交互式安装器。代劳只会把「环境没配好」变成
「脚本坏了」，而且新机器上这类失败往往发生在没有网络的场合。脚本只做一件装包
做不到的事：**在动手之前把缺的东西指出来，并指回 build-environment.md 对应小节**。

### 预检项对着 CMake 的真实查找逻辑挑

不是「常见工具清单」。逐条对应：

- `pkg-config` + `gtk+-3.0` —— `ui/linux/CMakeLists.txt:57-58`
- `pkg-config` + `x11` + `xi` —— `cnativeapi-0.4.0/cxx_impl/src/CMakeLists.txt:34-37`，
  且该分支仅在 `CMAKE_SYSTEM_NAME STREQUAL "Linux"` 时生效（所以 macOS 不查）
- `cmake` / `ninja`

这些的真实症状都是 CMake 在 configure 阶段报错，而那已经在几分钟的
`cargo build` 之后了。预检的价值就是把这个失败提前几十秒。

### 每次都跑 cargo，不做「.so 缺失才跑」

Dart 侧 frb 默认从 `../target/release/` dlopen，所以必须是 release。按需构建
在日常场景下会「改了 Rust 代码但没 regen」→ 静默跑旧 `.so` → 直到调用缺失接口
才报难懂的错。cargo 自己的 fingerprint 让无改动的重复构建耗时接近 0
（实测全量 1m08s、增量 12s），正确性不靠猜。

### 第 5 步的 hash 校验：唯一能提前抓到它的时机

比对 Dart 侧 `rustContentHash` 与 `src/frb_generated.rs` 的常量。刚编完的 `.so`
里烧的就是后者，所以**源码层面相等 ⟺ 运行时相等**。

不一致时 app 会在 `RustLib.init()` 抛
`Content hash on Dart side ... is different from Rust side`，而那句话完全看不出
该跑 regen。这里是唯一能在启动前抓到它的时机，所以是硬失败（exit 1）而不是警告。

`regen.sh` 之所以没有这一步，是因为它「总是重编」；`regen.ps1` 有这一步但同样在
重编之后。bootstrap 每次都重编，所以可以放心用源码层面的比对做代理。

## Verification

**已验证（`bootstrap.sh`）：**

在 `/tmp` 下用 `git clone` 造了三个干净 clone 逐条走：

- **真实首跑**：无 `target/`、无 `ui/.fvm/`、无 `ui/.dart_tool/` → 五步全过，
  release 全量 1m08s。增量重跑 13.6s。
- **hash 不一致分支**：改 `src/frb_generated.rs` 的常量制造不一致 → 第 5 步
  exit 1 并打出两条出路，`--setup-only` 与启动路径都不会被执行。
- **缺 pkg-config 模块**：用假 `pkg-config` 让 `gtk+-3.0` 不可见 → 第 1 步
  报错，三个发行版的装法命令与文档小节一起打出，exit 1。
- **Flutter 解析的三条分支**：
  - `ui/.fvm` 已解析 → 不要求 PATH 里有 fvm
  - `.fvm` 缺失但有 fvm → `fvm install` 后使用
  - `.fvm` 与 fvm 都缺、只有全局 flutter → 回退并**显式警告版本不再由
    `ui/.fvmrc` 钉住**（实测拿到全局的 3.47.5）
  - 三者都缺 → 报错并列出三条出路
- **参数透传**：用假 `flutter` 打印 argv，确认默认设备、`--device`、
  透传参数自带 `-d` 三种情况下都不会出现两个 `-d`。

**已验证（`bootstrap.ps1`）：**

下载 PowerShell 7.4.6 linux-x64：

- `[System.Management.Automation.Language.Parser]::ParseFile` 解析 9 个脚本全部通过
- PSScriptAnalyzer 1.25.0：Error 0 findings；Warning 只有
  `PSAvoidUsingWriteHost`(21) 与 `PSUseShouldProcessForStateChangingFunctions`(1)，
  后者与 `regen.ps1` 同规则同误报（`Stop-WithGuidance` 的动词 Stop 被误判为
  状态变更函数，实际只打印+抛异常）。与既有脚本基线一致，无新增规则类别。
- UTF-8 BOM + LF 行尾，与既有 8 个 ps1 一致。

**PowerShell 参数绑定与 bash 不同（实测确认，写进了两处文档与脚本头注释）。**
最小复现脚本验证：`ValueFromRemainingArguments = $true` **不改变** `-`/`--` 开头的
token 按参数名前缀匹配的规则。所以 `-d android`、`--device android` 都会被绑到
`-Device`，而不是交给 `flutter run`。设备在 Windows 上一律用 `-Device` 指定。

据此删掉了 ps1 里「透传里自带 -d 就不加默认设备」的分支——在 PowerShell 上那是
**永远走不到的死代码**（`-d` 根本进不了 `$FlutterArgs`），留着会误导读者以为
可以用 `-d` 透传。bash 版保留该分支，因为 bash 没有前缀匹配。

**未验证：**

- `bootstrap.ps1` 的运行时行为。依赖 Windows 专有工具链（MSVC、`flutter.bat`、
  Windows 路径布局），在 Linux 上无法执行。首次在 Windows 上使用时应先跑
  `-SetupOnly`，重点看第 2 步的 SDK 解析分支。
- `bootstrap.sh` 的 macOS 分支（`xcode-select -p` 预检 + 默认设备 `macos`）。
  本机是 Linux，`Darwin` 分支代码正确但未实跑。
- 实际启动 app 的完整流程（`flutter run` 之后的窗口渲染）。验证止于
  `--setup-only` 与用假 `flutter` 验证 argv 拼装。

> ⚠ **测试期间的意外副作用（已核查）。** 用真 `flutter` 验证启动路径时，app 被
> 真实启动了两次，触碰了 `~/.local/share/elsewhen/elsewhen.db`（真实个人库）。
> 后台 worker 因此发起了 3 次知识消化，全部因 API 超时 `failed`，
> `wiki_pages` 与 `wiki_revisions` **无任何改动**。教训：验证启动路径应当用假
> `flutter` 占位，不要用真的——`flutter run` 会真的把 app 拉起来。

## Consequences

新机器上从 clone 到 app 起来是一条命令，且每一步失败都指向具体缺什么。

代价：

- **两个脚本无 CI 覆盖。** `.github/workflows/ci.yml` 只跑 `cargo test` 与
  `cargo build --release`，bash/PowerShell 脚本都不在门禁内。同一处逻辑改动要在
  两个文件里各改一次——这是既有的双份脚本成本的延续（见
  [Windows 开发脚本](2026-09-30-windows-dev-scripts.md) 的 Consequences）。
- **第 5 步的 hash 匹配式绑死了 frb 的生成布局。** frb 升级改了常量名或类型，
  这里会误报「找不到 rustContentHash」。已在报错信息里写明这一点，且失败是硬错误
  不会静默放行。
- **预检项会随 CMakeLists 漂移。** 四个 pkg-config 模块是照 2026-10-01 的
  `CMakeLists.txt` 抄的，nativeapi 升版改了依赖就得跟着改。选它是因为预检的价值
  就是把「几分钟后在 CMake 里失败」提前到「几十秒内失败」，而漂移的代价远小于
  失去预检。