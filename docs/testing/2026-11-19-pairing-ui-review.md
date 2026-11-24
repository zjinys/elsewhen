# 配对 UI 与流程检查（2026-11-19）

本次检查当前 Flutter 页面、控制器及现有测试，先用四个合成状态场景复现问题，再按下方结论落地产品修复。未连接个人数据、真实相机或线上中继；视觉结论基于组件布局和手机宽度截图，真人手机操作与线上中继验收仍待完成。

## 已复现的流程问题

| 优先级 | Before（当前） | After（建议） | Why |
| --- | --- | --- | --- |
| P1 | owner 审批成功清掉 `invitation`，但步骤、快照进度、交接错误只在 `offer != null` 分支显示；批准后又看到创建邀请表单 | 活跃连接页面以本次 invitation / enrollment 身份驱动，批准后切到同一张卡的同步阶段 | 当前最需要反馈的时候进度区恰好消失，用户可能误以为没成功而再次创建邀请 |
| P1 | 新设备停在 `awaiting_approval`，没有自动检查审批；需要点击“检查审批” | 用户发出加入请求后，由控制器自动轮询；收到审批后沿既有受保护存储确认流程继续 | owner 已批准并不意味着新设备会自动进入下一阶段；用户不知道还要自己按一次按钮 |
| P2 | 任意历史 owner enrollment 为 complete，就把当前新邀请显示成五步全部完成 | 完成、进度、错误都绑定当前 invitationId，历史记录不参与当前流程判断 | 第二次添加设备时出现虚假完成状态 |
| P2 | 第一次“准备本机”后，`configured` 变为 true，未手动选过 tab 就自动落到“管理” | 在用户选定的“连接新设备”流程内完成准备，然后直接出示邀请 | 用户刚完成前置步骤，下一步入口却被页面隐藏 |

代码依据：

- [审批状态](../../ui/lib/sync/sync_setup_pairing.dart)的 `approveInvitation` 清空邀请；[owner 页面](../../ui/lib/widgets/owner_sync_pairing_settings.dart)的 `offer == null` 分支展示创建表单，else 分支才展示 `SyncPairingSteps`、进度和 `enrollment.error`。
- [加入控制器](../../ui/lib/sync/sync_join_controller.dart)的 `_resumeEnrollment` 不驱动 `awaiting_approval`，`inspectApproval` 目前由[加入页面](../../ui/lib/widgets/joiner_sync_pairing_settings.dart)按钮触发。owner 的邀请请求轮询位于 widget 的 3 秒 timer，离开页面即停止；审批前的网络协调应改由控制器持有，并遵循取消/关闭边界。
- owner 页的 `deviceJoined` 使用 `.states.any(role == owner && phase == complete)`，未核对当前邀请 ID；发送快照和阻塞信息也未全部按本次邀请筛选。
- [同步设置](../../ui/lib/widgets/local_sync_settings.dart)根据 `configured ? manage : bind` 选择默认页签。

复现用例的日志为 `/tmp/elsewhen-pairing-ui-audit.log`，原始源码归档为 `/tmp/elsewhen-pairing-ui-audit-repro.dart`；产品回归契约见 `ui/test/sync_pairing_flow_test.dart`，覆盖审批后进度、邀请隔离、自动轮询、关闭竞态、过期恢复和扫码回退。

## 页面与信息组织建议

