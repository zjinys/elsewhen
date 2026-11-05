# 命名密钥库与占位符引用（一等凭据管理）

- 日期：2026-11-05
- 状态：`proposed`（架构与设计收敛，待排期实施）
- 范围：全局凭据管理机制；消费方包括 MCP servers（环境变量与 HTTP 请求头）、外网搜索（`web_search_configs`）、未来第三方工具/Skill 扩展；运行时占位符解析；安全存储与快照同步边界
- 性质：架构 + 安全重构提案（新表 + AEAD 密封扩展 + 运行时占位符替换 + 集中设置 UI）
- 关联：
  - `src/storage/provider_keys.rs`（现有 ring AEAD 密封基建，master key 体系）
  - `src/storage/sync/snapshot/records.rs:568`（MCP 定义与「凭据不离本机」不变式）
  - `docs/notes/proposed/architecture/2026-11-04-mcp-intent-keywords.md`（MCP 意图与配置）
  - `src/storage/web_search.rs`（`web_search_configs.api_key` 明文现状）

---

## 1. 需求与核心洞察

### 1.1 现实痛点
用户配置了多个 MCP 服务器（或未来更多集成工具），常常出现：
1. **一个 Key 供多台 Server 共同使用**（如小红书、GitHub、企业内部服务的 PAT/Token）。
2. **Key 具有周期性轮换或失效变更的需求**：一旦变更，需要手动打开每台 MCP 服务器的编辑面板逐个修改粘贴，操作繁琐且易遗漏。
3. **明文同步安全隐患**：MCP 的 `headers_json` 目前已被严格排除在快照外（见 `records.rs:568`），但 `env_json`（用于 stdio 模式的子进程环境变量）**目前仍会随快照同步**。若用户在环境变量中配置了 `RED_API_KEY=sk-xxxx`，该敏感明文会随快照被同步到所有配对设备，打破了「凭据不离本机」的安全红线。

### 1.2 为什么不做「批量替换」？
针对「批量修改多个 MCP 的同一个 key」，表面上有两种解法：
- **方案 B：纯 UI 批量替换**（扫描所有 server 的 env/headers，按值聚合并批量覆写字符串）。
- **方案 A：命名密钥与占位符引用**（引入一等「密钥」概念，集中定义，各处引用）。

**结论**：方案 B 虽简单但无长期价值。因为：
- **密钥（Secret）本质上是一个独立的一等公民（First-class Concept）**，它拥有独立的生命周期（创建、旋转/更新、失效、脱敏显示、引用审计）。它不应只是某几个 JSON 文本字段中的偶发字符串。
- 方案 B 无法解决「多个副本重复存储」的问题，下次换 key 依然需要做批处理；更无法消除 `env_json` 明文随快照泄露的问题。
- 引入命名密钥库后，该概念不仅服务于 MCP，未来还可以自然覆盖 `web_search_configs`、外部模型 API Key、用户 Skill 脚本凭据等所有敏感输入。

---

## 2. 总体架构设计

### 2.1 概念模型
```
┌────────────────────────────────────────────────────────┐
│                   命名密钥库 (Named Secrets)             │
│  - xhs_token      => 密封密文 (AEAD encrypted)         │
│  - github_pat     => 密封密文 (AEAD encrypted)         │
│  - tavily_search  => 密封密文 (AEAD encrypted)         │
└───────────────▲────────────────────────▲───────────────┘
                │                        │
        运行时占位符解析           运行时占位符解析
                │                        │
┌───────────────┴──────────────┐ ┌───────┴───────────────────────┐
│ MCP Server A (stdio)         │ │ MCP Server B (streamable-http)│
│ env:                         │ │ headers:                      │
│ { "TOKEN":                   │ │ { "Authorization":            │
│   "{{secret:xhs_token}}" }   │ │   "Bearer {{secret:xhs_token}}"│
└──────────────────────────────┘ └───────────────────────────────┘
```

1. **定义与引用解耦**：配置中只保存占位符 `{{secret:<name>}}`，不存真实凭据。
2. **运行时解析（Late Binding）**：仅在起进程或发起网络请求的最后一刻，由运行时从密封存储中解密并完成字符串插值。
3. **快照同步天然安全**：配置中携带的只是公开的无害引用名（如 `{{secret:xhs_token}}`），即使随快照同步，真实凭据也绝不出本机；新设备收到配置后只需在本机密钥库中填入该名称对应的 Key 即可生效。

---

## 3. 详细设计

### 3.1 存储层：`named_secrets` 与密封策略

新增数据表 `named_secrets`：
```sql
CREATE TABLE named_secrets (
    id TEXT PRIMARY KEY,               -- UUID
    name TEXT NOT NULL UNIQUE,         -- 引用代号，格式 [a-zA-Z0-9_\-]+，如 xhs_token
    title TEXT NOT NULL DEFAULT '',    -- 展示名称/备注，如 "小红书开放平台 Token"
    ciphertext BLOB,                   -- 密封后的密文（AEAD）
    plaintext TEXT,                    -- 未密封安装下的明文（兼容过渡，密封后置 NULL）
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
```

#### 密封与 AAD 绑定（复用现有 `provider_keys.rs` 基建）
- 密封采用项目已有的 `ring::aead` AES-256-GCM / CHACHA20-POLY1305 基建与 master key 体系。
- **AAD 上下文绑定**：
  ```rust
  ("elsewhen.named-secret.v1", identity.dataset_id, installation, id, name)
  ```
