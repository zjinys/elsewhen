# elsewhen · 别时

> 本地优先的个人认知主循环 —— 记录此刻，复利此生。

**Elsewhen 是一个跑在你自己电脑上的个人认知系统**：把随手记录、对话和网页剪藏沉淀为"关于你的持久知识"，而不是一堆一次性聊天记录。Rust 本地核心 + Flutter 跨平台界面，数据 100% 存在本机（SQLite 单文件），AI 只经你配置的接口访问；多设备间端到端加密同步，中继只中转密文。

```text
随手记录 → AI 理解 → 人物 / 项目 / 认识沉淀 → 每日回顾 / 后续行动
保存内容 → 阅读理解 → 观点 / 灵感 / 创作素材 → 采用或归档
```

## 核心亮点

- **原生速度，本地优先** —— SQLite 单文件本地库，无云端账号、无遥测，离线可用；`ELSEWHEN_DATA_DIR` 环境变量即可便携携带
- **知识复利，不是聊天碎片** —— 采用 LLM Wiki 模式（"编译一次，持续更新"）：AI 把事件消化成持久知识页（成本 / 能力 / 资产 / 项目 / 关系 / 决策 / 习惯…），同一事实被 N 条事件支持时证据计数=N，页面向"更懂你"累积
- **原始事件不可变** —— 你的记录是你的日记，永不被 AI 覆盖；知识页全部是派生数据，可随时重新消化重建
- **AI 只提议，核心决定** —— 页面合并规则由 Rust 核心执行（kind/slug 校验、证据并集、`evidence_count` 等事实数字 AI 改不了）；人工编辑过的页面受保护；AI 的修订建议走审阅卡，你点头才入库
- **端到端加密多设备同步** —— 设备配对后差量追平，中继只中转密文读不到任何明文；支持同一设备临时挂载另一数据集
- **多 Provider 容错接入** —— 任意 OpenAI 兼容上游，多配置故障转移；API 密钥 AEAD 密封在本机（同步内容不含明文 Key）
- **无感后台自动化** —— 事件分析与知识消化由后台 worker 完成，失败自动指数退避，队列与运行日志全量只读可查

实施进度见 [`docs/roadmap/2026-09-17-personal-cognition-main-loop-roadmap.md`](docs/roadmap/2026-09-17-personal-cognition-main-loop-roadmap.md)。

## 快速上手

构建与启动（桌面，以 Linux 为例）：

```bash
cargo build --release                 # Rust 桥接库
cd ui && fvm flutter pub get
cd .. && ./scripts/debug-run.sh       # 或 flutter build linux 后运行
                                      # ui/build/linux/x64/release/bundle/elsewhen_ui
```

1. **配 AI**（设置 → AI Provider）：填 Base URL / API Key / 模型，保存即生效。未配置时记录与分析照常入库，仅 AI 环节排队等待；
2. **随手记录**：主对话流直接说，或用 Capture 浮窗速记（`--dart-entrypoint-args "--mode=capture"`）；
3. **知识库**：记录约 10 分钟沉淀后由 AI 自动消化成知识页；修订建议在审阅卡里逐条采纳；
4. **回顾**：每日回顾与 AI 给出的后续行动；
5. **多设备**（可选）：设置 → 同步，Owner 出示配对码、Joiner 扫码，之后新数据自动双向追平。

## 架构

- **Rust 核心** (项目根目录)：事件存储、AI 分析、知识消化、SQLite 数据库；只构建为 Flutter 桥接库（不再提供 CLI）
- **Flutter UI** (`ui/` 目录)：现代化跨平台 GUI，支持 Linux/macOS/Windows 桌面和 Android/iOS 移动端
- **端到端加密同步** (`src/sync/`)：多设备点对点差量同步，中继服务器只过密文

目前 Flutter UI 已包含对话、知识库、待办、Capture、AI Provider 设置、网页/文本导入、人物关系和知识页 AI 加工。知识页支持标签、来源分区和派生产物关联。

## Flutter GUI (推荐)

