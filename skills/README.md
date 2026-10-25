# skills

本目录放**仓库自带的技能**（agent skill）：一段可复用的操作流程 + 必要的脚本与配置。
不是文档归档，也不是脚本目录 —— 判断标准只有一条：**「换个会话、换个人，照着它能把同一件事
再做一遍，并且能自己证明做对了」**。做不到就还只是笔记，别放进 `skills/`。

发现方式：`.pi/settings.json` 里 `"skills": ["../skills"]`，pi 启动时扫描本目录下的
`*/SKILL.md`。所以**目录名 = skill 名**，`SKILL.md` 的 frontmatter `name` 必须与目录名一致。

## 现有技能

| 技能 | 一句话 | 何时用 | 入口 |
|---|---|---|---|
| [`relay-deploy`](relay-deploy/SKILL.md) | 把仓库里的盲中继（relay）部署/更新/回滚到自建服务器，并**证明**换上去的确实是这份代码 | 改过 `relay/standalone/*` 之后要上线、要回滚、要体检线上 | `skills/relay-deploy/scripts/deploy.sh` |

### relay-deploy

- **用途**：本地构建镜像 → 传到 Tower（`192.168.101.116`）→ 用**同一份数据卷**重建容器 →
  三层验证（`/health` / 容器内模块 md5 与仓库比对 / 契约冒烟）。
- **用法**：
  ```bash
  ./skills/relay-deploy/scripts/deploy.sh              # 构建 + 部署最新代码
  ./skills/relay-deploy/scripts/deploy.sh --smoke      # 部署后再跑契约冒烟
  ./skills/relay-deploy/scripts/deploy.sh --rollback   # 回滚到上一个镜像
  ./skills/relay-deploy/scripts/verify.sh              # 只读体检，不动线上
  ```
- **配置全在文件里**：服务器地址、SSH 用户、容器名、端口、数据目录、配额都改
  `relay-deploy/config.env`；密钥类写 `relay-deploy/config.local.env`（已 `.gitignore`）。
  **脚本里没有常量** —— 换一台服务器不该改代码。
- **三条铁律**（写在这里，因为忘一次就会踩一次）：构建必须 `--network=host`；
  服务器上没有 node、没有仓库副本，只能 `docker save | ssh 'docker load'`；
  重建容器**不许**带回 `/releases` 挂载与 `RELEASES_DIR`，但数据卷 `/data` 必须照原样挂回去。
- **`/health` 绿 ≠ 部署成功**：旧镜像没有 `blob.mjs`/`sweeper.mjs` 时 healthcheck 照样绿，
  所以验证必须三层，见 `relay-deploy/references/runbook.md`。
- **已知不做的**：线上**不跑** blob 数据面冒烟（会往生产信箱写真实分片）；数据面验收在本地
  standalone relay 上跑（`docs/roadmap/2026-10-24-relay-v3-dataplane-roadmap.md` 的验收节）。

## 写新技能

1. **目录名 = 技能名**（小写、连字符），新建 `skills/<name>/SKILL.md`。
2. **frontmatter 必须有两项**：
   ```yaml
   ---
   name: <name>
   description: <干什么 + 何时用，第三人称，一句话说清触发条件>
   ---
   ```
   `description` 是唯一的自动触发依据 —— 写成「关于部署的一些说明」就永远不会被用上；
   要写成「部署/更新/回滚 elsewhen relay 到自建服务器：… Use when deploying…」。
3. **配置与脚本分开**：可变的东西（地址、端口、路径、配额、tag）进 `config.env`；
   敏感项进 `config.local.env` 并加进 `skills/<name>/.gitignore`。脚本只读配置，不写常量。
4. **每个脚本要能自己判断成败**：打印「我验了什么、期望什么、实际什么」，退出码非 0 即失败。
   「跑完没报错」不算验证（relay 的 `/health` 就是反例）。
5. **破坏性动作必须显式开关**：`--rollback`、`--force` 这类要写明会动什么、能不能回退。
6. **写完立刻更新本文件的表格**（技能名 / 一句话 / 何时用 / 入口），
   并同步 `docs/proposal-backlog.md`（若该技能来自某条提案）。

## 与其它目录的分工

| 目录 | 放什么 |
|---|---|
| `skills/` | 可复用的**操作流程**（怎么安全地做某件事），带脚本与配置 |
| `docs/` | 事实与决策：需求、roadmap、方案 note、review、实现缺口 |
| `scripts/` | 产品自身的构建/打包/桥接脚本（`regen.sh`、`package-*.sh` 等），与「怎么运维一台服务器」无关 |

一个改动同时要动 `scripts/` 与 `skills/` 时，注意区分：`scripts/` 是**造东西**，
`skills/` 是**把造出来的东西搬到某台机器上并证明它在那儿**。