| Before（当前） | After（建议） | Why |
| --- | --- | --- |
| 同步健康区、状态区、绑定/管理切换、准备本机、邀请与加入表单层层堆叠 | 已连接设备为主页面；未配置时直接给“连接新设备 / 加入已有设备”两个意图入口；配对在独立持续页面内完成 | 用户先表达要完成的事，再按顺序填写相关信息 |
| 手机上“通过 Key”是主按钮，扫码是次按钮 | 有可用扫码能力的手机以“扫描二维码”为主操作；保留“粘贴邀请”回退；桌面按可用能力选择 | Key 既容易与 API Key 混淆，也没有说明邀请是可粘贴的文本 |
| owner 先填写“你已部署的中继 URL”，已连接数据集也会优先读本机上次邀请偏好 | 优先使用当前数据集实际生效的中继；首次缺配置时明确引导设置，已配置时折叠到“连接设置” | 无需每次加设备都重新理解基础设施，也避免历史偏好与当前路由冲突；没有可用默认服务时不能假装可直接连接 |
| 多处确认重复解释 Provider、API Key、暂存库、安装证明和名册 | 把共享数据和 API Key 的影响保留在授权决定附近；按钮改为“连接这台设备 / 允许连接”，协议细节进“连接详情” | 用户仍明确授权，但不必理解内部协议才能完成操作 |
| 五个技术阶段、转圈箭头、另一个全局 busy 进度条与通知文案并列 | 三个用户阶段：识别设备 → 双端确认 → 同步数据；每一屏一个主要动作，明确“请在另一台设备上允许连接” | 让等待对象与动作负责人一眼可见；“等待用户操作”不伪装成正在计算 |
| 长公钥指纹夹在展开的身份详情中，核对动作离主要内容远 | 设备图标、名称、系统优先展示，核对信息靠近授权按钮；完整指纹仍可查看/复制 | 核对必须能执行，而不是形式性勾选；若引入两端同样的短校验码，需要绑定经认证的配对会话/握手材料，不能只截一段哈希或移除身份验证 |
| 数据传输显示分片 `received/total`，部分阶段没有总量；故障状态与重试按钮散落 | 有总量时显示真实比例及“已接收/总计”；准备阶段说明“正在准备数据”；断线时保留进度并显示“等待主设备，恢复后继续” | 不虚构百分比、速度或剩余时间；分片细节可折叠，恢复入口就近放置 |
| 正常等待时并列“重发原请求 / 检查审批 / 重发安装证明 / 停止加入” | 正常轮询和幂等重发自动推进；失败后按原因给“重试连接 / 更换邀请 / 继续上次连接”；取消单独放次级位置 | 不把网络协议维护交给用户，也减少重复点击和误取消 |
| 邀请二维码只显示绝对到期时间，过期后可能仍保留二维码并反复报错 | 有效期显示剩余时间；过期明确置灰并给“重新生成邀请”，且区分已批准交接与未批准邀请的有效期 | 用户知道旧码不能继续用；不能因为邀请过期就错误中止已经批准的交接 |
| 加入完成和“进入数据集”分两处显示，点击进入后仍要求完全退出重开 | 完成页明确“已连接，首次数据已准备”；基于当前启动固定数据库的约束，把动作命名为“选择并重启后打开”，或实现受控的重启再进入 | “已连接”与“当前正在使用同步数据”是两个不同事实；不能 UI 上假称已切换 |

## 状态可信度的相关问题

[同步健康区](../../ui/lib/widgets/sync_workspace.dart)把活跃授权成员数量同时作为在线人数和总人数，实际没有在线依据；`tick.ran` 也只代表本地同步轮次运行过，不代表每个远端当前在线。建议没有可靠活跃时间/回执依据时，写“已连接 N 台设备”，并逐设备显示最后成功通信或“暂未确认”；不再用名册计数宣称在线。

名册查询目前是 `FutureBuilder(future: api.getSyncRoster())`，没有明确订阅配对/名册修订。配对完成后应刷新同一份设备读模型，确保设备列表、状态头和成功页面同源。

## 推荐交互顺序

**已有设备**：选择“连接新设备” → 如有必要准备本机并确认共享范围 → 出示二维码/复制邀请 → 收到请求就显示对方设备，核对并允许 → 持续展示首次同步 → 本次连接完成，可选择“再连接一台”。

**新设备**：选择“加入已有设备” → 扫码或粘贴邀请 → 核对邀请设备与共享影响，确认本机名称 → 发起连接 → 自动等待另一台设备允许 → 自动完成受保护存储确认和首次同步 → 明确选择同步数据并按当前实现重启打开。

视觉上采用一张主卡：顶部显示两台设备的身份，中间突出当前阶段、真实进度和下一步指引，底部一个主动作。完整步骤、relay、指纹、分片、修订号和诊断日志放入可展开详情。首次同步等待时说明两端需保持运行，进入后台的持续行为依据平台实际能力表述；不承诺系统杀进程后仍在传输。

建议顺序：先修四个已复现状态问题与在线状态误报，再收敛意图入口和持续配对页面，最后改善核对信息、过期/恢复、完成后的进入动作。保留当前后端安全边界和逐跳确认，不因简化 UI 省略实际授权或持久化确认。


## 2026-11-19 UI implementation follow-up

- Pairing now starts from two intent cards: connect a new device or join an existing dataset.
- Owner and joiner screens use three user-facing stages: identify devices, confirm on both devices, and sync data. Internal enrollment phases remain in the controller only.
- Approval keeps the device identity and transfer/error state visible; enrollment errors are scoped by operation and invitation ID. QR codes are hidden when expired, with regeneration; scanning exposes paste fallback.
- Joiner automatically resends the request/proof or checks approval on a bounded timer. Closing/canceling stops timers before releasing the native operation.
- The active workspace reopens on the pairing tab, roster counts say connected devices rather than online, and snapshot progress uses a real percentage only when a total is known.
- Validation: pairing-flow regression and the existing pairing/sync/localization suites pass; `scripts/check-localization.py` and ARB parity pass.