Flutter UI 提供更现代的体验，支持两种模式：

```bash
cd ui/

# 主应用模式：完整的事件时间线（桌面入口在 lib/main_desktop.dart）
fvm flutter run -d linux --target=lib/main_desktop.dart

# Capture 模式：快速录入浮动窗口
fvm flutter run -d linux --target=lib/main_desktop.dart \
  --dart-entrypoint-args "--mode=capture"
```

也可以用 `scripts/debug-run.sh`（日志同时写入 `~/.local/share/elsewhen/agent-debug.log`）；日常使用建议直接运行 release bundle：`ui/build/linux/x64/release/bundle/elsewhen_ui`（需先 `cargo build --release` + `flutter build linux`，见下文构建）。

详细文档见 [`ui/README.md`](ui/README.md)。

测试或便携运行时可设置 `ELSEWHEN_DATA_DIR=/path/to/data` 覆盖默认平台数据目录。

## 后台事件分析

事件保存时会在同一 SQLite 事务中创建分析任务，应用内的后台 worker 自动处理，不阻塞记录。

AI Provider 配置保存在数据库 `ai_provider_configs` 表（唯一配置源），在应用内 **设置页** 填写 Base URL、模型和 API key，保存即生效。未配置 Provider 时，分析任务保留在队列中，不影响原始事件。

Provider 不可用、超时或返回非法 JSON 时，Raw Event 不受影响，任务进入指数退避重试状态。

分析队列的 pending / running / retry / succeeded / failed 数量可在 Flutter **设置 → 数据 → 事件分析队列** 查看。

## LLM Wiki 个人知识库与认知推微

