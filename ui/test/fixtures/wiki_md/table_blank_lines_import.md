# 英文技术文档修订 · english-doc-revision



> 把你写好的英文技术文档，按 Google 官方技术写作规范**逐条审改**，交付一份带 **Word 原生修订标记**的 .docx——每处改动都能看到「改成什么」和「依据哪条规则」。



- **技能名**：`english-doc-revision`（英文技术文档修订 / English Doc Revisor）

- **版本**：0.2.0 · 分类：效率工具

- **面向**：需要写英文 Design Doc / API 文档 / README / Release Notes 的中文母语工程师

- **规则来源**：Google Developers Technical Writing Courses（CC BY 4.0，已署名）



---



## 一、它到底解决什么问题



不是「英文不够好」，而是「写得不合技术写作规范，评审看不懂」。四个高频真实痛点：



| 痛点 | 典型症状 | 这个技能怎么处理 |

|---|---|---|

| 不知道错在哪 | 语法没错，但评审会上一路被问「这到底要做什么」 | 按 R1–R8 八类规则逐节扫描，指出**结构与信息组织**的问题，不只是句子 |

| 改了还是被打回 | `The system should be fast` 这类**无法验收**的需求 | 按 R4.4/R4.6 换成可度量指标（p99 / uptime / 成本） |

| 通用润色工具帮不上 | 只把句子改「顺」，不解决「需求不可验证」「术语混用」 | 专攻技术写作风格层，每条意见带规则编号 |

| 改完要自己回贴 | 聊天里一堆建议，得手工往 Word 里搬，格式全乱 | 直接产出**带修订标记的 .docx**，在 Word/WPS 里一键接受或拒绝 |



### 真实对照（来自 `demo/`）



原文（52 词一句话，叠了 5 个问题）：



> There are many problems with our current notification system, which was built several years ago and has been receiving complaints from multiple different teams for quite a long time now, and looking at the situation, it is clear that something needs to be done about it.



评审后：



> The legacy notification system has three problems: teams file complaints weekly, incidents take hours to diagnose, and adding a new channel requires a full release.



依据：R3.2（一句一事，约 20 词内）、R3.4（不用 there is/are）、R3.5（避免 -ing 悬垂开头）、R3.1（去被动）。



---



## 二、两种工作模式（自动分流，不用记命令）



| | 模式 A · 评审 | 模式 B · 从零写作 |

|---|---|---|

| **触发** | 你直接贴一段英文（或上传 .docx / .md） | 你说「帮我写 / 起草一个 README / 写个 Design Doc」 |

| **技能动作** | 判断文档类型 → 按规则库逐节检查（不抽样）→ 出报告 | 定类型 → 问三个必答问题（读者是谁 / 读者要做什么 / 唯一核心信息）→ 先骨架后逐节推进 |

| **产出** | 评审报告 + 可选 Word 修订稿 | 逐节英文草稿，每节附一句中文说明「这节只要读者记住什么」 |

| **收尾** | Top 3 优先修改项 + 三维评分 | 用评审规则库自查一遍，报告修了哪些问题 |



> 三个必答问题答不上来时，技能会基于上下文给假设让你确认，不会空转追问。



---



## 三、三种交付物



### 1）评审报告（聊天里直接看）



每条问题固定五要素，**先讲最重要的**（超过 15 条时正文只展开前 10 条，其余归入「次要问题清单」）：



```

🔴 R4.4 需求语气模糊：「should be fast」

- 原文：> The system should be fast...

- 问题：fast / reliable / scalable / efficient 全部不可验证，且一句塞四个需求

- 依据：R4.4（must/should/may 全文一致）、R4.6（状态词→可验证标准）

- 改写：Latency p99 < 30s / Availability 99.9% / Cost < $X per million

```



结尾固定给 **Top 3 优先修改项** + **三维评分**（结构 / 语言 / 完整度，各 5 分制）。



### 2）Word 修订稿（核心卖点）



不是「把改好的全文发给你」，而是**在原文上标修订**：



- 删除有删除线、新增有下划线，右侧批注写「为什么改 + 规则编号」

- 在 Word / WPS 里逐条**接受或拒绝**，改动全程可追溯

- 内置**完整性闸门**：所有「保留 + 删除」片段按序拼接必须与你的原文逐字一致，否则拒绝生成（`INTEGRITY FAIL`）。技能弄丢或篡改你的原文，是过不了这道闸的



两条管线，按原稿格式选：



| 管线 | 脚本 | 输入 | 输出 | 特点 |

|---|---|---|---|---|

| **新建修订稿** | `scripts/build_review_docx.py` | .md / .txt（Markdown 结构） | 全新 .docx | **零依赖**（纯标准库），适合粘贴文本 |

| **原地注入修订** | `scripts/apply_track_changes.py` | 你自己的 .docx | `<原名>-reviewed.docx` | **保留原样式/模板**，只在原 run 上切分注入 `w:ins` / `w:del` / 批注 |



配套 `scripts/extract_docx.py` 把 .docx 抽成带段落编号的清单，让技能能精确定位「改第几段」，两脚本共用同一套定位逻辑，编号可直接落地。



### 3）从零草稿（模式 B）



按 `references/templates.md` 的五套骨架逐节产出：Design Doc / API Reference / Release Notes / README / ADR。



---



## 四、规则库从哪来



蒸馏自 Google Developers Technical Writing Courses（TW1 写作基础 + TW2 进阶技巧），**翻译、提炼、重组**为可执行检查项，每条带编号，可溯源到课程章节。



