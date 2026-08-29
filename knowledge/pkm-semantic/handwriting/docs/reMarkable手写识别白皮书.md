# reMarkable 手写识别白皮书（块⑥）

> **块⑥「手写识别」= 笔迹 → 文字/结构的通用识别引擎。** 与五大分块都不同：不改设备行为、只读设备文件 + host/设备侧调云识别。首个能力是「手写批注 → 文字」（cardhw + freeform export），未来扩到「手写结构 → 有序/无序列表、待办」（数字/实心圆=列表、方框=待办）。
>
> ⚠️ **跨块关系**：本块代码骑在 `pkm/` daemon 上（端化 cardhw 复用 `cardsync`/`notebook_sync`/`notebook_rm`/`fswatch`），概念独立成块——同「划词查字典」（块4代码在 pkm）的处理。与 PKM 的 ★待办（手绘星→语义）**同源但不同块**：★待办深绑总结卡片、留块5；块⑥是**通用识别引擎**，★待办算它的近亲不算成员。
>
> 优先级/分块地图见顶层《[功能路线图白皮书](../../../../docs/reMarkable功能路线图白皮书.md)》（路线图 P1/P2 = 本块）；反解基础设施与 PKM 共用 `device-core`，架构见《[设备端 Rust 架构](../../../../docs/reMarkable设备端Rust架构.md)》。

---

## 01｜概述与定位

**做什么**：设备上的手写中文笔记 → 识别成文字/结构，让手写可检索、可引用、进 PKM。**定位**：不追求设备上实时识别（那需设备端算力 + 深度 hook，风险高），追求「写完之后笔记不死在设备里」——是 Supernote「实时识别」卖点的**流转版**替代。

**两个已落地成员** + 一个方向：

| 成员 | 形态 | 状态 |
| --- | --- | --- |
| **cardhw · 卡片手写批注注入** | 卡片某槽旁手写想法 → vision 空间关联 → 内联注入书摘行 | host 版真机通；**端化 A1/A2/B/C + 端到端真机通（DeepSeek）** |
| **export · freeform 手写 → Markdown** | 整页手写 → 待校对 Markdown vault | host 版真机通 |
| 结构识别（未来） | `.rm` 笔迹结构 → 有序/无序列表、待办清单 | 规划中 |

**核心定位（de-risk 定案）**：**辅助转写 + 人工校对**，不是无人值守 OCR——工整字近满分、快写连笔约六成，须留校对兜底。

---

## 02｜识别质量 de-risk（2026-08-29，真机本人笔迹）

立项前先做小样本识别质量实测。**方法**：设备手写 → 退回书库触发落盘 → host 拉 `.rm` + xochitl 原生缩略图 → 派**不知道 ground truth 的独立视觉 agent 盲测转写**（去掉"知道答案往答案上凑"的偏差）→ 对 ground truth 算逐字 CER（编辑距离/GT 长度）。样本 = 项目作者本人手迹，同一页含「工整带圈清单」+「快写连笔句子」两种风格。

| 书写风格 | 内容（汉字数） | 原生缩略图→vision | 反解自渲染→vision | 传统 OCR（tesseract chi_sim） |
| --- | --- | --- | --- | --- |
| 工整 | ①手写②工作③读书④打游戏 | **100%** | **100%** | ≈0%（乱码） |
| 快写连笔 | 一段短句（23 字） | **65%**（CER 35%） | 57%（CER 43%） | ≈0% |

**四条定案**：

