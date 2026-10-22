# 品牌说明 / Brand

## 名称
- 英文（字标）：**Elsewhen**
- 中文名：**别时**
- 释义：else（别的）+ when（时刻）的拼合词，本义"在别的某个时刻"，对应 elsewhere（别处）只是把"地点"换成"时间"。
  契合产品理念：把零散的、不在此刻的时刻，沉淀成关于你自己的、持续复利的知识。
- Slogan：**记录此刻，复利此生。**
- 备选文案：把那些别处的时刻，变成更懂你的自己。

## 视觉
- 母题：开口的**时间轨道** + 偏移的"else"圆点（一个不在此刻的瞬间）。
- 色板：深蓝 `#171A21` → 靛蓝 `#5B63D3`（轨道渐变）；图标底色 `#16171C`–`#23252C`。
- 字体：Inter（字标），几何无衬线。

## 资产
所有图标共用同一套轨道几何：`r=196`、`stroke=44`、圆点 `r=28`、光晕 `r=62`。
只有容器和导出尺寸会按平台调整，不再维护第二套比例。

- `elsewhen-icon-v2.svg` / `-1024.png` / `-256.png` —— 圆角深色底 App 图标（桌面、macOS、Linux 启动器、DMG、窗口/任务栏图标）。
- `ui/assets/brand/elsewhen-icon-android.svg` —— 满幅直角深色底（iOS、Android legacy 图标导出；平台自己做圆角/圆形裁切）。
- `ui/assets/brand/elsewhen-splash.svg` —— Android 12 启动图：同一枚圆角图标缩进 192dp 安全圈。
- `ui/assets/brand/elsewhen-tray-v2.png` / `.ico` —— 系统托盘图标：与窗口/任务栏图标同源同图，仅导出尺寸更小；不再维护透明背景的第二套样式。`-v2` 后缀同时用于打破桌面外壳按文件路径缓存旧托盘图标的问题。
- `ui/assets/brand/elsewhen-adaptive-foreground.svg` —— Android adaptive icon 前景层（透明，轨道缩进 72dp 安全圈）。
- `elsewhen-logo-v2.svg` / `elsewhen-logo-v2.png` —— 横向字标组合（透明底，白底页面 / 文档用）。

## 生成
所有位图都由上面的 SVG 统一导出，不要单独手改 PNG/ICO：

```bash
./scripts/generate-brand-assets.sh
```

依赖 `rsvg-convert` 与 ImageMagick。脚本会生成 macOS/iOS AppIcon、Android launcher 与 splash、Linux/Windows 任务栏图标、托盘 PNG/ICO，以及打包脚本引用的根级 1024/256 PNG。

## 落地
- Linux 桌面：任务栏 / 窗口图标由 GTK runner 内嵌 `ui/linux/runner/app_icon.png`；启动器图标走 `.desktop` + hicolor 主题，打包与安装脚本统一使用根级 `elsewhen-icon-v2-256.png`。
- Windows：任务栏与 MSI 图标使用 `ui/windows/runner/resources/app_icon.ico`。
- macOS：`ui/macos/.../AppIcon.appiconset`；DMG 使用根级 `elsewhen-icon-v2-1024.png`。
- iOS：`ui/ios/.../AppIcon.appiconset`，由满幅源导出，交给系统裁切。
- Android：legacy `mipmap-*/ic_launcher.png` + `mipmap-anydpi-v26/ic_launcher.xml` adaptive icon；启动图 `mipmap-*/ic_splash.png`。
- 旧资产 `assets/icons/elsewhen.svg` 等已无引用，仅作历史存档。
