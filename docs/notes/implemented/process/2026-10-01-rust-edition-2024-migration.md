# Agent Note: Rust edition 2024 迁移

Status: implemented

## Problem

项目此前使用 Rust edition 2021。2026-10-01 的迁移评估发现库代码可编译，
但 `cargo check` 不包含测试目标，会漏掉测试中 8 处 `std::env::set_var` /
`remove_var` 在 2024 下新增的 unsafe 要求。2026-10-03 用户要求完成升级，
随后明确要求同时升级到 2024 格式风格。

工作区已有大量未提交业务改动，迁移不能用还原文件或全局自动修复覆盖它们。
语言 edition、rustfmt 风格、工具链版本和依赖升级是独立决定，不捆绑推进。

## Decision

- `Cargo.toml` 使用 `edition = "2024"`。
- `rustfmt.toml` 显式设置 `style_edition = "2024"`；全仓运行 `cargo fmt`。
  导入排序、长宏调用和链式调用的换行采用新风格，不保留 2021 风格作为过渡。
- 不修改依赖版本、不声明未经完整依赖链验证的 `rust-version = "1.85"`。
  Rust 1.85 只是 edition 2024 的起点，不是本项目的 MSRV 承诺。
- 本机已有 Rust/Cargo 1.99.0，满足迁移需要。本次不执行 `rustup self update` /
  `rustup update`，不改全局默认工具链或 target；CI 继续使用浮动 stable。
- 不重生成 FRB：公开业务 API 没有变化，只重编 release 库。

## Environment test isolation

不在并行单元测试里机械添加 `unsafe { set_var(...) }`。`test_env_guard` 只能约束
主动持锁的项目代码，不能阻止其他线程、依赖或系统库读取进程环境，因此它不是
环境变量写入的安全证明。

`src/api/mod.rs` 的 `DataDirGuard` 直接复用 `AppConfig::pin(Some(...))`，
持测试锁并在退出时 `unpin`，不再修改或恢复环境变量。

原 `src/config.rs` 的五个配置测试迁移到 `tests/config_environment.rs`：

- `Cargo.toml` 为该测试注册 `harness = false`，绕开 libtest 的测试线程。
- 父进程为每个用例创建唯一临时根目录，用 `Command::env` 设置子进程的初始
  `ELSEWHEN_DATA_DIR`，不修改父进程环境。
- 每个用例在独立、无其他线程的进程入口执行，只调用同步配置和文件系统逻辑。
  只有“钉死后环境改变无效”和“未钉死时每次重读环境”两个场景在该入口内执行
  带安全说明的 `unsafe set_var`，不再需要恢复环境。
- 子进程先验证环境指向父进程传入的临时目录；所有数据路径都位于自建根目录下，
  退出后由父进程清理。断言或子进程失败向 Cargo 返回失败，不访问个人数据库。
- 保留显式目录优先、钉死后稳定、同目录幂等、不同目录拒绝、未钉死重读的原始覆盖。
  测试入口支持常见名称过滤、`--exact`、`--skip`、`--list`；不承诺完整模拟 libtest。

业务配置解析和存储行为保持不变。

## Formatting and bridge boundaries

初始评估曾观察到 `cargo fmt --check` 的 311 个 diff 块；它不是本次实际改动文件数。
本次在格式化前后逐文件比对，`cargo fmt` 实际修改 **80 个 Rust 文件**。
这些排序、换行改动与已有业务改动共存，未提交 Git。

旧评估确认 FRB 生成的 `extern "C" fn` 函数项不是 2024 要求加 `unsafe` 的
`extern "C" { ... }` 声明块。本次全目标检查直接验证生成代码能编译。
codegen 固定使用旧 edition 格式化自身产物不决定 crate 的语言 edition；
未来重生成后应按仓库配置重新运行 `cargo fmt`，无需为此次迁移触发 codegen。

## Verification

- `cargo check --all-targets`：通过；保留 3 条既有测试警告（未使用导入和变量）。
- `cargo fmt --check`、`git diff --check`：通过。
- `cargo test --test config_environment`：五个独立配置用例全部通过。
- 首轮 `cargo test --all-targets`：451 passed / 1 failed / 10 ignored。
  `ai::provider::streaming::tests::actual_http_stream_delivers_deltas_and_usage`
  访问 `127.0.0.1` 的本地测试服务时收到 HTTP 502。当前环境设置了小写代理变量，
  未设置 `NO_PROXY` / `no_proxy`。
- 仅为测试命令补充回环代理排除后，全量重跑通过：**452 个单元测试 + 5 个配置
  集成测试通过，10 个单元测试按原设置忽略**。未改应用代理逻辑或跳过失败用例；
  API 快录测试和上述 HTTP 流测试均在重跑中通过。
- `cargo build --release`：通过，生成 Flutter 使用的 release 桥接库。
- `Cargo.lock` 与本次迁移前备份逐字节相同，依赖未改变。

```bash
env NO_PROXY=127.0.0.1,localhost,::1 no_proxy=127.0.0.1,localhost,::1 cargo test --all-targets
cargo build --release
```

## Alternatives considered

1. **`cargo fix --edition` 自动包 unsafe**：旧评估用最小样例证实工具会生成包裹，
   但能编译不代表满足环境 API 的线程安全条件；自动修复也会触及当前脏工作区。
   本次改为明确的测试隔离，不运行全局 `cargo fix`。
2. **继续 2021 edition**：兼容但不满足用户升级要求；已安装工具链支持 2024，
   迁移不需要升级依赖或改写业务代码。
3. **2024 语言 + 2021 格式**：能缩小差异，是初始实施方案；用户明确要求同时升级
   style，因此最终统一 2024。不存在必须保留旧风格的技术限制。
4. **只跑 `cargo check`**：排除，因为原始兼容错误全部在测试中；必须验证所有目标。
5. **顺带升级工具链、补 MSRV 或重生成桥接**：与本次请求无关；工具链已满足要求，
   依赖链最低版本未验证，公开 API 未变化，均不做。

## Consequences

收益是语言和自动格式风格统一到 2024，并移除并行测试中未经证明安全的环境写入；
配置语义仍由真实环境和真实文件系统测试覆盖，而不是用 mock 替代。

代价是 80 个 Rust 文件的格式差异，以及五个配置测试改用小型独立进程入口。
这些环境测试随 `cargo test --all-targets` 或 `cargo test --test config_environment`
运行，不再包含在单独的 `cargo test --lib` 中。

本次本地验证不能替代 Windows/macOS CI、Flutter UI 或真实模型质量验收。