1. **传统印刷体 OCR（tesseract）对中文手写判死**——喂它出的是随机字，工整/快写都 ≈0%。别在这条上投入。置信度高。
2. **多模态 vision 是唯一可用识别路**；工整近满分，快写掉到 ~60%，且是「**局部整段崩**」而非均匀掉字（写快提速那段整体失真——信息在笔迹里已不存在，任何引擎救不回，非引擎问题）。置信度中：n 小，但两条独立盲测在工整样本上均 100%、快写样本上彼此大幅分歧，「独立识别的一致性」本身即可信度指标。
3. **喂进去的渲染质量 ≈ 值 8 分**：xochitl 原生缩略图 65% > 自制均匀细线渲染 57%（同样本同模型，唯一变量是渲染）。→ 管线直接**收割 xochitl 自动生成的 `<doc>.thumbnails/<page>.png`**（真实线宽/抗锯齿、零渲染代码，等于设备免费给的「截图」），不自写栅格器、不碰活体截屏 DRM ioctl。缩略图既是现成「截图」、又不牺牲离线（文件已在 `/home`）。置信度高。
4. **定位 = 辅助转写 + 人工校对**：工整页近满分可直入；快写 60% 直接入库不可信，须带落地前 review/改错（改 1/3 字 << 全部重打，仍是净加速）。

**唯一能再拔高快写的杠杆**：`.rm` 自带笔顺/时序，online-HWR（用笔画动态而非静态图像）理论上吃得到 offline 图像识别吃不到的信息——但无现成中文离线引擎，要上云或自训，成本高，**留作后续可选增强**。

**生产后端实测反超 de-risk 保守值**：MVP 换 Gemini 3.6 Flash 后，同一张 384px 缩略图上工整清单 100%、**快写行 91%**（23 字仅错 2）——de-risk 表里 65%/57% 是 de-risk 阶段的读数。这把「快写~60%」下限抬了一档，但「需校对」定位不变（91% 仍非满分、样本小）。⚠ 各家模型名漂移快（`gemini-2.5-flash` 已对新用户下线 → 须用 `gemini-3.6-flash`），`vision.py` 默认值要跟着更。**★注：Gemini 这组质量数据仍有效，但生产默认后端已定为 DeepSeek**——国内用户设备无法挂代理、Gemini 国内不可达，DeepSeek 国内直连可用（质量代价见 §04「后端定案」）。

**落盘延迟坐实**：手写后停在原页时 host 侧 `.rm`/缩略图**均不更新**，退回书库才落盘。管线取数据必须卡在「切页/退出」之后（与下文 rmkit-cn 警示一致）。

---

## 03｜cardhw · 卡片手写批注注入（host 版真机通）

P1 OCR 之上的第一个「消费端」，也是 **P1（OCR）+ P2（手写-文字关联）合体**：给 PKM「总结卡片」加手写批注。用户在某条书摘 `·` 条目旁手写想法 → 整页喂 vision 做**空间关联**（判断手写贴哪条黑色打印条目，输出 `{anchor:黑字条目原文, note:手写转写}`）→ 把手写转写**内联拼接**到该条目行尾 → **模式 A** 重建为纯文本页（手写被消化成文字）。

- **机制关键**：手写笔划在 rmscene 层**不暴露 anchor**（§05 探针说的 anchor_id 抠不出来），故走 **vision 空间关联**而非抠坐标——比啃 P2 anchor 逆向更稳、且零逆向。关联准度：盲测 4/4、活体 Gemini 3/3。
- **三条实战打磨**（真机反馈驱动）：
  ① 只转写**手写批注**、不读印刷字（早期靠颜色分，端化版已改**按字形区分**：机器字规整 vs 手写连笔、兼容黑笔，见 §04 prompt 改进）；
  ② **内联拼接**在书摘行尾（不另起行）——「写在哪就贴哪」；
  ③ **打印文字泄漏结构性过滤**：工具手握整页 RootText 全部打印 bullet 原文，任何 vision `note` 与某打印 bullet 高度雷同（相似度>0.85）即判泄漏丢弃（真机撞过：Gemini 把打印书摘「只想睡觉」当手写挂到「她」上，被过滤拦下）——比调提示词可靠。
