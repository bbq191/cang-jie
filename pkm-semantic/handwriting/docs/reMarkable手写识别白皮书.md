# reMarkable 手写识别白皮书（块⑥）

> **块⑥「手写识别」= 笔迹 → 文字/结构的通用识别引擎。** 与五大分块都不同：不改设备行为、只读设备文件 + host/设备侧调云识别。首个能力是「手写批注 → 文字」（cardhw + freeform export），未来扩到「手写结构 → 有序/无序列表、待办」（数字/实心圆=列表、方框=待办）。
>
> ⚠️ **跨块关系**：本块代码骑在 `pkm/` daemon 上（端化 cardhw 复用 `cardsync`/`notebook_sync`/`notebook_rm`/`fswatch`），概念独立成块——同「划词查字典」（块4代码在 pkm）的处理。与 PKM 的 ★待办（手绘星→语义）**同源但不同块**：★待办深绑总结卡片、留块5；块⑥是**通用识别引擎**，★待办算它的近亲不算成员。
>
> 优先级/分块地图见顶层《[功能路线图白皮书](../../../docs/reMarkable功能路线图白皮书.md)》（路线图 P1/P2 = 本块）；反解基础设施与 PKM 共用 `device-core`，架构见《[设备端 Rust 架构](../../../docs/reMarkable设备端Rust架构.md)》。

---

## 01｜概述与定位

**做什么**：设备上的手写中文笔记 → 识别成文字/结构，让手写可检索、可引用、进 PKM。**定位**：不追求设备上实时识别（那需设备端算力 + 深度 hook，风险高），追求「写完之后笔记不死在设备里」——是 Supernote「实时识别」卖点的**流转版**替代。

**两个已落地成员** + 一个方向：

| 成员 | 形态 | 状态 |
| --- | --- | --- |
| **cardhw · 卡片手写批注注入** | 卡片某槽旁手写想法 → vision 空间关联 → 内联注入书摘行 | host 版真机通；**端化 A1/A2 真机通** |
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

**生产后端实测反超 de-risk 保守值**：MVP 换 Gemini 3.6 Flash 后，同一张 384px 缩略图上工整清单 100%、**快写行 91%**（23 字仅错 2）——de-risk 表里 65%/57% 是 de-risk 阶段的读数。这把「快写~60%」下限抬了一档，但「需校对」定位不变（91% 仍非满分、样本小）。⚠ 各家模型名漂移快（`gemini-2.5-flash` 已对新用户下线 → 须用 `gemini-3.6-flash`），`vision.py` 默认值要跟着更。

**落盘延迟坐实**：手写后停在原页时 host 侧 `.rm`/缩略图**均不更新**，退回书库才落盘。管线取数据必须卡在「切页/退出」之后（与下文 rmkit-cn 警示一致）。

---

## 03｜cardhw · 卡片手写批注注入（host 版真机通）

P1 OCR 之上的第一个「消费端」，也是 **P1（OCR）+ P2（手写-文字关联）合体**：给 PKM「总结卡片」加手写批注。用户在某条书摘 `·` 条目旁手写想法 → 整页喂 vision 做**空间关联**（判断手写贴哪条黑色打印条目，输出 `{anchor:黑字条目原文, note:手写转写}`）→ 把手写转写**内联拼接**到该条目行尾 → **模式 A** 重建为纯文本页（手写被消化成文字）。

- **机制关键**：手写笔划在 rmscene 层**不暴露 anchor**（§05 探针说的 anchor_id 抠不出来），故走 **vision 空间关联**而非抠坐标——比啃 P2 anchor 逆向更稳、且零逆向。关联准度：盲测 4/4、活体 Gemini 3/3。
- **三条实战打磨**（真机反馈驱动）：
  ① 只转写**彩色手写**、不读黑色打印字；
  ② **内联拼接**在书摘行尾（不另起行）——「写在哪就贴哪」；
  ③ **打印文字泄漏结构性过滤**：工具手握整页 RootText 全部打印 bullet 原文，任何 vision `note` 与某打印 bullet 高度雷同（相似度>0.85）即判泄漏丢弃（真机撞过：Gemini 把打印书摘「只想睡觉」当手写挂到「她」上，被过滤拦下）——比调提示词可靠。
