# regen.ps1 - flutter_rust_bridge 一键重新生成 + 重编 Rust release 库（Windows）
#
# 与 regen.sh 等价的主流程（codegen → cargo build --release），并多做一件
# .sh 没做的事：跑完后静态比对两侧的 codegen content hash。
#
# 为什么必须重编 release 库：
#   重新生成 Dart 侧代码后，target\release\elsewhen.dll 里的
#   FLUTTER_RUST_BRIDGE_CODEGEN_CONTENT_HASH 会与 Dart 侧的 rustContentHash
#   不一致，运行时 RustLib.init() 直接报 hash mismatch。
#   .sh 靠「脚本最后一步总是重编」来保证。PowerShell 版改成显式校验——
#   万一 codegen 没真正生效（工具版本不匹配、命令静默 no-op），.sh 会安静地
#   产出一对不匹配的库，直到运行时才炸；这里在脚本结束前就报错。
#
# ---------------------------------------------------------------------------
# ⚠ 硬依赖：libclang.dll（第 0 步会检查）
#
#   ffigen 靠 libclang 的 C API 解析 C 头，没有它 `generate` 跑不起来。
#   **Visual Studio 自带的 clang-cl 不算数**：那是 clang-cl.exe / clang.exe 两个
#   驱动，位置在 ...\VC\Tools\Llvm\，而 ffigen 的搜索列表里没有这个路径，
#   它也不提供 libclang.dll。
#
#   ffigen 22.0.0 在 Windows 上的**默认**搜索位置只有两个（源码可查，勿凭印象）：
#     · ffigen/lib/src/strings.dart:254      → C:\Program Files\LLVM\bin\
#     · ffigen/lib/src/config_provider/spec_utils.dart:465-474
#                                           → %USERPROFILE%\scoop\apps\llvm\current\bin
#   没有 PATH 搜索，LIBCLANG_PATH 也无效（ffigen 的 lib 全量 grep 零命中）。
#
#   ⚠ 但**并非只能装到这两个位置**。ffigen 有一等配置项 llvm-path
#   （strings.dart:24），且优先级高于默认位置 —— yaml_config.dart:335-340：
#     有该键 → llvmPathExtractor（spec_utils.dart:534-560）先试用户给的路径，
#              全不命中才回落默认；无该键 → 直接走默认位置。
#   两个入口：
#     · 命令行  --llvm-path <LLVM 根目录>          ← 本脚本的 -LlvmPath 走这个
#     · 配置    flutter_rust_bridge.yaml 的 llvm_path:
#   路径拼接是 path.join(你给的路径, dynamicLibParentName)，而 strings.dart:27 里
#   dynamicLibParentName 在 Windows 上是 bin —— 所以要给的是**根目录**
#   （C:\Program Files\LLVM），不是 ...\bin。也接受直接给 libclang.dll 的完整路径
#   （spec_utils.dart:545-551 会先按扩展名判定）。
#
#   装法二选一：llvm.org 官方安装器（默认 C:\Program Files\LLVM）或
#   `scoop install llvm`。两者都落在默认路径里时**不需要任何额外参数**。
#   装在别处时用 -LlvmPath 指定，不要去改 yaml —— 那份文件三平台共享，
#   frb 没有条件键，写死 Windows 路径属于机器特定污染。
#
#   注意这与 MSVC Build Tools 是**两件独立的事**：后者是 cargo 链接 .dll 用的，
#   VS 提供链接器、LLVM 提供 ffigen 的解析引擎，互不替代。#2 的 cargo build
#   能成功不代表 #1 能成功，反之亦然。
#
# ---------------------------------------------------------------------------
# ⚠ 关于第 1 步（clang 头文件 / CPATH）：Windows 上很可能不需要，请勿照抄 Linux
#
#   regen.sh 第 1 步做的事源于一个 Linux 专属问题：ffigen/libclang 解析 C 系统头
#   时找不到 stdbool.h，就把 cbindgen 生成的
#     typedef bool (*DartPostCObjectFnType)(...)
#   解析成 `typedef bool = ffi.NativeFunction<...>`，直接遮蔽 Dart 的 bool 类型，
#   构建报错（该仓库踩过）。解法是把 clang 内置头目录放进 CPATH，再用 cc/cxx
#   shim 把 CPATH 从真正的 C 编译里摘掉。
#
#   那个 stdbool.h 缺失源于 Debian/Ubuntu 的 clang 打包问题，官方 LLVM 安装器的
#   libclang.dll 自带编译期 resource dir 路径，会自己找到
#   lib\clang\<ver>\include\stdbool.h，所以 Windows 上通常不复现。
#   **本脚本第 1 步因此是「探测到才启用」而非「缺了就报错」**，并且不实现
#   cc/cxx shim —— cargo/cc-rs 对非 .exe 的 CC 有引号处理差异，这层封装在
#   Windows 上比 POSIX 脆弱得多；CPATH 本身是 gcc/clang 的搜索路径变量，
#   MSVC 的 cl.exe 根本不读它，regen.sh 那个污染问题在 Windows 上无对应物。
#
#   我无法在 Windows 上实测本脚本。若第 1 步在你的机器上反而引入了问题，
#   直接删掉标记为「可选」的那一段即可，主流程不受影响。
# ---------------------------------------------------------------------------
param(
    # 强制启用 clang 头文件注入（默认自动探测；探测到就用，没探测到就跳过）
    [switch]$ForceClangInclude,

    # 指定 LLVM 位置，转成 --llvm-path 传给 codegen，并让第 0 步改为只校验它。
    # 接受「根目录」（自动补 bin）或 libclang.dll 的完整路径。
    # LLVM 装在 ffigen 的两个默认位置时**不需要**这个参数。
    [string]$LlvmPath
)

