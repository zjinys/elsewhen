# 用 Skill + Codex 快速完成视频二创的方法

## 背景
把国内视频二次创作后搬运到 YouTube（不直接搬运），用 Codex 类 Agent 配合 skill 提效。

## 核心工具

### 1. claude-video（/watch skill）——让 Agent「看懂」视频
- 安装：`npx skills add bradautomates/claude-video -g`（支持 Codex、Cursor 等 50+ 宿主；Claude Code 用 `/plugin install watch@claude-video`）
- 功能：贴视频链接或本地文件 → 自动抓字幕 → 按场景抽帧 → 输出带时间戳的逐字稿
- 零配置：yt-dlp 和 ffmpeg 首次运行自动安装
- 成本：多数公开视频用免费字幕即可；无字幕才需 Whisper API
- 意义：Agent 本来只能靠标题猜视频内容，装上后能真正「看」懂内容结构，这是二创的第一步

### 2. Video Processor skill——流水线脏活
- yt-dlp 下载 + ffmpeg 抽音频/转格式 + Whisper 转写

## 二创流水线
1. **拉源**：yt-dlp 下载国内源视频
2. **看懂**：/watch 让 Codex 抽取字幕 + 关键帧，生成带时间戳的逐字稿
3. **出方案**：让 Codex 基于逐字稿设计二创方案（改头改尾、加解说、重新剪辑结构）
4. **执行**：ffmpeg 按方案重剪
5. **出海**：加英文字幕（可让 Agent 翻译逐字稿生成 SRT）

## 待补充（跑通后回填）
- 实际可用的下载参数（分辨率/格式）
- 抽帧间隔、字幕生成实测效果
- YouTube 审核对二创程度的实际反馈