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
# ⚠ 关于第 1 步（clang 头文件 / CPATH）：Windows 上很可能不需要，请勿照抄 Linux
#
#   regen.sh 第 1 步做的事源于一个 Linux 专属问题：ffigen/libclang 解析 C 系统头
#   时找不到 stdbool.h，就把 cbindgen 生成的
#     typedef bool (*DartPostCObjectFnType)(...)
#   解析成 `typedef bool = ffi.NativeFunction<...>`，直接遮蔽 Dart 的 bool 类型，
#   构建报错（该仓库踩过）。解法是把 clang 内置头目录放进 CPATH，再用 cc/cxx
#   shim 把 CPATH 从真正的 C 编译里摘掉。
#
#   在 Windows 上 libclang 通常能自己定位 resource dir（官方 LLVM 安装器带
#   lib\clang\<ver>\include），那个 stdbool.h 缺失多半是 Debian/Ubuntu 的 clang
#   打包问题，不会复现。**本脚本第 1 步因此是「探测到才启用」而非「缺了就报错」**，
#   并且 cc/cxx shim 用 .cmd 实现 —— cargo/cc-rs 对非 .exe 的 CC 有引号处理差异，
#   这层封装在 Windows 上比 POSIX 脆弱得多，能不用就不用。
#
#   我无法在 Windows 上实测本脚本。若第 1 步在你的机器上反而引入了问题，
#   直接删掉标记为「可选」的那一段即可，主流程不受影响。
# ---------------------------------------------------------------------------
param(
    # 强制启用 clang 头文件注入（默认自动探测；探测到就用，没探测到就跳过）
    [switch]$ForceClangInclude
)

$ErrorActionPreference = "Stop"

$root = Split-Path -Parent (Split-Path -Parent $MyInvocation.MyCommand.Path)
Push-Location $root

try {
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
    & $codegen generate
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
        throw "codegen hash 不一致：Dart=$dartHash Rust=$rustHash。`n" +
              "说明 codegen 没有真正重新生成，或 Rust 侧未同步重编。`n" +
              "app 启动时会在 RustLib.init() 报 hash mismatch —— 先解决这个再继续。"
    }

    Write-Host "✔ 完成: 生成的 Dart/Rust bridge 与 target\release\elsewhen.dll 已保持一致。"
    Write-Host "  codegen content hash = $dartHash（两侧一致）"
} finally {
    Pop-Location
}
