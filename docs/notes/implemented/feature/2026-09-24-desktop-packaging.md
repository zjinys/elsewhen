# 2026-09-24 桌面端三平台打包：AppImage / MSI / DMG

## 结论
新增 `scripts/package-appimage.sh`（Linux AppImage）、`scripts/package-dmg.sh`
（macOS DMG）、`scripts/package-msi.ps1` + `packaging/windows/elsewhen.wxs`
（Windows MSI），Makefile 对应目标 `appimage / dmg / msi`。
AppImage 已在本机端到端实测（构建、解包内容、外地 CWD 运行桥加载全通）；
DMG/MSI 脚本需在对应平台运行，逻辑对称、本机无法实测。

## 关键问题：Rust 桥接库怎么进包（frb 无 cargokit 的手动链路）
本仓库不用 cargokit，`libelsewhen.{so,dll,dylib}` 由 regen.sh 单独构建，
运行时靠 frb 默认加载链找库。逐平台核对 `flutter_rust_bridge-2.14.0-beta.2
/lib/src/loader/_io.dart` 后的落点：

| 平台 | frb 加载方式 | 打包时库放哪 |
|------|-------------|-------------|
| Linux | `dlopen('libelsewhen.so')` 裸名，glibc 搜调用方 RPATH（Flutter runner `$ORIGIN/lib`）→ 实测外地 CWD 运行 OK | `bundle/lib/`（AppRun 再兜 `LD_LIBRARY_PATH`） |
| Windows | `LoadLibrary('elsewhen.dll')` 裸名，加载器先搜 exe 所在目录 | exe 旁边（`runner/Release/`） |
| macOS | 默认链（CWD 相对 ioDirectory → `*.framework`）在 Finder 启动（CWD=/）全灭 | `Contents/Frameworks/`，**且需 Dart 显式路径兜底** |

macOS 兜底实现：`rust_bridge_repository._bundledRustLibrary()`——
`Platform.resolvedExecutable` 推 `../Frameworks/libelsewhen.dylib`，存在才
`ExternalLibrary.open` 注入 `RustLib.init(externalLibrary:)`；开发/测试环境
文件不存在返回 null，走 frb 默认链（行为不变）。
注意 `ExternalLibrary` 在 frb 2.14 的导出口是
`flutter_rust_bridge_for_generated_io.dart`（不在主入口）。

## 平台 runner 脚手架
ui/ 原本只有 linux runner；`flutter create --platforms=windows,macos .` 生成
windows/ 与 macos/，并把标识改到与 Linux 对齐：
- macOS `AppInfo.xcconfig`：PRODUCT_NAME=Elsewhen、
  BUNDLE_ID=io.github.elsewhen.Elsewhen（与 APPLICATION_ID 一致）
- Windows `Runner.rc`：CompanyName=Elsewhen、ProductName=Elsewhen

## 各包要点
- **AppImage**：AppDir 布局 `AppRun + elsewhen.desktop + elsewhen.png +
  usr/bin/<bundle>`；AppRun 设 LD_LIBRARY_PATH 后 exec elsewhen_ui；
  appimagetool 缺失自动下载到 `~/.cache/elsewhen/`；无 /dev/fuse 时
  APPIMAGE_EXTRACT_AND_RUN=1。
- **DMG**：dylib → Contents/Frameworks；品牌 PNG 经 sips+iconutil 生成
  AppIcon.icns；默认 ad-hoc codesign（本机可用），分发需
  `CODESIGN_IDENTITY` + notarytool 公证（脚本头注释写明）。
- **MSI**：WiX v3.11（heat 现场收集 Release 目录 → candle/light），
  不写死文件清单；WixUI_Minimal + 占位 License.rtf；UpgradeCode 固定
  （E06CEF46-…）保证 MajorUpgrade 语义；快捷方式组件 GUID 固定。

## 验证
- AppImage 本机实测：构建成功（dist/Elsewhen-0.1.0-x86_64.AppImage），
  解包含 AppRun/desktop/图标/libelsewhen.so；release bundle 从 /tmp 启动
  日志「Applied main window chrome / App initialized」——证明 bundle/lib
  下的桥库 RPATH 加载成立（ioDirectory 路径在外地 CWD 必失败）。
- 两个 sh 脚本 `sh -n` 语法通过；msi 脚本（PowerShell）本机无法验证。
- 残留风险：macOS/Windows 未实测；WiX 仅覆盖 v3.11（v4/v5 的 heat 已移除/
  改名，脚本会明确报错提示安装 v3.11）。
