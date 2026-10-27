# android-app-update runbook

排障与手工步骤。脚本能自己判断成败，这一份是给它判断不了的情况准备的。

## 0. 前置条件

| 需要 | 检查 |
|---|---|
| `adb` 在 PATH | `adb version` |
| 设备可见且授权 | `adb devices` —— 状态必须是 `device`，`unauthorized` 要在手机上点「允许调试」 |
| Android SDK 里有 `build-tools`（提供 `apksigner`/`aapt2`） | `ls ~/Android/Sdk/build-tools/` |
| FVM 与固定 Flutter 版本 | `fvm flutter --version`（根目录 `CLAUDE.md` 有版本号） |
| 桥接一致 | `ui/lib/bridge/generated.dart/frb_generated.dart` 的 `rustContentHash` == `src/frb_generated.rs` 的 `FLUTTER_RUST_BRIDGE_CODEGEN_CONTENT_HASH` |

改了 Rust 公开 API 而没跑 `scripts/regen.sh` 时，`scripts/build-apk.sh` 会在第一步拦下
（比装到手机上再在 `RustLib.init()` 报 hash mismatch 好得多）。

## 1. 无线调试（本机常用）

```bash
adb pair 192.168.101.109:<pair-port>      # 手机上「无线调试 → 使用配对码配对」
adb connect 192.168.101.109:<debug-port>
adb devices
```

序列号形如 `adb-<hash>-<name>._adb-tls-connect._tcp` 或 `192.168.101.109:39877`，
把 `adb devices` 里显示的那串**原样**填进 `config.env` 的 `APK_DEVICE`
（不要加引号，也不要自己拼 IP）。

`connect` 掉线时先 `adb disconnect` 再重连；设备在列表里但状态是 `offline` 时，
手机端关掉再打开无线调试比反复 `connect` 快。

## 2. 签名

当前用 Android debug 证书：

```
Signer #1 certificate DN: C=US, O=Android, CN=Android Debug
SHA-256: 00706741dc04dc8015a95d320112d7f95ba5dc008bb9631443a96fc0cab5b294
```

（`ui/android/app/build.gradle.kts` 没配 `signingConfigs`，release 回落到 debug key。）

- **同一张证书**：`adb install -r` 覆盖成功，应用数据保留。
- **不同证书**：`INSTALL_FAILED_UPDATE_INCOMPATIBLE`。只能先
  `adb uninstall <pkg>`（**应用数据全丢**）再装。这属于破坏性操作，脚本不代劳，
  只在 `install.sh` 里提前把两侧指纹都比给你看并中止。
- 想换正式签名：在 `ui/android/app/build.gradle.kts` 加 `signingConfigs`，
  密钥与口令放本地（不进仓库），然后把新指纹写进 `config.env` 的
  `APK_EXPECTED_CERT_SHA256`（多个用空格分隔）。

## 3. 复核失败时

`install.sh` 最后会从设备把 `base.apk` 拉回来重新比 md5。对不上时按顺序查：

1. **是不是同一个设备**：`adb devices -l`；多设备时 `APK_DEVICE` 有没有设对。
2. **安装有没有真的落盘**：`adb shell pm path <pkg>` 的路径里那段随机后缀会变；
   重新 `pull` 一次自己算 md5。
3. **是否有克隆/分身应用**：国产 ROM 的「应用分身」会另外放一份 APK，签名可能相同，
   包名也可能是 `<pkg>.clone`。`adb shell pm list packages | grep elsewhen`。
4. **装完又被换回去**：某些系统的「安装后恢复」或第三方管家会把旧版本还原。
   关掉它，或 `adb install -r --force-queryable`。
5. **构建产物本身在变**：确认 `ui/build/app/outputs/flutter-apk/app-release.apk`
   的 mtime 是你这次构建的时间（`build.sh` 会打印）。

## 4. 首次启动迁移失败

症状：`--relaunch` 后 `pidof` 为空，或界面卡在启动。

```bash
adb -s <serial> logcat -d | grep -iE 'elsewhen|migration|panic|sqlite' | tail -60
```

- 迁移**只追加、只前进**：`user_version` 不会回退（`rusqlite_migration` 的
  `to_latest` 对任何 `user_version >= 1` 的库跳过 baseline）。手机上装回旧版 APK
  不会把库降回去，而是会因为 schema 比代码新而出现各种怪现象 —— **不要用退版本来
  「修」升级问题**。
- 失败后 app 会带着旧库继续存在：想恢复原状只能**整包数据清除**
  （`adb shell pm clear <pkg>`，**应用数据全丢**，破坏性，人工确认后再执行）。
- 真要备份，用应用内的导出功能。设备未 root、app 非 debuggable 时 adb 拿不到私有目录：

  ```bash
  adb shell run-as dev.elsewhen.elsewhen_ui ls -l files/     # → run-as: package not debuggable
  adb root                                                   # → adbd cannot run as root in production builds
  ```

  这就是本 skill 不做自动备份的原因 —— 不是没写，是权限上做不到。

## 5. 手工步骤（脚本坏了时的最小操作集）

```bash
# 1) 诊断：设备上装的是哪版
adb -s <serial> shell dumpsys package dev.elsewhen.elsewhen_ui | grep -E 'versionName|lastUpdateTime'
P=$(adb -s <serial> shell pm path dev.elsewhen.elsewhen_ui | sed 's/package://' | tr -d '\r')
adb -s <serial> pull "$P" /tmp/installed.apk
unzip -p /tmp/installed.apk lib/arm64-v8a/libelsewhen.so > /tmp/installed.so
strings /tmp/installed.so | grep -c 63-relay-v3-review-f5   # 0 = P4 之前的版本

# 2) 构建
./scripts/build-apk.sh

# 3) 比对签名
<build-tools>/apksigner verify --print-certs ui/build/app/outputs/flutter-apk/app-release.apk

# 4) 安装 + 冷启动
adb -s <serial> install -r ui/build/app/outputs/flutter-apk/app-release.apk
adb -s <serial> shell monkey -p dev.elsewhen.elsewhen_ui -c android.intent.category.LAUNCHER 1
```

## 6. 与桌面端的关系

- 桌面端（`./scripts/elsewhen.sh`）加载 `target/release/`，与手机的 APK **各自携带**
  一份 Rust 内核。两端内核版本不一致时同步未必立刻报错，但协议/迁移语义可能已经分叉，
  所以**改完 Rust 要两边都更新**：手机走本 skill，relay 走 `skills/relay-deploy`。
- 桌面端启动只要求 `target/release/libelsewhen.so` 与桥接 hash 一致；手机端还额外
  取决于 build 时的 `scripts/build-android-rust.sh` 产物。