- **快照同步不变式**：
  - `named_secrets` 表加入 `LOCAL_ONLY` 排除名单（同 `provider_key_secrets`、`provider_key_policy`）。
  - **凭据绝不离本机**。同步协议永远不导出发往其他设备。

### 3.2 引用语法与占位符规范

- 语法标准：`{{secret:<name>}}`
  - 示例：`Bearer {{secret:xhs_token}}`
  - 示例：`{"RED_KEY": "{{secret:xhs_key}}", "PORT": "8080"}`
- 约束：
  - 密钥 `name` 仅允许半角字母、数字、下划线、短横线：`^[a-zA-Z0-9_\-]+$`。
  - 支持前缀/后缀组合拼接（如 `Bearer {{secret:xxx}}`）。

### 3.3 运行时解析（Runtime Resolution）

位于 `src/ai/mcp/runtime.rs` 及相关 HTTP client 构造最深处：

```rust
pub fn resolve_secret_placeholders(store: &Store, raw: &str) -> Result<String> {
    // 匹配正则或字符扫描 {{secret:...}}
    // 1. 若无占位符，直接原样返回（无额外开销）
    // 2. 若有占位符，提取 name，调用 store.read_named_secret(name)
    // 3. 任何未找到的 name，或 master key 处于 locked 状态时：
    //    立即中断并抛出明确错误，严禁静默替换为空串或留存裸模板发起调用！
}
```

#### 失败与安全边界：
1. **阻断性原则**：如果引用了一个不存在的密钥（例如 `{{secret:deleted_token}}`），工具调用立即失败，并返回友好提示：`"MCP 服务器配置引用了未找到的密钥: deleted_token，请在设置→密钥中配置"`。
2. **日志脱敏防泄漏**：
   - 调试日志（`log::debug` / `eprintln!`）、错误堆栈、UI 状态展示、数据库日志一律使用**未解析前的原始模板**（包含 `{{secret:...}}`）。
   - 解析后的明文字符串生命周期严格局限于当前进程运行或当前 HTTP Request 实例，不进入任何长期缓存。

### 3.4 交互与 UI 设计（设置页「密钥管理」）

在设置页（集成或安全大类下）提供一等「密钥管理」面板：

1. **密钥清单**：
   - 名称（`name`）与备注（`title`）
   - 掩码显示（如 `sk-8a***9f2`），提供「点击复制」或「显示」眼睛图标
   - **反向引用审计（Reference Audit）**：
     - 卡片清晰列出：`已在 3 处使用：MCP (xhs-fetch, xhs-post), Web 搜索`
     - 让用户清楚知道修改这个 Key 会影响哪些服务
2. **创建与编辑（旋转/Rotate）**：
   - 弹窗填写代号与实际值。
   - 保存时即刻加密写入，**已引用的所有 MCP / 工具即时全部生效**。
3. **删除保护**：
   - 若某密钥当前仍被任何 MCP 服务器或配置引用，删除时弹窗明确警示：`该密钥仍被 2 台服务器引用，删除后相关服务将无法启动`。
4. **MCP 服务器编辑对话框增强**：
   - 在 `env` 与 `headers` 的 JSON 输入框旁，提供一个「插入密钥引用」便捷按钮，点击直接选择已有的命名密钥并插入 `{{secret:name}}`。

---

## 4. 演进与落地路线

### 第一阶段（P1）：核心存储与 MCP 占位符解析
- 新增 `named_secrets` 表及迁移。
- 实现 `Store::create_named_secret` / `read_named_secret` / `update_named_secret` / `list_named_secrets`（集成 AEAD 密封）。
- 在 `src/ai/mcp/runtime.rs` 实现 `resolve_secret_placeholders`，在进程启动与 HTTP 请求前插值。
- 单元测试：占位符解析、缺失报错、前缀后缀拼接、密封解锁状态测试。

### 第二阶段（P2）：设置页「密钥管理」UI 与 MCP 编辑器联动
- Flutter 设置页新增「密钥管理」面板，支持查看、添加、编辑（旋转）、删除密钥。
- 列表支持展示被 MCP 引用的反向计数。
- MCP 服务器编辑面板加入「引用密钥」快捷选择。

### 第三阶段（P3）：存量迁移与生态铺开
- 提供「存量凭据一键提取」辅助工具：检测到各 MCP 中相同或明文配置的 token，提示用户「一键提取为命名密钥」。
- 推广至 `web_search_configs` 及后续插件生态。

---

## 5. 预期收益与风险评估

| 收益项 | 评估 |
|---|---|
| **改 key 效率** | 从「N 台服务器逐个点开编辑」变为「在密钥管理处修改一次，全局即时生效」 |
| **安全合规** | 彻底根治 MCP stdio `env_json` 明文随快照同步的漏洞；快照只同步占位符名，凭据 100% 不离本机 |
| **可维护性** | 明确建立「密钥」一等概念，为后续技能脚本、更多外部服务集成提供统一的凭据管理中枢 |

| 风险项 | 应对策略 |
|---|---|
| 占位符拼写错误 | 运行时严格校验并阻断，UI 编辑器提供选择器而非纯手打 |
| 密钥误删导致服务中断 | 删除前扫描 MCP 与配置中的引用，存在引用时强警示确认 |
