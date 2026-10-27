---
name: android-app-update
description: 把当前仓库编出来的 Elsewhen APK 构建、安装/更新到 Android 设备上，并证明装上去的确实是这份代码（无 root、无 run-as 也可用）。包含「设备上那版要不要更新」的只读体检。Use when building or installing/updating the elsewhen Android app on a device, when asked "手机上的 app 要不要更新", or when a Rust/Flutter change must land on the phone.
---

# Android app 构建与更新

桌面是 owner、手机是 joiner，两边必须跑同一份代码：Rust 内核版本不一致时，
同步协议、迁移版本、桥接 hash 都可能对不上，而**这些差异从界面上看不出来**
（app 照常启动、同步照常报「正在同步」）。这个 skill 负责把手机换成当前仓库的
APK，并且**证明**换上去的确实是它。

## 三条命令

```bash
./skills/android-app-update/scripts/check.sh                # 只读体检：要不要更新
./skills/android-app-update/scripts/build.sh                # 只编译，不碰设备
./skills/android-app-update/scripts/install.sh              # 构建 + 安装 + 复核
```

常用开关：

| 命令 | 作用 |
|---|---|
| `check.sh --build` | 先构建再比（改完 Rust 的常规做法） |
| `install.sh --no-build` | 只装本地已有的 APK |
| `install.sh --dry-run` | 只打印将要执行什么，不动设备 |
| `install.sh --relaunch` | 装完冷启动一次并确认进程活着（**首次跨迁移版本升级建议加**） |
| `APK_DEVICE=<serial> ...` | 临时指定设备（优先级最高） |

退出码：`0` = 已是最新／操作成功，`1` = 需要更新（check）或失败，`2` = 环境不满足。

## 配置

全部在 `config.env`，脚本里没有常量：设备序列号、`applicationId`、
允许的签名证书 SHA-256、flutter 命令、是否总是重建、Android SDK 位置。
敏感项写 `config.local.env`（已 `.gitignore`）。

**值是带空格的字符串时必须加引号**：`APK_FLUTTER_CMD="fvm flutter"`。
不加引号在 shell 里会被读成「以 `APK_FLUTTER_CMD=fvm` 为环境变量执行 `flutter` 命令」——
赋值不生效，还会真去跑那条命令（写这个 skill 时每次 source 都在跑 `flutter help`，藏得很深）。

**环境变量优先于配置文件**：`APK_DEVICE=xxx ./scripts/check.sh` 会覆盖
`config.env` 里的设置（脚本先把环境变量摘出来，再 source 配置，最后回填）。

## 怎么判断「要不要更新」

不看时间戳，只看 APK 里的三样东西：

| 判据 | 从哪来 | 为什么可信 |
|---|---|---|
| `libelsewhen.so` md5 | 拉回设备上已装的 `base.apk` 解出 `.so` | 与本地 APK 逐字节比，不同就是不同 |
| `libapp.so` md5 | 同上 | Dart AOT 产物，说明业务代码是否一致 |
| **认得哪些迁移** | `strings libelsewhen.so` ∩ 仓库 `migrations/` 清单 | `rusqlite_migration` 把 `migrations/` 编进二进制，所以名字是硬证据 |

第三个判据的价值在「差在哪」：`6261f1ba… ≠ 8ab4…` 说明不了问题，
「设备上那版不认识迁移 `63-relay-v3-review-f5`」直接指出手机升级后会先跑哪一步。

**不要用裸正则从二进制里猜迁移名**：`strings` 会把任意 ≥4 字符的可打印序列切出来，
`67-s`、`35-w` 这种杂串到处都是，按 `^[0-9]{2}-[a-z-]+$` 取「最大版本」得到的
是垃圾（本 skill 第一版就这么错过）。可信集合只有仓库 `migrations/` 目录本身。

## 安全边界（它不会做的事）