- **保留契约 + daemon 配套改**：转写行搭 `cardsync::parse_card` 保留通道（★块下非★行逐字保留）。但内联拼接改了书摘行文本，`merge_highlights` 去重须改 **`t+空格` 前缀匹配**（「只想睡觉 但醒了」仍认作书摘「只想睡觉」不重复插；「她」不误配「她站在…」），否则重建重复。改 daemon 核心 → 需重新部署才生效；+2 单测。
- **真机 E2E**：dry-run 报「内联方案 + 泄漏丢弃」→ `--apply`（先备份原 `.rm`）→ 回拉设备读 RootText 确认：笔划=0（手写消化）、批注内联到正确条目、泄漏未入、无重复。
- **代码**：host = `pkm-semantic/handwriting/{vision.py（卡片模式 transcribe_card）, cardhw.py}`；daemon 去重 = `pkm/src/cardsync.rs`。注入用 `rmscene.simple_text_document`（`notebook_rm` 的对拍参照源）。

---

## 04｜端化 cardhw（设备自主调云，A1/A2/B/C + 端到端真机通）

host 版要「回电脑连云」才能转写；端化版让**设备自己调云 + 走 /upload 注入**，脱离 host。**2026-08-29 端到端真机跑通**：设备写手写批注 → 关笔记 → daemon 触发 → DeepSeek 识别 → 内联注入 → `/upload` 重建 → 手写消化成文字 → 设备重开卡片肉眼可见（详见本节末「端到端真机验证 + 后端定案」）。

![端化 cardhw 数据流（关笔记事件 → 设备调云 vision → 内联注入 → /upload 重建）](cardhw-ondevice-flow.svg)

**架构定案（两条铁律驱动）**：

- **可见性必走 /upload**：xochitl 零 inotify、维护全内存文档模型 → **直写 `.rm` 运行时不可见**（连重开都只见旧页）。故端化注入**必走 `/upload` 重建路**（复用 `notebook_sync::sync_auto_notebook`，同 ★卡片），把转写并进卡片重建、手写消化成文字。
- **设备能自己调云**（两枪 de-risk 通）：① 设备 `ureq+rustls` 发 HTTPS 到云端多模态 API（de-risk 用 Gemini 验证，**生产默认 DeepSeek**，见本节末「后端定案」）——**与 reading weread 同机制、生产已验证**；真机实测设备自主拿到标注（rustls 证书校验穿过 host 代理也通=SNI 透传非 MITM；⚠ 但 fake-ip 代理只管本机不管设备转发流量→超时，端云需真直连，DeepSeek 国内直连无需代理）。② 通知：**daemon（Rust）够不到 QML 通知 API** → 「daemon 写状态文件 + 注入 QML 观察器轮询」桥接（Phase C，已建）。⚠ 通知用的是 **MainView 的全局 `notificationQueue.enqueue`**，非 reader 局部 `showNotification`（后者关笔记后回书库够不着）——详见本节末「端化 C」。

**A1 · 设备调云核心**（真机端到端）：交叉编 `cj-cardhw`（aarch64-musl 静态 1.7MB 含 rustls）→ 部署设备 → 设备自己调 Gemini → 内联注入 → `/upload` 重建。回拉新卡确认：笔划=0、批注内联到正确书摘行、打印泄漏未入、旧重名卡 `parent=trash`（`sync_auto_notebook` 顺带合并重名）。代码 `pkm/src/cardhw.rs`（vision 适配器 ureq + inject 移植，5 单测）+ `pkm/src/bin/cj_cardhw.rs`。

**A2 · 事件触发**（真机通）：daemon `settle` 加步骤④——`fswatch CLOSE_WRITE`（退出笔记落盘）事件里，对**在库「总结卡片」**（`is_active_summary_card`，排除 trash）跑 `process_card_doc`。**仅事件驱动**（冷启动不跑，免开机批量调云）；失败只记不崩、不阻塞画星；observe 跳过。