$ErrorActionPreference = "Stop"

# 多行提示**不能**用 throw 直接抛。PowerShell 7 对未捕获异常的默认视图
# （ConciseView）会折叠连续空白，把排版好的处置说明压成一堵字墙；PS 5.1 的
# NormalView 不折叠，所以这个坑只在 7+ 暴露 —— 实测：
#   C:\Program Files\LLVM\bin\   %USERPROFILE%\scoop\apps\llvm\current\bin
#   它不做 PATH 搜索...两条出路：    1) 装到默认位置   2) 装在别处
# 换行全没了。改成 Write-Host 逐行输出（两个版本都正常），throw 只带单行摘要，
# 退出码仍为 1，finally 的 Pop-Location 也不受影响。
function Stop-WithGuidance([string]$summary, [string]$guidance) {
    Write-Host ""
    Write-Host $guidance
    Write-Host ""
    throw $summary
}

$root = Split-Path -Parent (Split-Path -Parent $MyInvocation.MyCommand.Path)
Push-Location $root

try {
    # ---------- 0. 硬依赖前置检查：libclang.dll ----------
    # 放在最前面，因为缺它时 codegen 会在完全无关的位置报错（ffigen 抛的是
    # 「找不到动态库」），用户很难联想到是 LLVM 没装。
    #
    # 给了 -LlvmPath 就只校验它（ffigen 的 llvm-path 优先级高于默认位置，见头注释）；
    # 没给才复查 ffigen 的两个默认路径。默认路径与源码一一对应：
    # strings.dart:254 / spec_utils.dart:465-474。
    # USERPROFILE 必须守卫：ffigen 自己判了 userHome != null 才 join（spec_utils.dart:466），
    # 而 Join-Path $null 抛的是 ParameterBindingValidationException，在
    # $ErrorActionPreference = "Stop" 下会终止整个脚本 —— 那样用户看到的是一句
    # 「Cannot bind argument to parameter 'Path'」而不是下面那段可执行的处置。
    $libclangDirs = @("C:\Program Files\LLVM\bin")
    if ($env:USERPROFILE) {
        $libclangDirs += (Join-Path $env:USERPROFILE "scoop\apps\llvm\current\bin")
    }
    # [System.IO.Path]::Combine + [System.IO.File]::Exists 而非 Test-Path：
    # Test-Path 遇到不存在的驱动器会**抛**「Cannot find drive」而不是返回 false，
    # 在 $ErrorActionPreference = "Stop" 下会终止脚本。.NET 的 Exists 内部吞掉
    # 全部 IO 错误、只返回 false，语义正好是这里要的「这个文件在不在」。
    # @() 不能省：Where-Object 只命中一条时返回的是标量 String，此时 $x[0] 取到的是
    # 首字符而非路径（实测得到 "C"）。而单机上只有一处 LLVM 恰恰是最常见情况。
    $libclangDirs = @($libclangDirs |
        Where-Object { [System.IO.File]::Exists([System.IO.Path]::Combine($_, "libclang.dll")) })

    if ($LlvmPath) {
        # 严格复刻 ffigen 的 llvmPathExtractor（spec_utils.dart:534-552），
        # 顺序和条件都不能简化：
        #   ① 先按「根目录」试 <path>\bin\libclang.dll
        #   ② 再按「libclang.dll 完整路径」试，且要求扩展名非空**且文件确实存在**
        # 只判扩展名会把 D:\llvm-18.1 这类带版本号的根目录误当成文件名 —— ffigen
        # 因为同时判了文件存在所以不受影响，这里必须跟它一样。
        $dll = [System.IO.Path]::Combine($LlvmPath, "bin", "libclang.dll")
        if (-not [System.IO.File]::Exists($dll) -and
            [System.IO.Path]::GetExtension($LlvmPath) -and
            [System.IO.File]::Exists($LlvmPath)) {
            $dll = $LlvmPath
        }
        if (-not [System.IO.File]::Exists($dll)) {
            Stop-WithGuidance -summary "libclang.dll 未找到（-LlvmPath 指定的 LLVM 下没有）" -guidance @"
-LlvmPath 指定的 LLVM 下没有找到 libclang.dll：

  -LlvmPath $LlvmPath
  → 解析为：$dll

已尝试两种形式（与 ffigen 的 llvmPathExtractor 一致）：
  <路径>\bin\libclang.dll          ← 当作 LLVM 根目录
  <路径>                            ← 当作 libclang.dll 完整路径

所以 -LlvmPath 通常给 **根目录**，例如：

  -LlvmPath 'C:\Program Files\LLVM'
  -LlvmPath 'D:\tools\llvm-18.1'

官方安装器的默认根目录是 C:\Program Files\LLVM。

也可以直接给完整文件名，例如：

  -LlvmPath 'C:\Program Files\LLVM\bin\libclang.dll'

不想自定义位置就去掉 -LlvmPath —— ffigen 会自己搜这两个默认路径：
  C:\Program Files\LLVM\bin\
  %USERPROFILE%\scoop\apps\llvm\current\bin
"@
        }
        $libclangDirs = @([System.IO.Path]::GetDirectoryName($dll))
    }
    elseif (-not $libclangDirs) {
        Stop-WithGuidance -summary "libclang.dll 未找到（ffigen 的两个默认位置都没有）" -guidance @"
ffigen 需要 libclang.dll，但 ffigen 22.0.0 在 Windows 上的两个默认位置都没有：

  C:\Program Files\LLVM\bin\
  %USERPROFILE%\scoop\apps\llvm\current\bin

它不做 PATH 搜索，LIBCLANG_PATH 也无效（ffigen 的 lib 里没有这个字符串）。
两条出路：

  1) 装到默认位置 —— 官方安装器（默认就装到 C:\Program Files\LLVM）或
     `scoop install llvm`，装完直接重跑本脚本即可。
  2) 装在别处 —— 用 -LlvmPath 告诉 ffigen 去哪找：
       .\scripts\regen.ps1 -LlvmPath 'D:\tools\llvm'
     （给根目录，ffigen 会自己补 bin；也可直接给 libclang.dll 完整路径。）

