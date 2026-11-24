# Agent Note: 后台摘要失败冷却与独立记忆运行库

Status: implemented

## Problem

Linux Flutter 调试默认加载 target/release/libelsewhen.so。仅运行 Flutter build 不会重编 Rust，旧库会继续按知识页读取新独立记忆，报告“记忆正文不存在”。摘要请求收到非 JSON 后后台预热下一拍立即重试，重复请求和错误日志掩盖了另一个独立故障。

## Decision

重编实际加载的 release 库。独立记忆自审读取 content，旧缺页条目延期跳过，不中断后台任务。摘要仅接受完整结构化返回或包裹完整 JSON 的单一代码块；空响应及其他格式保留失败，不伪造摘要。格式错误诊断只记录字符数、首字符码位及结束原因，不输出模型返回正文。

后台摘要预热按会话、消息边界、渠道 basis、模型、预算及提示词依赖生成输入键。失败后同输入冷却 15 分钟；更换配置或提示词可以重新尝试。有效摘要节点仍保留供后续复用。

## Alternatives considered

没有通过隐藏全部日志解决故障，也不把普通文字或截断 JSON 当作有效摘要。没有让缺页旧记忆触发全局 worker 退避。Flutter Hot Reload 无法替换进程中已经加载的原生动态库，需重启应用。

## Consequences

后台无效响应的重复消耗降低；冷却期内同输入不重试，恢复最多等待 15 分钟。迁移与正文优先路径必须在真实 release 动态库中生效，不能只以 cargo test 和 Flutter 编译作为运行库更新证据。

## Validation

摘要模块 19 项通过、2 项已有实验忽略；覆盖完整代码块、普通文本/空响应拒绝、失败冷却、配置变化及冷却到期重试。记忆自审 16 项通过，包含无知识页独立正文和缺页旧记录延期。`cargo build --release --lib` 完成，实际加载路径 target/release/libelsewhen.so 的文件时间更新为本轮构建时间。