> **踩坑 + 根治自循环**：处理后 `queue_trash` 旧手写卡是异步，trash-agent 改其 metadata=trash 又是写事件 → step④ 拿「已消化但未删的旧卡 `.rm`（笔划仍在）」重转写 → 模型输出微变 → `sync_auto_notebook` 总认为有改动 → 无限 /upload。**双重根治**：① 排除 `parent=trash` 的卡；② **内容哈希幂等**（`cardhw-done.txt` 记已处理手写页 `.rm` 的 md5，不依赖 trash 时序，exactly-once）。真机复测：关笔记 → `[cardhw]` 注入恰好 1 次、90s 无重复、daemon 回零唤醒。

**配置**：daemon 读 `reading-qol.json` 的 `cardhwEnabled`/`cardhwProvider`/`cardhwModel`；API key 读 `cardhw.key` 文件（明文，Phase B 由设置面板写）。设备侧 wifi 需有网（正常场景）。

**B · 设置面板**（真机通，2026-08-29）：系统增强设置页新增「手写识别」二级页（qmldiff 哨兵 990007，块⑥），三控件——主开关（写 `cardhwEnabled`）、识别后端 4 段互斥（Gemini/DeepSeek/OpenAI/Claude→`cardhwProvider`）、模型可选框（`cardhwModel`）+ API Key 框（失焦即存、框清空只留尾号回显、echoMode 密文，单独明文写 `cardhw.key` 不进 json）。三语 cn/tw/en，复用本文件既有惯用法（Toggle Panel/分段 Repeater/边框 TextInput），无新选择器。**三页互覆盖防护**：翻页/书籍/笔记三个写 `reading-qol.json` 的二级页 load+save 同步带上 cardhw 三键，全量回写不抹。**离线验证**：用设备同款 `asivery/qmldiff` 对全份 .qmd 实跑 apply-diffs（1 diff·exit 0·emit 干净·注释合法无裸分号污染·990007 三处到位）。**真机 E2E**：改前备份（QRR 外，防 .qmd 双载）→ scp → md5 本地=设备 → restart → 健康检查绿（is-active=active、MainPID 变、NRestarts=0、qrr in maps=5、qmldiff 加载无 parse 错）→ 屏上操作：开关/选 DeepSeek/填 key 三项落地核对通过（`reading-qol.json` 三键正确 + 其余 14 键未被覆盖 + `cardhw.key` 写入）。qmd 落 /home 分区 OTA 不丢、重启能拉起。

**端到端真机验证 + 后端定案 + inject 修复**（2026-08-29，DeepSeek 实测）：

- **端到端跑通**：用户在《人骨拼图》- 总结卡片手写「这是一句总结」→ 关笔记 → daemon `fswatch` 触发 step④ → DeepSeek 识别 → `inject_inline` 注入 → `sync_auto_notebook` /upload 重建（生成新卡、旧卡入 trash、手写笔划消化成文字）→ 用户设备上肉眼确认手写变成印刷体拼在书摘行后。daemon 日志 `[cardhw] 注入 1 条并 /upload（手写消化）` 为证。
- **★后端定案 = DeepSeek（`deepseek-v4-flash-vision-exp`）**：目标用户群在国内、**设备自身无法挂代理、多数用户路由也无代理**；DeepSeek 是国内多模态 API、**直连国内网即可用、无需任何代理**——这是务实的生产默认。**Gemini 判为不适用**（`generativelanguage.googleapis.com` 国内需代理，设备端走不通）——**这反转了早前 §02/§03 里"推荐 Gemini / Gemini 质量更好所以用它"的取舍**：Gemini 质量确实更好且无 384 上限，但**在国内直连场景不可达**，故 Gemini 保留为四后端之一（有代理/海外用户可选），**默认与推荐都是 DeepSeek**。✅ 代码缺省已改 DeepSeek（`cj_stars_daemon`/`cj_cardhw`/面板 4 页，2026-08-30），面板仍可手选四后端。
- **DeepSeek 质量实况（384-token 图像上限的代价）**：稀疏测试卡潦草手写 1/4；规整真书卡上清楚单句核心读对（「这是一句总结」✅）但**易把附近印刷划线掺进 note**（384 低分辨率分不清印刷/手写）。定位仍是「辅助转写、需校对」。**用笔建议：手写用红/蓝等非黑色**给模型最强区分线索。
- **网络出口坑**：设备端调云需**真直连**。实测被 host 的 clash-meta **fake-ip**（`Meta` TUN，把 `api.deepseek.com` 解析到 `28.0.x`）挡过——该代理只拦 host 本机 OUTPUT、不管设备 USB 转发/热点流量 → 设备 TLS 连上但读应答超时。**DeepSeek 国内直连无需代理**正好绕开此坑；用带 fake-ip 的网反而不通。。
- **inject 修复三处**（真机暴露、`cardhw.rs` + 7 单测）：① `inject_inline` 的 `applied` **无条件 push** 致误报"已注入"→ `sync` 判"有变化"却内容相同 → 空转/None → 改为**仅真拼上才记 applied**，`ends_with` 命中（幂等/泄漏回环）归 `leaked`；② 新增 `strip_printed_prefix`——低分辨率下 vision 常把「印刷条目 + 手写」连成一串，剥掉印刷前缀只留手写；③ `CARD_PROMPT` 从「彩色=手写、黑色=印刷」改为**按字形区分**（机器字规整 vs 手写潦草连笔），兼容黑笔手写、强化"绝不把印刷字算进 note、宁漏不误"。

