# Agent Note: 用户要求的目标重置与 Provider 密钥迁移

Status: implemented

## Problem

用户要求清空当前目标、计划与目标聊天，并将 AI Provider 的 API Key 去重迁入已建立的密钥管理。当前 Provider 运行读面尚未解析命名引用，直接修改配置会造成认证失败。

## Decision

Provider 运行读面（默认渠道与failover）在读出凭据模板后解析命名密钥；编辑/存储读面保留模板，避免重新复制真实值。密钥反向引用审计纳入 Provider，引用中的密钥受删除保护。

数据操作前使用 SQLite backup 保存权限0600的完整备份。系统密钥环仅在本机内存读取，不打印密钥；解开11个Provider密文，按完全相同的值去重为8项命名密钥。新增8项均用原master和命名密钥AAD密封，Provider原密封槽改为引用模板，api_key_source保留命名引用以支持锁定状态的引用审计。逐项解密回读引用并比较原始值，11项全部等价，明文列均为NULL。原有1项命名密钥保留，总数9。

目标重置使用单事务，限定goal_id/goal_mentor/goal-studio会话，清理相关反馈与生成缓存后由既有会话删除级联清理历史；目标/计划发送删除记录，并删除旧目标资料元数据。个人profile/me与其他聊天保留。目标协议既有deleted字段允许显式重置使用，普通完成仍走状态迁移；旧版本配对端需要更新后才能接受目标删除记录，未在本次操作中声称真机同步已签收。

## Alternatives considered

不直接把原Key替换为未接入解析器的模板；不以密文相等判断Key相等（随机nonce会不同）；不清空整个对话表。使用SQLite一致备份，不复制可能遗漏WAL的裸文件。

## Consequences

2026-11-06 当前库实际结果：5目标、1计划、5目标会话删除，剩余均为0；11个Provider引用8个新增密钥，命名密钥总计9，Provider与命名密钥明文列均为0，外键检查通过。备份：`/home/pp/.local/share/elsewhen/elsewhen.db.backup-goals-and-provider-secrets-20261106-093954`。备份和既有同步日志保留恢复/复制所需历史，不宣称安全擦除磁盘历史。

验证：生产副本演练清理通过；目标清理与Provider引用/轮换/删除保护2项测试通过；生产迁移11项原Key等价验证、剩余计数及外键检查通过。任务日志仅含数量，不包含凭据。
