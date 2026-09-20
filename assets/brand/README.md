# 品牌说明 / Brand

## 名称
- 英文（字标）：**Elsewhen**
- 中文名：**别时**
- 释义：else（别的）+ when（时刻）的拼合词，本义"在别的某个时刻"，对应 elsewhere（别处）只是把"地点"换成"时间"。
  契合产品理念——把零散的、不在此刻的时刻，沉淀成关于你自己的、持续复利的知识。
- Slogan：**记录此刻，复利此生。**
- 备选文案：把那些别处的时刻，变成更懂你的自己。

## 视觉
- 母题：开口的**时间轨道** + 偏移的"else"圆点（一个不在此刻的瞬间）。
- 色板：深蓝 `#171A21` → 靛蓝 `#5B63D3`（轨道渐变）；图标底色 `#16171C`–`#23252C`。
- 字体：Inter（字标），几何无衬线。

## 资产
- `elsewhen-logo-v2.svg` / `elsewhen-logo-v2.png` —— 横向字标组合（透明底，白底页面 / 文档用）
- `elsewhen-icon-v2.svg` / `-1024.png` / `-256.png` —— 暗色满铺方形 App 图标（直角、无四角留白）

## 落地
- Linux 桌面：任务栏 / 窗口图标由 GTK runner 内嵌 `ui/linux/runner/app_icon.png`（v2-256）设置；启动器图标走 `.desktop` + hicolor 主题（`scripts/install-linux-desktop.sh` 等打包/安装脚本均指向 v2 资产）。
- `ui/ios/.../AppIcon.appiconset` 与 `ui/android/.../mipmap-*` 等移动端图标同样由 v2-1024 导出。
- 旧资产 `assets/icons/elsewhen.svg` 等已无引用，仅作历史存档。