**端化 C（通知桥 + token 统计）——2026-08-30 真机通**：
- **通知桥**：daemon 处理卡片时写 `cardhw-status.json`（`cardhw_status::write_status`，`{state,book,count,seq,ts}`，**单调 seq** 让观察器凭 seq 变化认出新事件）→ 注入 QML 观察器（`cardhw-notify.qmd`，5s Timer 轮询）→ 弹原生通知「处理中→完成/失败」。
  - **⚠ 关键设计修正：注入 MainView 而非 DeviceSceneView，用 `notificationQueue.enqueue` 而非 `showNotification`**。离线反编译坐实（`extract_qml`）：reader 的 `showNotification`（`DeviceSceneView.qml`）是**阅读器局部浮标、只在开着文档时可见**；而 cardhw 在**关笔记**后触发，完成时用户已回书库 → reader 浮标够不着。**MainView**（`device.view.main`，书库+文档视图的常驻宿主）持有全局 `required property NotificationQueue notificationQueue`（`xofm.libs.notificationbar`），`notificationQueue.enqueue({text,icon,timeout,...})` 是**全局命令式通知队列**（原生 31 处在用），书库上下文里就渲染这条——这才是关笔记后能被看见的通知面。锚点 `TRAVERSE ?#root → LOCATE AFTER FocusScope#rootItem`；路径 `/qml/device/view/main/MainView.qml`（qmldir `MainView 1.0 MainView.qml` + 同族路径印证，运行期 `Processing file …MainView.qml` 命中）。
  - **真机验证**（隔离测法，脱开云/网络）：手写 `cardhw-status.json` 造 seq 递增事件 → 观察器日志 `CJ-HW-ENQUEUE … nq=obj` + `CJ-HW-AFTER cur={《人骨拼图》已消化 3 条手写批注} docLoaded=false`（enqueue 执行、`currentNotification` 正确设值、库视图）→ 连发 10 条用户**肉眼确认书库通知条可见**。踩坑：观察器**首见 seq 作静默基线不补发**，故隔离测须先写一次establish基线、再 bump seq 才弹（真运行天然满足：观察器先于 daemon 写就绪）；通知是 5s toast，单发易错过、连发才稳。
- **token 统计**：`device-core::vision::call_vision` 归一各家 `usage`（openai 系 prompt/completion_tokens、anthropic 系 input/output_tokens）→ 随 `ProcessOutcome.usage` 上抛 → daemon `cardhw_status::accumulate_usage` 按 provider 累计进 `cardhw-usage.json`（`{provider:{calls,input_tokens,output_tokens}}`，全零跳过不虚增）。`cj-cardhw` CLI 也打单次 token 行。（面板展示 usage 为后续小改，文件已就位。）