### 评审规则 `references/rubric-review.md`（8 类 40+ 条）



| 编号 | 覆盖层 | 检查什么 |

|---|---|---|

| R1 | 结构与组织 | 标题信息化、章节顺序、元数据 |

| R2 | 段落层 | 一段一主题、首句承载信息、段落长度 |

| R3 | 句子层 | 主动语态、一句一事、this/that 带名词、去 there is/are、避免悬垂 |

| R4 | 词语层 | 术语一致、缩写首次展开、must/should/may 一致、状态词→可验证标准、称 you 不称 the user |

| R5 | 列表与表格 | 平行结构、列表引导句、表格可比性 |

| R6 | 代码与命令 | 可复制、占位符标注、解释性注释 |

| R7 | 插图与无障碍 | alt text、图注、对比度、包容性语言 |

| R8 | 文档类型专属 | Design Doc / API / Tutorial / README / RFC·ADR / Release Notes 各自的骨架检查 |



### 写作规则 `references/rubric-writing.md`（W1–W5）



W1 文档类型决策 → W2 三个必答问题 → W3 逐节推进 → W4 语言产出自检 → W5 收尾自查。



### 模板 `references/templates.md`



5 套英文骨架（含中文段落注释）：Design Doc、API Reference、Release Notes、README、ADR。



---



## 五、支持的文档类型



| 范围 | 类型 | 处理方式 |

|---|---|---|

| **核心支持** | Design Doc / RFC / ADR、API Reference、README、Release Notes / Changelog、Tutorial | R1–R8 全量检查，含类型专属骨架检查 |

| **通用支持** | Postmortem、PR 描述、运维手册、代码注释块 | 按 R1–R7 通用技术写作规则评审 |

| **明确不支持** | 英文简历、商务邮件、营销文案、求职信 | 会先说明超出范围；你坚持的话只从基础可读性（主动语态/句长/拼写语法）给有限建议 |



---



## 六、三步上手



1. **要评审**：把英文文档直接粘进来（或上传 .docx / .md）——不用说明来意，技能会自动判定文档类型并开始逐节评审。

2. **要 Word 修订稿**：评审后说「给我 Word 修订版」，或一开始就说「改完导成带修订的 docx」。

3. **要写新的**：说「帮我写个开源 README」，会先问三个必答问题，再逐节带你写完。



---



## 七、边界与诚实说明



- **不替代语法/拼写检查器**（Grammarly、LanguageTool）：本技能修的是技术写作风格层，不是把每个拼写错误挑出来。

- **不评业务与架构对错**：它管「写得是否可评审、可验证、可执行」，不管方案本身是否合理。

- **不做翻译**：产出正文一律英文；中英混排草稿会先转成英文再评审，不会把你的英文文档翻成中文。

- **不编造技术事实**：写作模式缺关键信息时宁可追问，不会替你填数字、指标或组件名。

- **输入形态**：文本 / Markdown / .docx。旧版 `.doc` 二进制需先在 Word 里另存为 `.docx`。

- **段落含超链接**：只支持整段批注，不做段内文字替换（避免破坏链接）。

- **代码块**：按原样保留、不参与修订标记，代码里的问题用批注提出。

- **规则来源**：Google 公开课程内容的编辑版本，非 Google 官方产品，也不代表 Google 背书。



---



## 八、目录结构



```

skill-en-doc-revision/

├── SKILL.md                        # 技能定义（YAML front matter + 执行指令）

├── INTRO.md                        # 本文件：对外介绍

├── README.md                       # 工程说明（MVP 结构、上架待办）

├── MARKETING_COPY.md               # 发布文案与卖点（含竞品对照）

├── PUBLISH_CHECKLIST.md            # 上架检查清单

├── references/

│   ├── rubric-review.md            # 评审规则库 R1–R8（40+ 条，带溯源）

│   ├── rubric-writing.md           # 写作规则库 W1–W5

│   ├── templates.md                # 5 套英文模板 + 中文注释

│   └── review-protocol.md          # 评审标记 JSON 协议 v1（与渲染器的契约）

├── scripts/

│   ├── build_review_docx.py        # 文本/Markdown → 修订 docx（零依赖）

│   ├── extract_docx.py             # .docx → 带编号的段落清单

│   └── apply_track_changes.py      # 在用户原始 .docx 上原地注入修订

├── demo/

│   ├── bad-design-doc.md           # 种入 15+ 缺陷的烂 Design Doc（演示输入）

│   ├── review-output.md            # 完整评审报告示例（聊天形态）

│   ├── review-markup.json          # 结构化标记（渲染器输入）

│   └── bad-design-doc-reviewed.docx # 最终交付物：Word 修订模式版

└── test-samples/                   # 三份 bad + 一份 good + 对应修订稿 + 测试方案

```



---



## 九、来源与授权



评审规则蒸馏自 Google Developers, Technical Writing Courses（<[https://developers.google.com/tech-writing>），依据](https://developers.google.com/tech-writing>），依据) CC BY 4.0（<https://creativecommons.org/licenses/by/4.0/>）使用，经翻译、提炼与重组；规则内容为原课程观点的编辑版本，非原文照搬。



---



**一句话总结**：市面上有「帮你润色文字」的，也有「从零生成 docx」的；这个技能做的是第三件事——**按技术写作规范审你已有的英文草稿，并把改法以 Word 修订形态交回你手上。**
