# Agent Note: Android 本地与 CI 使用同一固定发布证书

Status: implemented

## Problem

Android release 原来使用默认 debug signingConfig。GitHub Actions 的临时 runner 每次创建自己的 debug keystore，导致发布 APK 与本机证书不同。手机旧版 0.7.2 无法覆盖升级，用户因此授权卸载再安装 0.8.0。继续沿用原流程会再次出现相同问题。

## Decision

保留手机当前 0.8.0 的证书 SHA-256 `00706741dc04dc8015a95d320112d7f95ba5dc008bb9631443a96fc0cab5b294`。从本机既有私钥复制出独立的 `~/.android/elsewhen-release.jks`，使用独立随机口令和 alias `elsewhen-release`；证书和私钥身份不变，不需要再次卸载。原默认 debug keystore 保留。

本地口令和路径保存在忽略提交的 `ui/android/key.properties`；密钥与配置权限均为 0600。仓库只存模板与公钥指纹 `ui/android/release-signing.sha256`，不存私钥或口令。

`ui/android/app/build.gradle.kts` 从本地文件或四项 `ANDROID_KEYSTORE_*` / `ANDROID_KEY_*` 环境变量读取 release 配置。`preReleaseBuild` 依赖签名检查，缺配置、文件缺失、alias 或口令错误、指纹变化均停止，不回退到 debug 签名。debug 构建保留独立开发签名。`androidSigningProperties` Gradle 属性可指定配置文件。

GitHub 仓库已配置四项加密 secrets：`ANDROID_KEYSTORE_BASE64`、`ANDROID_KEYSTORE_PASSWORD`、`ANDROID_KEY_ALIAS`、`ANDROID_KEY_PASSWORD`，内容经 stdin 交给 gh，不输出日志。发布 workflow 恢复同一密钥，构建后再次核验 APK 指纹，然后上传；always 步骤清理 runner 上的临时密钥。脚本 `scripts/android-signing.py` 拒绝覆盖已有密钥文件，恢复失败删除本次创建的临时文件。

## Alternatives considered

每次生成新密钥：无法保持 Android 升级所需的签名身份。

直接新建不同证书：会让刚重装的手机再次无法覆盖更新，因此保留当前证书，仅把密钥的存储、口令和 alias 独立管理。

把 keystore 或口令提交 Git：会公开私钥，不采用；CI 使用仓库加密 secrets。

只配置本机固定签名：不能解决临时 runner 的证书变化，因此同步修改发布 workflow，并核验发布产物的预期指纹。

## Consequences

本机和后续采用该 workflow 的 CI 构建保持同一签名身份，版本升级不再要求卸载或清数据。密钥需要持续保留；换机器时恢复既有密钥，不重新生成。已发布的旧 tag 不改写，旧发布 APK 的证书不会因此改变。workflow 代码仍需随仓库改动提交并推送后才会用于远端构建；本次未触发新版本发布。

证书源自 Android debug 证书，当前用途为既有分发的升级兼容；本次不宣称完成 Play 上架或签名轮换。未来迁移签名需另外处理 Android 兼容机制。

## Verification

- 本机固定签名 `assembleRelease` 成功（506 tasks），APK 公钥指纹与手机已安装证书一致；包内 Dart AOT 和 Rust 库与刚安装版本逐字节一致。
- 实际 Gradle 构建验证纯环境变量配置可用，缺配置和不同证书均失败。CI 恢复验证正确密钥成功、缺 secret/错误口令/不同证书失败，已有目标文件不被覆盖，失败不残留私钥。
- 四项 GitHub 加密 secrets 已创建并通过名称列表核验；workflow YAML 解析和签名恢复/清理步骤检查通过。私密本地配置由 git ignore 排除。
- 手机先前已按用户授权卸载旧版并安装 0.8.0+80，整包、Dart AOT、Rust 回读一致且冷启动进程存活。固定配置后的额外覆盖安装验证因手机再次断开而未执行，不据此声称该步骤通过。
