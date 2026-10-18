# 外部工具与技能 / External Tools & Skills

让 AI 不只聊天，还能**调用外部工具**帮你做事——查网页、连你自己的服务、跑你定义的小脚本。为安全起见，**每次调用都先问你**。

Let the AI do more than chat: it can call **external tools** — search the web, connect to your own services, run small scripts you define. For safety, **every call asks you first**.

## 添加工具（MCP）/ Add a Tool (MCP)

设置页的 **MCP 工具** 区块点「添加」。MCP 是 AI 工具的通用标准，分两种：

- **本地命令**：在你这台电脑上跑的程序；
- **远程服务**：填一个 http(s) 地址，直接连到网上的工具。

Add servers in the **MCP Tools** section of Settings:

- **Local command**: a program that runs on this computer;
- **Remote service**: an http(s) endpoint you connect to directly.

给工具起名字、填好命令或地址、保存后打开开关即生效。Name it, fill in the command or URL, save, and switch it on.

### 适用平台 / Platform

每个工具可以标「全部 / Linux / macOS / Windows」——标了某个平台的工具只在对应电脑上激活。手机上**本地命令类的工具不可用**（手机上没有那些程序），但远程服务可以直接用。Label each tool All / Linux / macOS / Windows — a platform-specific tool only activates on matching machines. On the phone, **local-command tools are unavailable** (those programs don't exist there), while remote services work directly.

## 审批与信任 / Approval & Trust

AI 每次要调用工具，会先弹出**审批卡**：显示工具名、要执行什么、参数是什么。你选**允许**或**拒绝**。Each time the AI wants to call a tool, it shows an **approval card**: which tool, what it will do, with which arguments. You **allow** or **deny**.

用得顺手的工具可以标记**信任**（盾牌图标）——之后调用不再询问。改主意随时点回来恢复「每次都问」。For tools you use often, mark them **trusted** (shield icon) and calls stop asking. Change your mind anytime to return to ask-every-time.

启用的工具旁边有开关，随时停用——停用后 AI 完全看不到它。Each tool has an on/off switch — when off, the AI can't even see it.

## 技能 / Skills

**技能**是你教给 AI 的小本领：一段脚本，放在技能目录里，AI 需要时可以执行。技能和工具走同一套审批机制，同样可以标记信任。

**Skills** are little abilities you teach the AI: a script in your skills folder the AI can run when needed. Skills use the same approval system as tools, and can also be marked trusted.

## 多设备行为 / Across Devices

- 工具的**定义**（名字、地址、配置）会同步到你的其他设备，不用重复配置；
- 但两样东西**不跟同步**：**密钥头**（需要时在另一台设备重新填一次）和**信任状态**（每台设备自己决定哪些工具免审批）。

- Tool **definitions** (name, URL, config) sync to your other devices — no re-configuring;
- Two things never sync: **secret headers** (re-enter them once per device) and **trusted status** (each device decides for itself which tools skip approval).