Elsewhen 内置一个 **LLM Wiki 个人知识库**（模式来自 Karpathy 的 [LLM Wiki](https://gist.github.com/karpathy/442a6bf555914893e9891c11519de94f) 思路）：
"编译一次，持续更新"——AI 把随手记录的事件消化成**关于你的、持久复利的知识页**，
而不是每次查询时从原始记录重新推导。人类负责记录与提问，AI 负责整理与维护。

三层结构：

- **原始事件**（`events` 表，不可变）：你的日记，事实来源
- **wiki 页面**（`wiki_pages` 表，AI 维护 + 核心确定性合并）：`recurring_cost` / `capability` / `asset` / `project` / `relationship` / `decision` / `habit` / `constraint` / `insight` 等页面，markdown 正文，带**事件溯源**与**证据计数**
- **schema**（系统提示词 + Rust 合并规则）：约束 AI 成为"守纪律的维护者"，AI 只提议，核心决定入库

知识消化全自动，没有手动触发入口：

- 每条记录保存时同事务登记一个消化任务，约 10 分钟沉淀窗后（等事件分析完成、给你改「可记录性」的时间）由后台 worker 分批消化；
- 闲聊、元对话等被判定为不可记录的事件自动跳过，改回可记录后重新排队；
- 模型失败或返回不合法时整批不写入、指数退避重试，连续失败 5 次后冷却 6 小时再自动重试；
- 队列明细与每批运行日志（新建/更新/受保护页面、耗时、失败原因）在 **设置 → 数据 → 知识消化** 只读查看。

核心机制：

- **证据复利**：同一个事实被 N 条事件支持时 `evidence_count=N`，页面向"更懂你"累积；
- **确定性合并**：AI 建议的页面变更由核心校验（kind/slug 枚举、来源事件必须属于本批）后以事件 id **并集**合并，AI 不能改数字；整批页面变更、日志与任务确认同一事务提交；
- **人工编辑保护**：你编辑过的页面不会被后台覆盖，新证据只累加；
- **原始事件永不被覆盖**，wiki 全部是 derived data，可随时重新消化重建。

认知推微（四透镜洞察）将在 FR-PES-004 阶段 3 与共享选材一起接入并自动化。设计细节见 [`docs/llm-wiki.md`](docs/llm-wiki.md) 与 [`FR-PES-004`](docs/requirements/product/FR-PES-004-LLM-Wiki知识库闭环.md)。

## 构建

```bash
cargo build --release
```

产物为 Flutter 桥接动态库（Linux `target/release/libelsewhen.so`）。三个平台由 `.github/workflows/ci.yml` 编译和测试。

Linux 上安装一次桌面元数据，使窗口合成器能解析应用 ID 到图标：

```bash
./scripts/install-linux-desktop.sh
```

发行版安装包（均从源码构建后打包，不会安装到打包容器的系统）：

```sh
./scripts/package-deb.sh        # Debian/Ubuntu，dist/*.deb
./scripts/package-rpm.sh        # Fedora/RHEL，dist/*.rpm
./scripts/package-pacman.sh     # Arch，从源码构建（CI 走另外的快路径，见下）
./scripts/package-appimage.sh   # AppImage
scripts/package-msi.ps1         # Windows MSI（WiX v3.x）
./scripts/package-dmg.sh        # macOS DMG（Apple Silicon）
```

安装包包含 Flutter 应用、桥接库、桌面入口和正式品牌图标（`assets/brand/elsewhen-icon-v2-256.png`）。

也可以通过 Makefile 调用：

```sh
make release          # 构建 release 二进制
make deb              # Debian/Ubuntu
make rpm              # Fedora/RHEL
make pacman           # Arch Linux
make package          # 构建全部格式（要求三种打包工具都已安装）
```

所有记录入口都不能绕过 `Store::insert_event` 的本地提交边界。

## 多设备同步

数据在设备间端到端加密同步：中继只中转密文信封，Service 端读不到任何明文；本地库仍是唯一真相源，同步是差量追平而非镜像替换。

- **配对**：设置 → 同步里发起，Owner 出示配对码、Joiner 扫码加入，双方互存设备身份；
- **实时追平**：对端写入的数据在数秒内到达本端，会话/知识页就地刷新，无需重启；
- **数据集切换**：支持在同一设备上切换/临时挂载另一数据集（迁移试运行等场景）。

设计细节与当前进度见 [`docs/roadmap/2026-10-03-multi-device-sync-roadmap.md`](docs/roadmap/2026-10-03-multi-device-sync-roadmap.md) 与手工测试清单 [`docs/roadmap/2026-10-14-sync-manual-test-roadmap.md`](docs/roadmap/2026-10-14-sync-manual-test-roadmap.md)。

## 开发

### Flutter UI 开发

```bash
cd ui/
fvm install                    # 安装 Flutter 3.47.4
fvm flutter pub get            # 安装依赖
fvm flutter analyze            # 代码分析
fvm flutter test               # 运行测试
fvm flutter run -d linux       # 运行应用
```

### CI 与发布

- [`.github/workflows/ci.yml`](.github/workflows/ci.yml)：push 时三平台编译与测试；
- [`.github/workflows/release.yml`](.github/workflows/release.yml)：`workflow_dispatch` 手动验证全平台构建；推送 `v*` tag 时正式发布——Linux（deb/RPM/AppImage/pacman）、macOS（DMG）、Windows（MSI）、Android（APK）自动构建并附到 GitHub Release。

### Rust 核心开发

```bash
cargo test                     # 运行测试
cargo build --release          # 构建 release 桥接库
```

Rust API 发生变化后运行 `scripts/regen.sh`，它会重新生成 Flutter-Rust Bridge 代码并重建 release 动态库。真实 Bridge 测试使用独立临时数据库，测试代码不得依赖个人数据目录已有内容。

## 文档

- [Flutter UI README](ui/README.md) - Flutter GUI 开发指南
- [CLAUDE.md](CLAUDE.md) - Claude Code 开发指南
- [需求文档](docs/requirements/) - 产品需求和架构设计

## 许可证

项目以 [GNU AGPL-3.0](LICENSE) 发布。第三方组件与依赖的许可说明见 [NOTICE.md](NOTICE.md)，
其中 vendored 编辑器 `ui/third_party/appflowy_editor` 沿用上游双许可（AGPL-3.0 OR MPL-2.0）。