- **不卸载、不清数据**：只用 `adb install -r`。手机上的库靠 app 自己首次启动的迁移
  从旧版本升上来（迁移只追加、`01-baseline` 冻结、`user_version` 只增）。
  要卸载是破坏性操作，脚本只会**打印**命令让你自己执行。
- **不做备份，因为做不到**：设备未 root（`adb root` 在量产版上直接拒绝），
  app 非 debuggable，`run-as` 报 `package not debuggable` —— adb 拿不到
  `/data/data/<pkg>`。需要备份就用应用内的导出功能。
- **不猜设备**：adb 上有两台时直接报错并要求显式指定。猜错就是把 APK 装到别的机器上。
- **签名先比对再装**：覆盖安装遇到不同证书只会得到 `INSTALL_FAILED_UPDATE_INCOMPATIBLE`，
  不告诉你哪张证书不一样。装之前先比 `Signer #1` SHA-256。

## 复核（为什么必须做）

`adb install` 打印 `Success` **不等于**设备上跑的是这个 APK。所以 `install.sh`
装完会把已安装的 `base.apk` 重新拉回来，再比一次 md5 与迁移名：

```
libelsewhen.so  本地 6261f1baa93a7d2d2913531e8c469055
                设备 6261f1baa93a7d2d2913531e8c469055
最大迁移版本    本地 63-relay-v3-review-f5 / 设备 63-relay-v3-review-f5
✓ 设备上运行的确实是这份 libelsewhen.so
```

这条规则跟 `relay-deploy` 是同一条：**`/health` 绿 ≠ 部署成功，
`install` Success ≠ 装对了**。判断「线上/设备上是不是这份代码」的唯一依据是
产物逐字节比或语义等价比。

首次跨迁移版本升级时加 `--relaunch`：进程起不来最常见的原因就是首次启动的迁移失败，
这时取证看 `adb logcat -d | grep -iE 'elsewhen|migration|panic'`。

## 踩过的坑（写进脚本了）

1. **`strings | grep -q` 永远返回未命中**：`grep -q` 命中即退出，上游 `strings`
   收到 SIGPIPE，配合 `set -o pipefail` 整条管道被判失败 —— 「明明存在」被读成
   「不存在」。改用纯 shell 的 `case "$s" in *"$sub"*)`。
2. **`APK_FLUTTER_CMD=fvm flutter` 跑成了命令**（见上「配置」节）。
3. **空值的配置项会覆盖环境变量**：`. config.env` 是无条件赋值，`APK_DEVICE=`
   让 `APK_DEVICE=xxx ./script` 静默失效。脚本现在先摘环境变量、最后回填。
4. **空参数被当成 target 文件**：`set -- "${BUILD_ARGS[@]:-}"` 在数组为空时
   会展开出**一个空字符串参数**，透传给 `flutter build apk` 后变成
   `Target file "" not found.` —— 报错与真正原因（配置全默认、没有任何额外参数）
   相距很远，且发生在 Rust 编完之后、只剩最后一步时最容易误判成 flutter 环境坏了。
   现在 `build.sh` 用 `${arr[@]+"${arr[@]}"}` 只在非空时展开；`scripts/build-apk.sh`
   也独立过滤空参数（任何入口传空都无害）。
5. **`scripts/build-apk.sh` 的桥接 hash 预检读不了负数**（`[0-9]+` 不匹配
   `-854730017`），表现为「读不到桥接 hash，先跑 regen.sh」——这个预检在任何负
   hash 的仓库里从来没放行过。已在 `a392968` 修好（bash + PowerShell 两侧）。

## 相关

- 构建本身由仓库的 `scripts/build-apk.sh` 负责（桥接 hash 预检 → Rust arm64 →
  `flutter build apk`），本 skill 不重复实现，只透传参数并记录产物身份。
- relay 侧上线用另一个 skill：`skills/relay-deploy/`。
- 排障与手工步骤：`references/runbook.md`。
