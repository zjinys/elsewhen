# 合规审计记录 —— 六项法律风险（2026-10-25）

针对「注册年龄验证 / 自托管字体 / 会话重放 / 营销邮件退订 / 续订条款 / DMCA
代理登记」做了全库实证审计。**结论：六项中五项因功能不存在而完全不适用，
DMCA 一项给出登记指南与政策模板**。本页留下审计证据与未来触发条件，
防止新增功能时无察觉地击穿红线。

## 总览

| # | 风险项 | 结论 | 证据 |
|---|---|---|---|
| 1 | 注册年龄验证 | **不适用**：无注册功能 | 首次启动即本地初始化（本机密码锁 + 本地生成的密钥）；同步走设备配对（owner/joiner），无账号体系。`ui/lib` 全目录无 signup / email / age 相关表单 |
| 2 | 第三方字体隐私风险 | **合规**：字体零远程加载 | pubspec 无 `google_fonts` 包；全库无 `fonts.googleapis.com` / `fonts.gstatic.com` / CDN `@import url()`；品牌资产全部本地 SVG/PNG |
| 3 | 会话重放 / 遥测上传 | **不存在** | 无 sentry / firebase / crashlytics / amplitude 等任何采集 SDK；诊断唯一出口是本地 stderr（`ui/lib/utils/error_report.dart`），受 `ELSEWHEN_DEBUG` 环境变量门控，不开 debug 不留痕、不上传 |
| 4 | 营销邮件（CAN-SPAM 退订链接 + 邮政地址） | **不适用**：应用不发邮件 | relay 是端到端加密盲信箱，无 SMTP / nodemailer / sendmail 依赖，`relay/standalone/server.mjs` 只做密文转发 |
| 5 | 订阅续订条款披露（ROSCA / 商店政策） | **不适用**：无任何计费 | 无 `in_app_purchase` / RevenueCat / platform billing 依赖；代码中无订阅、支付、续订路径 |
| 6 | DMCA 指定代理登记 | **建议登记**（成本 $6，详见下文） | relay 托管用户密文快照；因运营者无法解读密文，最接近 §512(a) mere conduit，但登记 designated agent 是廉价保险，尤其未来 relay 商用化时 |

## DMCA 指定代理：登记操作指南

适用前提：只要**部署并在对外提供服务** relay（或任何托管用户内容的服务），
就建议登记。个人自部署给家人用可以不做；一旦公开可达、有第三方用户，就做。

1. 打开 **https://dmca.copyright.gov**（2016 年起仅接受线上登记，无纸质通道）。
2. 点 **Create a new account** 建立登记账号（用长期可用的邮箱——
   续期提醒邮件发到这里）。
3. 登录后 **Add Service Provider**：
   - 填写运营主体的**法定全称**与**真实街道地址**（P.O. Box 不被接受，
     无此前批准除外）；
   - 电话与邮箱此部分仅版权局可见，不公开。
4. **Alternate Names**：填公众可能搜到你的全部名字——域名、应用名
   （如 `elsewhen`）、品牌名。备用名不再额外收费。
5. **Designated Agent**：填代收投诉的联系人（可以是你自己）：姓名、
   地址（此处可用 PO Box）、电话、邮箱。
6. 预览核对 → 勾选承诺声明 → **Submit and Proceed to Payment**：
   经 Pay.gov 付款（信用卡几分钟到账，ACH 需数个工作日）。**现价 $6**。
7. 付款成功即生效，信息自动进入公开目录，收到确认邮件。

**必须守住的义务：**

- **每 36 个月续期**：原样 resubmit 或修改后 amend，每次都要重新走认证 + $6
  付款；逾期不续，登记失效、safe harbor 保护随之消失（系统在 90/60/30/7
  天前发提醒邮件）。
- **信息变化立即 amend**（运营商名、地址、代理变更逐项都要）。
- **对称义务**：同样的代理联系方式必须**公开发布在自己的网站**上，
  内容与版权局登记一致——模板见
  [`dmca-policy-template.md`](dmca-policy-template.md)（含 §512(c)(3) 合格
  投诉要件、§512(g) 反通知程序、重复侵权者政策 §512(i)）。

## 触发条件（未来做什么，就触发什么条款）

| 触发器 | 应对 |
|---|---|
| 新增账号系统 / 收集 email 或手机号 | 先定属地含 Lob 由 COPPA（美国 <13）/ GDPR 第 8 条（欧盟 <16 需家长同意）；注册流程加出生年/年龄声明，未成年走受限默认态 |
| 引入 `google_fonts` 或在 `ui/web/index.html` 加远程 `<link>` | 慕尼黑 Google Fonts GDPR 判例（2022-2025 判赔多起）——一律自托管：字体文件进 `assets/fonts/` 并在 pubspec 声明 |
| 引入会话重放 / 错误上报 / 分析 SDK | 同意机制（opt-in、明确点名所收集数据、可持续撤回）＋ 输入屏蔽：密码框、密钥繁衍、聊天输入框 Masking；打通同意开关前不启动 SDK |
| 引入邮件发送 / 营销名单 | CAN-SPAM：商业邮件必须带退订链接（≤10 天生效）与运营者真实**邮政地址**；欧盟另需 GDPR/PECR opt-in；由脚本禁止依赖兜底（见下节） |
| 引入订阅 / 付费墙 | ROSCA（§43 恢复在线购物）：在**订阅按钮旁直接**展示价格、周期、自动续订事实与取消方式；Apple 3.1.2 / Google Play 订阅政策同向 |
| relay 对外商用 / 开放注册 | 按上文 DMCA 指南登记；另查 relay 自身 license（当前 package.json 未声明 license 字段，商用前必须先定） |

## 维持机制

- `scripts/legal-audit.sh`：把 #2/#3/#4/#5 的红线写成 grep 回归测试，
  已入库可手动跑，建议加进 CI。命中任何一条都表示有人在打破
  「出流量的只有显式 AI 请求 / DoH 查询 / 端加密信箱」这一隐私不变量，
  必须先在 docs/legal/ 记录决定与理由再合入。
- 依赖审计基线：`cargo metadata` / `flutter pub deps` / relay node_modules
  于 2026-10-25 全量扫过，无 copyleft 或采集型依赖；后续新增依赖时
  重跑 `scripts/legal-audit.sh` 并对照 NOTICE.md 的多分支选择纪律。