注意 Visual Studio 自带的 clang-cl 不能替代：那是驱动而不是 libclang.dll，
且不在 ffigen 的搜索路径里。另外它与 cargo 链接所需的 MSVC Build Tools 是
两件独立的事 —— 能 cargo build 不代表 ffigen 能用，反之亦然。
"@
    }
    Write-Host "[0/3] libclang.dll：$($libclangDirs[0])"

    # ---------- 1. [可选] Clang 头文件 ----------
    Write-Host "[1/3] 探测 clang 内置头文件 ..."

    $clangInclude = $null
    if ($ForceClangInclude) {
        # 探测：优先 llvm 工具链里的 lib\clang\<ver>\include，再试 PATH 上的 clang 旁边
        $candidates = @()
        $llvmRoots = @(
            "C:\Program Files\LLVM",
            "$env:LOCALAPPDATA\Programs\LLVM",
            "C:\Program Files (x86)\LLVM"
        )
        # 用户已通过 -LlvmPath 指定的 LLVM 是最可信的根目录，优先探它。
        # 与第 0 步同一套判定：完整文件名形式要先取其所在目录。
        if ($LlvmPath) {
            if ([System.IO.Path]::GetExtension($LlvmPath) -and [System.IO.File]::Exists($LlvmPath)) {
                $llvmRoots = @([System.IO.Path]::GetDirectoryName($LlvmPath)) + $llvmRoots
            }
            else {
                $llvmRoots = @($LlvmPath) + $llvmRoots
            }
        }
        foreach ($r in $llvmRoots) {
            if (Test-Path "$r\lib\clang") {
                $candidates += Get-ChildItem "$r\lib\clang" -Directory -ErrorAction SilentlyContinue |
                    ForEach-Object { Join-Path $_.FullName "include" }
            }
        }
        $clangCmd = (Get-Command clang -ErrorAction SilentlyContinue).Source
        if ($clangCmd) {
            # <llvm>\bin\clang.exe → <llvm>\lib\clang
            $llvmBin = Split-Path -Parent $clangCmd
            $candidates += Join-Path (Split-Path -Parent $llvmBin) "lib\clang"
        }
        foreach ($c in $candidates) {
            if ((Test-Path (Join-Path $c "stdbool.h")) -and (Test-Path (Join-Path $c "stddef.h"))) {
                $clangInclude = $c
                break
            }
        }
    }

    if ($clangInclude) {
        Write-Host "     找到：$clangInclude"
        $env:CPATH = if ($env:CPATH) { "$clangInclude;$env:CPATH" } else { $clangInclude }
        Write-Host "     已注入 CPATH"
    } else {
        Write-Host "     未探测到 clang 内置头文件（或不需要），跳过 CPATH 注入。"
        Write-Host "     若 codegen 报 stdbool.h / bool 类型遮蔽错误，重跑并加 -ForceClangInclude。"
    }

    # ---------- 2. 重新生成 bridge 代码 ----------
    Write-Host "[2/3] flutter_rust_bridge_codegen generate ..."
    $codegen = (Get-Command flutter_rust_bridge_codegen -ErrorAction Stop).Source
    if ($LlvmPath) {
        # 原样透传：ffigen 的 llvmPathExtractor 接受根目录或 libclang.dll 完整路径两种形式
        & $codegen generate --llvm-path $LlvmPath
    }
    else {
        & $codegen generate
    }
    if ($LASTEXITCODE -ne 0) { throw "flutter_rust_bridge_codegen generate 失败（退出码 $LASTEXITCODE）" }
    Write-Host "     生成完成"

    # ---------- 3. 重编 Rust release 库（保持 hash 一致） ----------
    Write-Host "[3/3] cargo build --release ..."
    cargo build --release
    if ($LASTEXITCODE -ne 0) { throw "cargo build --release 失败（退出码 $LASTEXITCODE）" }

    # ---------- 4. [额外] 静态校验两侧 hash ----------
    # .sh 没有这一步：它只保证「总是重编」。这里直接读源码里的两个常量比对，
    # codegen 若没真正生效会立刻暴露，而不是等到 app 启动时 RustLib.init() 报错。
    $dll = "target\release\elsewhen.dll"
    if (-not (Test-Path $dll)) { throw "未找到 $dll，桥接库没有编出来" }

    $dartHash = (Select-String -Path "ui\lib\bridge\generated.dart\frb_generated.dart" `
        -Pattern 'int get rustContentHash => (-?\d+);' |
        Select-Object -First 1).Matches[0].Groups[1].Value
    $rustHash = (Select-String -Path "src\frb_generated.rs" `
        -Pattern 'FLUTTER_RUST_BRIDGE_CODEGEN_CONTENT_HASH: i32 = (-?\d+);' |
        Select-Object -First 1).Matches[1].Value

    Write-Host ""
    if (-not $dartHash) { throw "未能在 ui\lib\bridge\generated.dart\frb_generated.dart 中找到 rustContentHash" }
    if (-not $rustHash) { throw "未能在 src\frb_generated.rs 中找到 FLUTTER_RUST_BRIDGE_CODEGEN_CONTENT_HASH" }

    if ($dartHash -ne $rustHash) {
        Stop-WithGuidance -summary "codegen hash 不一致：Dart=$dartHash Rust=$rustHash" -guidance @"
codegen hash 不一致：Dart=$dartHash  Rust=$rustHash

说明 codegen 没有真正重新生成，或 Rust 侧未同步重编。
app 启动时会在 RustLib.init() 报 hash mismatch —— 先解决这个再继续。
"@
    }

    Write-Host "✔ 完成: 生成的 Dart/Rust bridge 与 target\release\elsewhen.dll 已保持一致。"
    Write-Host "  codegen content hash = $dartHash（两侧一致）"
} finally {
    Pop-Location
}