- **保留契约 + daemon 配套改**：转写行搭 `cardsync::parse_card` 保留通道（★块下非★行逐字保留）。但内联拼接改了书摘行文本，`merge_highlights` 去重须改 **`t+空格` 前缀匹配**（「只想睡觉 但醒了」仍认作书摘「只想睡觉」不重复插；「她」不误配「她站在…」），否则重建重复。改 daemon 核心 → 需重新部署才生效；+2 单测。
- **真机 E2E**：dry-run 报「内联方案 + 泄漏丢弃」→ `--apply`（先备份原 `.rm`）→ 回拉设备读 RootText 确认：笔划=0（手写消化）、批注内联到正确条目、泄漏未入、无重复。
- **代码**：host = `pkm-semantic/handwriting/{vision.py（卡片模式 transcribe_card）, cardhw.py}`；daemon 去重 = `pkm/src/cardsync.rs`。注入用 `rmscene.simple_text_document`（`notebook_rm` 的对拍参照源）。

---

## 04｜端化 cardhw（设备自主调云，A1/A2 真机通）

host 版要「回电脑连云」才能转写；端化版让**设备自己调云 + 走 /upload 注入**，脱离 host。

![端化 cardhw 数据流（关笔记事件 → 设备调云 vision → 内联注入 → /upload 重建）](cardhw-ondevice-flow.svg)

**架构定案（两条铁律驱动）**：

- **可见性必走 /upload**：xochitl 零 inotify、维护全内存文档模型 → **直写 `.rm` 运行时不可见**（连重开都只见旧页）。故端化注入**必走 `/upload` 重建路**（复用 `notebook_sync::sync_auto_notebook`，同 ★卡片），把转写并进卡片重建、手写消化成文字。
- **设备能自己调云**（两枪 de-risk 通）：① 设备 `ureq+rustls` 发 HTTPS 到 Gemini——**与 reading weread 同机制、生产已验证**；真机实测设备自主拿到标注（rustls 证书校验穿过 host 代理也通=SNI 透传非 MITM；真脱机用设备自己 wifi 直连更无碍）。② 通知：原生 `showNotification({message},ms)` 可从注入 QML 调（`reader-footnote-return.qmd` 已用），但 **daemon（Rust）够不到它** → 需「daemon 写状态文件 + 注入 QML 观察器轮询」桥接（Phase C）。

**A1 · 设备调云核心**（真机端到端）：交叉编 `wr-cardhw`（aarch64-musl 静态 1.7MB 含 rustls）→ 部署设备 → 设备自己调 Gemini → 内联注入 → `/upload` 重建。回拉新卡确认：笔划=0、批注内联到正确书摘行、打印泄漏未入、旧重名卡 `parent=trash`（`sync_auto_notebook` 顺带合并重名）。代码 `pkm/src/cardhw.rs`（vision 适配器 ureq + inject 移植，5 单测）+ `pkm/src/bin/wr_cardhw.rs`。

**A2 · 事件触发**（真机通）：daemon `settle` 加步骤④——`fswatch CLOSE_WRITE`（退出笔记落盘）事件里，对**在库「总结卡片」**（`is_active_summary_card`，排除 trash）跑 `process_card_doc`。**仅事件驱动**（冷启动不跑，免开机批量调云）；失败只记不崩、不阻塞画星；observe 跳过。

> **踩坑 + 根治自循环**：处理后 `queue_trash` 旧手写卡是异步，trash-agent 改其 metadata=trash 又是写事件 → step④ 拿「已消化但未删的旧卡 `.rm`（笔划仍在）」重转写 → 模型输出微变 → `sync_auto_notebook` 总认为有改动 → 无限 /upload。**双重根治**：① 排除 `parent=trash` 的卡；② **内容哈希幂等**（`cardhw-done.txt` 记已处理手写页 `.rm` 的 md5，不依赖 trash 时序，exactly-once）。真机复测：关笔记 → `[cardhw]` 注入恰好 1 次、90s 无重复、daemon 回零唤醒。

**配置**：daemon 读 `reading-qol.json` 的 `cardhwEnabled`/`cardhwProvider`/`cardhwModel`；API key 读 `cardhw.key` 文件（明文，Phase B 由设置面板写）。设备侧 wifi 需有网（正常场景）。

**未建**：**B 设置面板**（系统增强页开关 + 选模型 + 填 key）；**C 通知桥**（daemon 写状态 + 注入 QML 观察器调 `showNotification`）+ token 统计。

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
- **端化 B/C**：设置面板（选模型/填 key）+ 通知桥 + token 统计（各家 API 回传 `usage` 字段落本地计数）。
- **online-HWR 拔高快写**：吃 `.rm` 笔顺时序，但无现成中文离线引擎，需上云/自训——高成本、低优先。