**后端缺省已定案 = DeepSeek**（原"待定"已结）：`cj_stars_daemon` 的 `cardhwProvider` 缺省 + `cj_cardhw` `--provider` 缺省 + 设置面板 4 页 `cfgProvider` 默认全部 `gemini`→`deepseek`，与生产定案一致；面板仍可手选四后端。

---

## 05｜反解基础设施（P2 手写-文字 anchor）

用 rmscene 反解 `.rm` 把手写笔记结构化导出。核心抓手是真机验证的事实——reMarkable 原生 notebook 已把手写笔迹 anchor 到 typed text 的字符位置（`anchor_id` + `origin_x`）——据此把「打字正文 + 手写批注」的混排笔记按锚点合并导出成结构有序 Markdown（手写部分经识别管线转文字，或按位内嵌图片）。

**承接原「闭环3」反解成果**（此前为微信读书做的 rmscene 反解探针不废弃，正是这里的地基）：

- **已落地**：`rm-export/export.py` 的 `page_highlights()` 能读一页 `.rm` 的 `GlyphRange`，取高亮文本 + 字符 offset + 颜色、拼回整句。文档库遍历骨架（`glob *.metadata` → uuid → `<uuid>/*.rm`）现成。
- **已知边界**：原生 EPUB 阅读页只能手写批注、不能打字 → anchor 混排导出只适用 notebook 场景；批注（note）尚未在反解数据中定位（目前只通了高亮）；`.rm` 格式无文档，固件升级可能引入新 block 类型需跟进。
- **cardhw 为何绕开 anchor**：手写笔划的 anchor_id 在 rmscene 层抠不出来，故 cardhw 走 vision 空间关联（§03）而非坐标锚点——精确 anchor 逆向留作后续精度升级。

---

## 06｜竞品借鉴（rmkit-cn）

> 来源：`boangs/rmkit`（GPL-3.0）。**源码研读结论。**

**① 截屏喂多模态 vision，绕开反解坐标**（`ai_glyph.go`）：rmkit-cn 手写 AI 走**截屏 → 裁剪 → base64 PNG → OpenAI vision 格式识别**，不做纯 OCR、直接让模型「先识别成文本再按指令处理」。对「手写→可检索文字」的纯识别目标是低成本捷径。**本项目取舍**：用 xochitl 现成缩略图当「截图」（de-risk §02 定），既省截屏 DRM、又保离线；结构化导出（§05）仍走 rmscene 反解保位置信息。

**② `.rm` 不实时刷新**（`ai_page.go` 头注）：rmkit-cn 服务端扫 `.rm` 拼整页文字，后来改掉——注释写明「`.rm` 不实时刷新，用户点 AI 要等几秒」。对本项目直接警示：读 `.rm` 反解都会撞落盘延迟，xochitl 不在每次操作后立即写盘。落地卡在「切页/退出」时机（cardhw 的 CLOSE_WRITE 触发正是此时）。置信度高（与「xochitl inotify 不监视文档目录」硬证据互印证）。

---

## 07｜未来方向

- **结构识别**：`.rm` 笔迹 → 结构化格式。手写数字/实心圆 = 有序/无序列表、方框 = 待办清单。复用本块的反解 + 几何判据（同 ★待办 stardetect 的形状识别思路）。
- **端化 B/C 已建**（B 设置面板 2026-08-29、C 通知桥+token 统计 2026-08-30，均真机通，见 §04）。**待办**：设置面板展示 `cardhw-usage.json`（token 累计已落文件、面板 UI 未接）；按笔划 bbox 裁剪缩略图，在 384-token 上限内换更高有效分辨率（提准度大杠杆）。
- **online-HWR 拔高快写**：吃 `.rm` 笔顺时序，但无现成中文离线引擎，需上云/自训——高成本、低优先。
