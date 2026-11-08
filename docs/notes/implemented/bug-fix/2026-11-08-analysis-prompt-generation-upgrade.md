# Agent Note: 已执行版本74数据库缺少分析任务代数

Status: implemented

## Problem

用户报告分析worker在claim阶段重复报`no such column: prompt_generation`。当前迁移74源码包含该列，但较早执行的74数据库可能只有`prompt_bundle_id`而缺少后加的代数；修改已经执行过的迁移不会重放，版本号相同不能证明结构相同。直接新建库回归无法覆盖这种漂移。

## Decision

新增迁移75作为已修复结构的版本边界。前台初始化发现版本74时，在IMMEDIATE事务内检查`analysis_jobs`列信息，缺少`prompt_generation`才添加非负整数列并默认0；随后迁移到75。当前74已经有列时保留原值。历史迁移不回写，不重建表、不重排任务、不重置已有bundle/尝试次数。后台版本检查发现74低于75则跳过，不能在worker里执行迁移；新版应用重启的前台初始化负责修复。

构建脚本显式跟踪`migrations`和`resources`目录，包含目录中的新增文件会使嵌入迁移/资源重新编译，避免只改SQL后复用旧二进制。源码修复不会热修改已经运行的进程，需启动新构建。

## Alternatives considered

- 再编辑迁移74：对已经执行过74的库无效，继续造成新库和老库结构不同。
- worker遇错时补列：违反启动迁移边界，多worker争抢结构锁，难以判断当前运行的版本。
- 删除重建analysis_jobs或吞掉查询错误：会丢失任务/运行身份，无法保证重新分析时固定新bundle。

## Verification

- 当前74升级保留已有`prompt_generation=9`；缺列74升级保留原文、retry状态、3次尝试和诊断，默认代数0，重复初始化保留后设的代数7。
- 文件库回归复现缺列74：后台拒绝迁移；前台打开升级后成功claim，失败后显式重新分析使代数变为1，获得不同bundle，后台可重新打开。临时库不读取个人数据。
- 迁移专项29项通过，新增worker升级2项通过；随后Rust all-targets **1141 passed / 20 ignored**（库1139+集成2，`/tmp/digest-analysis-final-all.log`）。release及知识/语言/Prompt Bridge回归见同日[预算修复记录](2026-11-08-digest-context-budget.md)。

## Consequences

升级可以修复已落盘74的已知单列漂移，保留分析任务历史与重跑代数。仅修复已知历史问题；其他损坏结构仍明确报错，不猜测重建个人数据。新版本前台启动前后台会等待，代价是升级期间暂不推进分析。
