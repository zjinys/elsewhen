# Agent Note: Product Hunt 产品的 Affiliate 证据采集

Status: implemented

## Problem

用户需要判断发现的产品是否可通过推广赚取佣金。产品名称、热度与 Affiliate marketing 分类不足以证明该产品开放推广计划；同名产品、仅赠送使用额度的邀请奖励、帮助商家管理联盟的软件也不能当作返佣项目。

2026-11-20 实查 Product Hunt RSS 50 条，字段只有 id/published/updated/link/title/content/author，没有 Affiliate 字段。[公开 Post 文档](https://api-v2-docs.producthunt.com/object/post/)与 [ProductLink 文档](https://api-v2-docs.producthunt.com/object/productlink/)也未列出统一状态字段。普通 HTTP 请求 KaDeep AI 的 PH 产品页返回 challenge 页面，不能把访问失败记成无计划。

## Decision

Product Hunt 负责发现产品；使用其链接关联到产品官网，再从官网主动公布的 Affiliate / Referral / Partner 入口取得有界证据。优先发现实际链接，不穷举 URL，不绕过 challenge。产品域与计划主体必须匹配；第三方联盟平台必须由官网链接背书。

状态为 confirmed（有明确公开计划）、unavailable（官网明确否定或宣布关闭）、unknown（未查到、访问失败、只有含糊字样）。区分现金佣金、使用额度邀请奖励与经销/代理合作。现有条款过期或复查失败时保留先前证据与检查状态，不刷新为已确认。

每条证据记录 product_id、official_website、program_url、status、reward_type、commission_text、payout_text、eligibility_text、evidence_quote、source_url、checked_at、check_outcome；只填原文能支持的字段。申请行为由用户在原站完成，不自动报名或取得个人追踪链接。

卡片只显示新鲜且已确认的现金推广标记，点击详情查看计划链接、原文和核查时间。未确认时不显示“无 Affiliate”；仅作为 PH 产品的补充属性，不设置全局查询条件。新增补充证据与已有成功采集基线分离，不让官网暂不可用造成榜单丢条或虚假 gone。请求上限与复查周期如下方实现记录。

## Verified examples

- [TravelAnimator 官网](https://www.travelanimator.com/hub/how-to-become-a-travelanimator-affiliate)：明确开放推广计划，最高 10% 订阅佣金，有申请入口；PH 发布者也在论坛说明，但卡片以官网证据为准。
- [Tally 当前推荐条款](https://tally.so/help/referral-program)：20% 订阅费用，单个被推荐用户上限 $150。历史博客曾表述持续返佣，不能忽略当前上限。
- KaDeep AI：当前检查未取得明确 Affiliate 计划证据，只能标 unknown，不能标 unavailable。

## Alternatives considered

- 从 PH 的 Affiliate marketing 标签或简介关键词直接确定开放计划：会混淆产品类别、其他用户评论与产品自己的推广政策。
- 将未查到、403、超时或 JS 页面当作没有：把采集覆盖缺口伪装成商业事实。
- 只保存佣金百分比：遗漏有效期、分成周期、上限与申请要求，易误导决策；先保留可追溯条款原文。
- 在每次榜单采集时强制遍历所有官网：放大请求成本且让补充信息故障阻断主采集；采用独立有界补充流程。

## Validation scope

Workers 70 项通过、4 项 opt-in 跳过；Flutter 产品卡/来源筛选/研究流程 18 项通过，TypeScript 与相关 Flutter 静态检查、Worker dry-run 均通过。新增 D1 隔离测试验证补充采集不会修改产品观察，复查失败保留旧条款和首次核查时间，stale 不显示肯定标记。卡片预览中 Affiliate 产品明确标为合成示例，不代表 KaDeep AI 有推广计划。

覆盖公开返佣、仅赠额度、代理合作、明确关闭、同名异主体、无结果、challenge/超时、过期条款与第三方注册链接背书样例。每个 confirmed 标记必须能打开对应产品的官方证据；来源不可用不能把旧肯定结果刷新为新肯定或无计划，不影响榜单主采集。中文/英文卡片与详情保留核查时间和原文。

## Risks

网站结构、跳转、地区限制和条款可变化，覆盖不保证完整。自动规则可能把营销平台的功能说明误当作自身计划，必须校验主体和具体报名/条款证据。佣金页面不证明实际成交或收益，申请资格仍由平台决定。D1 0006 已应用，Worker 已于 2026-11-20 发布；重新采集 PH 外链后逐批检查。


## Implementation

`scout-cf/src/affiliate.ts` 每次 cron 最多处理两产品，独立 D1 0006 表保存证据与复查状态，lease 防止重复提交。每产品最多六次 HTTPS 请求（含跳转）、8 秒单请求、512 KiB 单页，只跟随官网公开的最多两个计划入口。七天复查成功记录，一天重试未确认项；失败保留原证据并标 stale，卡片隐藏过期或失败复查的肯定标记。

PH Atom v4 保留官方 Link 跳转地址，新字段不改变历史产品身份。榜单/详情批量附加 affiliate 对象，Affiliate 不改变查询维度和分页，2026-11-20 按用户反馈移除全局“可推广返佣”条件。原始观察/主采集哈希不包含补充证据。卡片来源改为标题同行图标，tooltip 保留来源全称；详情保留条款摘录、证据 URL 和核查时间。当前规则要求计划标题和明确邀请/佣金陈述，复杂条款与仅 JS 页面可能保持 unknown；佣金摘要不自行提取百分比，防止丢失上限。

## Consequences

可通过官网公开条款核实推广机会，并保留失败记录与原证据。请求预算与保守规则使覆盖逐步完成；缺 PH 官网跳转的历史条目要等再次采集。补充接口与迁移随 Scout 发布，客户端收到扩展 JSON 即可展示，无 FRB 签名变化。
