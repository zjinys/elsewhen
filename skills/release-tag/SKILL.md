---
name: release-tag
description: 给 Elsewhen 仓库发版打 tag。自动更新版本信息（Cargo.toml/Cargo.lock/ui/pubspec.yaml/PKGBUILD）、提交版本号、fast-forward 合并 main 并推送、打 annotated tag 并推送触发 CI 构建产物。当用户说"打tag/打标签/发版/升版本到X.Y/出个新版本/release"时使用。
---

# release-tag 发版

> 发版一条龙：**更新版本文件 → 提交 → 合并 main → 推送 → 打 tag → 触发 CI**。
> 自包含，脚本在 `scripts/` 目录，零安装直接运行。

## 怎么调用

```bash
bash <仓库根>/skills/release-tag/scripts/release-tag <version>            # 完整发版并推送
bash <仓库根>/skills/release-tag/scripts/release-tag <version> --no-push  # 只本地提交与打 tag
bash <仓库根>/skills/release-tag/scripts/release-tag <version> --message "自定义 tag 说明"
```

必须在目标 git 仓库内（或其子目录）运行；版本号必须是 `X.Y.Z` 数字形式。

## 它会做什么

1. **校验**：tag `v<version>` 本地与远端都不存在；main 是当前分支祖先（否则报错，提示先合并 main）。
2. **更新版本文件**（存在才改，只把这些文件单独提交，不碰工作区其它进行中改动）：
   - `Cargo.toml` → `version = "X.Y.Z"`，`Cargo.lock` 里 elsewhen 包同步；
   - `ui/pubspec.yaml` → `version: X.Y.Z+<build>`，**build 号 = 次版本×10 + 补丁号**（0.5.1→51、0.7.0→70 的既有惯例）；
   - `packaging/linux/PKGBUILD` → `pkgver=X.Y.Z`（本地 makepkg 兜底值；CI 发版时 release.yml 会从 tag 注入，以 tag 为准）。
3. **提交** `chore(release): 版本升至 X.Y.Z`。
4. **合并**：main `--ff-only` 到该提交（非 main 分支先 checkout main，完成后回到原分支）。
5. **推送** main、原分支（尽力而为）、annotated tag。tag 附注首行默认 `vX.Y.Z 发布`，自动附上次 tag 以来的 `git log --oneline` 提交清单。
6. 打印 CI 进度链接（GitHub Actions，release.yml 按 `v*` tag 构建全平台产物并发布 Release）。

## 注意事项

- **tag 一旦推送即触发 CI 发布**：想先本地确认用 `--no-push`，之后手动 `git push origin main "vX.Y.Z"`。
- 仓库 release.yml 带 tag/pubspec 一致性守卫：本脚本同步 pubspec 后守卫必然通过；手工改版本号漏掉文件正是守卫要防的事故（v0.7.0 曾因 PKGBUILD 硬编码 pkgver 发出 0.5.1 的 pacman 包）。
- 脚本不处理「main 领先当前分支」的情形：先用日常流程把 main 合进工作分支，再打版。
- 删除已推送的 tag/Release 属破坏性操作，不在本脚本职责内，需要时用 `gh release delete` + `git push origin :refs/tags/<tag>` 并明确告知用户后果。
