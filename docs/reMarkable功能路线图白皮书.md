# reMarkable Paper Pro Move 功能路线图白皮书

> 方向评估与优先级白皮书 · v1.0 · 2026-08

在 UI 中文化与拼音输入法两条线基本收工（见姊妹文档《[reMarkable 中文化白皮书](../chinese-ime/docs/reMarkable中文化白皮书.md)》《[reMarkable 拼音输入法白皮书](../chinese-ime/docs/reMarkable拼音输入法白皮书.md)》）之后，本文回答一个问题：**这台设备上，下一步做什么最值。**

> 注：中文化核心已归拢进 `chinese-ime/` 子文件夹（2026-08-15 重构）。本文中出现的 `xovi-extensions/cangjie-langhook/`、`pinyin-engine/` 等旧路径一律对应 `chinese-ime/langhook/`、`chinese-ime/pinyin-engine/`；`rm-export/`/`weread-client/` 仍在顶层。

**范围声明：** 本文是方向评估文档，不是实现方案——只做候选功能的价值排序、可行性论证和否决理由，具体实现细节待立项后另开方案文档。评估范围同样限定"本机个人定制"，不涉及破解 DRM、绕过付费校验或分发修改版固件。

## 本章目录

1. [01 · 结论速览](#01结论速览)
2. [02 · 评估框架](#02评估框架)
3. [03 · 生态盘点](#03生态盘点)
4. [04 · 已否决方向及理由](#04已否决方向及理由)
5. [05 · P0：微信读书集成——「墨香」（MoXiang），全设备自足](#05p0微信读书集成墨香moxiang全设备自足)
6. [06 · P1：手写 OCR → Markdown → Obsidian 数据流转](#06p1手写-ocr--markdown--obsidian-数据流转)
7. [07 · P2：手写笔记的反解与导出（含手写-文字 anchor）](#07p2手写笔记的反解与导出含手写-文字-anchor)
8. [08 · P3：中文字体渲染优化](#08p3中文字体渲染优化)
9. [09 · P4：笔记 QoL——先用社区现成扩展](#09p4笔记-qol先用社区现成扩展)
10. [10 · 候选方向：设备端 AI 助手（借鉴 rmkit-cn，未定级）](#10候选方向设备端-ai-助手借鉴-rmkit-cn未定级)
11. [11 · 共同风险与架构原则](#11共同风险与架构原则)

## 01｜结论速览

| 优先级          | 方向                                                | 一句话理由                                                                                                                                                                              | 地基完成度         | 置信度 |
| --------------- | --------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------ | ------ |
| **P0 ✅**       | 微信读书集成——「墨香」，全设备自足                  | 用原生阅读器+原生笔读微信读书的书；**全设备自足 Rust 二进制 + 墨香整页 app + 阅读器菜单**（host 脐带剪断）；扫码登录/下载/划线想法**双向同步**全设备端；净室自研借鉴 MiuRead 逻辑不抄码 | **端到端真机验证** | 高     |
| **PKM ✅**      | ★ 全局待办 —— PKM 语义引擎首个能力                  | 阅读时红笔画五角星 → 后台 Rust daemon 自动汇总成每本书的「总结卡片」笔记本（一星一页 + 章名·页号 + 可打字批注模板）；纯事件驱动（挂文档模型 `onRowsInserted`）、全设备自足、设置页开关  | **端到端真机验证** | 高     |
| **P1**          | 手写 OCR → Markdown → Obsidian                      | 生态盘点确认的差异化空白，OCR 放 host 侧绕开设备风险                                                                                                                                    | 0%（方向已定）     | 中     |
| **P2**          | 手写笔记反解与导出（含手写-文字 anchor）            | 承接原"闭环3"rmscene 反解成果；原生已把笔迹锚到字符位置，"混排笔记结构化导出"没人做过                                                                                                   | 反解探针已验证     | 中     |
| **P3**          | 中文字体渲染优化                                    | 显示清晰度唯一可行子集，文件级改动、低风险、独立小胜                                                                                                                                    | 字体管线已有       | 高     |
| **P4**          | 笔记 QoL（手势/目录等）                             | 社区 xovi 扩展现成，先装后评，只自研真缺的                                                                                                                                              | 现成可装           | 高     |
| **候选·未定级** | 设备端 AI 助手（选中文字/手写→润色/翻译/总结/问答） | 借鉴 rmkit-cn，机制现成（截屏 vision + 剪贴板取词）；但偏"消费"而非本项目"数据流转"定位、依赖云端 API                                                                                   | 机制现成           | 中     |
| **否决**        | 屏幕高刷、泛化清晰度优化、局域网传文件              | 见第 04 节                                                                                                                                                                              | —                  | 高     |

## 02｜评估框架

排序不是拍脑袋，用四个因子：

> **优先级 ≈ 用户价值 × 差异化 × 可行性 ÷ 风险**

- **用户价值**：对"用这台设备做中文阅读/笔记的人"的日常增益，不是技术炫技值。
- **差异化**：官方 roadmap 和社区 xovi 生态都没覆盖的才算数。已有人做好的，装现成的。
- **可行性**：优先选地基已打（本项目已有真机验证结论）的方向；其次选能离机开发的方向。
- **风险**：两类——设备风险（写坏内存/固件、OTA 打断）和沉没风险（官方一次更新原生实现，投入归零）。

由此推出一条贯穿全文的架构偏好：**逻辑尽量放 host/伴生工具侧，设备侧只留最薄一层，甚至零 hook**。中文化和输入法两条线被 OTA `.166` 打断过一次的教训（后靠纯特征码自定位恢复）说明：设备内 hook 的每一行代码都是 OTA 的受灾面。数据流转类功能天然满足这条偏好——`.rm` 文件反解在 host 跑，设备只是数据源。

> **★该偏好在 P0（微信读书/墨香）上被有意反转、并证明是对的**：墨香最终走成"**全设备自足**"（重活全在设备端 Rust 二进制，host 脐带剪断），与上面"设备侧最薄"相反。原因是墙二（设备无 python，host-tether 要长期连着 USB 跑 Python 很别扭）+ 墨香的重活是**普通用户态二进制**（不是 langhook 那种进程内 hook，OTA 受灾面小、崩了也不砖）+ 设备 wifi 独立可达云端。所以偏好的**真实内核不是"逻辑离机"，而是"设备内的高危面（进程内 hook / 改内存 / 动核心服务启动依赖）越少越好"**——用户态静态二进制 + QML 明文注入既满足这条内核、又换来"拔了 USB 也能独立用"。。

## 03｜生态盘点

动手前先看各方占了什么坑，避免重复劳动：

**官方（reMarkable）**：打字笔记、手写识别（不含中文流转）、USB Web UI 传文件、云同步。OTA 节奏活跃，任何"对标官方补功能"的项目都活在它的阴影下。

**社区 xovi 生态**：[rm-hacks-xovi-qmd](https://github.com/Samarkin/rm-hacks-xovi-qmd)（笔/选择工具切换手势、隐藏关闭按钮适配 Move 小屏）、[rmitchellscott/xovi-qmd-extensions](https://github.com/rmitchellscott/xovi-qmd-extensions)（可折叠目录等 QoL）、[awesome-reMarkable](https://github.com/rehackable/awesome-remarkable) 索引。QoL 层面轮子已经不少。

**中文化竞品（与本项目正面重叠，2026-08-14 源码研读补充）**：

- **[boangs/rmkit](https://github.com/boangs/rmkit)（"rmkit-cn"，GPL-3.0）**：同样基于 xovi 的大而全中文工具箱——UI 汉化 + 拼音 IME（Go 常驻服务 `ime-go` + uinput 模拟按键注入）+ AI 润色/翻译 + 扫码传文件 + 小游戏。已成为这条赛道的"上游内核"。架构与本项目相反（进程外解耦 vs 进程内原生 hook），取舍对照见两份姊妹白皮书的 📌 借鉴块。
- **[pretenderlu/rmtool](https://github.com/pretenderlu/rmtool)（GPL-3.0）**：桌面 Python GUI，通过 SSH 管理设备。其拼音 IME/汉化的设备端组件**移植自 rmkit-cn**，自有功能是点击翻页、快速黑白阅读、刷新优化、字体/壁纸管理。工程上以"固件指纹白名单（架构+平台+固件+xochitl SHA-256 四维匹配）"著称，与本项目"特征码运行时自定位"是两种固件适配哲学。
- **[paperweight.cn](https://paperweight.cn)（镇纸，闭源付费）**：见 记录，商业化中文输入法 + Typing 模式 + 局域网传文件。

**结论：三家中文化竞品都没有碰"中文场景的数据流转"（阅读批注导出、手写中文 OCR、笔记生态对接、微信读书集成）——各方都空着的地带只有这一块。** 本文 P0–P2 全部落在这块，差异化定位第三次被印证。rmkit-cn / rmtool 的**阅读体验增强**（点击翻页、单色快刷）与本项目不冲突，作为可借鉴的 QoL 增量吸收（见 4.1 与 09 节）。

## 04｜已否决方向及理由

### 4.1 屏幕高刷 —— 否决，置信度：高

三重否定：

1. **够不到**：刷新波形（waveform）和刷新调度在独立的 EPD 控制器里执行，社区分析指向 E Ink 与 Himax 合作的 [T2000 TCON](https://myereader.substack.com/p/remarkable-paper-pro-and-gallery)，固件闭源且不在 xochitl 进程内——本项目全部 hook 能力（LD_PRELOAD 注入 xochitl）对它无效。
2. **不敢改**：波形表描述的是电泳粒子的驱动电压序列，改错是残影/烧屏级别的**永久硬件损伤**，违反本项目"出问题立即回退"的纪律——波形写坏没有回退。
3. **不值得**：收益上限被 Gallery 3 墨水的物理响应速度封死；rM1/2 时代的 [framebuffer 社区研究](https://github.com/ichaozi/RemarkableFramebuffer)也只是调用官方局刷 ioctl，从未有人成功改写波形，rMPP 上更无先例。

#### 📌 竞品借鉴修正（rmtool）·"阅读场景单色快刷"不属于本否决，已从 4.1 拆出、提级到 09 节立项

> 来源：`pretenderlu/rmtool`（GPL-3.0）`fast-mono-reading/qmd-src/fast-mono-reading-3.28.qmd`。机制源码研读；**本项目已按 .166 真实 QML 净室重写并真机验证 Mono 生效（2026-08-14，详见 9.1 节）。**

上面否决的是**改写 waveform**（自造驱动电压序列）——那是够不到、不敢改、不值得。但 rmtool 的 fast-mono-reading 揭示了一条**完全不同性质**的路，必须与 4.1 区分开：它用的是 **xochitl 固件自带的屏幕模式** `Epaper.ScreenModeItem.Mono`（QML 层可控），阅读时把彩色墨水屏锁成单色，跳过彩色 waveform 的多遍刷新——**不碰任何波形表、不写 EPD 控制器**。

**硬件安全评估（置信度：高）**：Mono 是厂商官方模式、走厂商自己的单色 waveform，与 4.1 第 2 条"改错=残影/烧屏级永久损伤"是两码事；唯一副作用是单色快刷累积的残影，用固件自带的 `ghostBuster.forceClearNow` 周期全刷即可清除（可逆），**无永久硬件损伤风险**。tap-page-turn（点击翻页）更是纯 QML 触摸映射、零硬件交互。

**结论**：这两个阅读增强不在 4.1 否决范围内。用户已明确"对阅读体验很重要"，**从"显示优化"里拆出、正式立项，记入 09 节**（并非 P3 字体那种"观感"优化，而是"翻页交互 + 刷新速度"的体验优化）。落地边界见 09 节。

### 4.2 泛化的"显示清晰度优化" —— 否决大部，保留一个子集

PPI（Move 为固定硬件参数）、Gallery 3 白态偏灰、色彩对比度，都是墨水屏物理与 TCON 固件决定的，软件层不可达。唯一例外是**中文字体渲染**——这部分真实可做且价值不低，单独立为 P3（第 08 节）。

### 4.3 局域网传文件 —— 否决，置信度：高

官方 USB Web UI 已有基础版，差异化低。且官方云同步 + 本项目 P1 的同步管线会顺带覆盖此需求。

### 4.4 逐项对标 Supernote 笔记功能 —— 不整体立项，拆解处理

逐项对标（手势擦除、套索、链接跳转、目录）本质是给官方 roadmap 打工：官方每次 OTA 都可能原生实现，同时弄坏 hook。拆解如下：

- 已有社区扩展的（手势、目录）→ P4，装现成的。
- Supernote 真正的护城河"笔记可检索/可流转"→ 正是 P1 + P2 的内容，以数据流转的形态实现，而非在设备上复刻 UI。

## 05｜P0：微信读书集成——「墨香」（MoXiang），全设备自足

**做什么**：让用户在 reMarkable Paper Pro Move 上用微信读书——扫码登录自己的账号、书下载进 xochitl 书库**原生阅读、原生笔划线**、划线/想法**双向同步**到微信读书云端。做成一个叫**「墨香」（MoXiang）** 的设备端功能：入口在 xochitl 首页侧边栏、展开为一个**整页 app**（像文件/设置那样的顶层页），另在 EPUB 阅读器「...」菜单加一项。功能逻辑**借鉴**[觅阅 MiuRead](https://github.com/miumiupy98-art/miuread-koreader)（4.3.2，非官方微信读书客户端，KOReader 插件，AGPL-3.0），**净室自研、不抄源码**（见 5.4）。

> **★架构定案（2026-08 演进，已真机端到端）**：从早期"**host Python 大脑 + 设备薄 hook**（scp/`/upload` + host 桥）"彻底演进到"**全设备自足**"——所有重活（扫码登录 / 分片下载 + 解码 + 组 EPUB / 注入书库 / 划线想法双向同步 / 会话续期 / 个人阅读数据）都由设备上的 **aarch64 Rust 静态二进制**完成，**host 脐带全剪断**。下面 5.2 是这条定案架构；5.3 的协议/解码事实不变，只是实现语言从 Python 逐字节移植成了 Rust（真机对拍一致）。原"KOReader 独立 app 复用路线"早已否决（切走 xochitl、丢原生笔/统一 UI），rmscene 反解成果转化为 P2 并与本节共享（见第 07 节）。

### 5.1 两堵决定架构的硬墙

**墙一 · 渲染器是独立进程**：xochitl 把 EPUB 转 PDF 渲染由**独立进程 `xochitl_pdf_renderer`** 干（真机日志实证，见《中文化白皮书》）。LD_PRELOAD hook 只注入主进程、**触达不到渲染进程**。推论：**"集成"不靠 hook 渲染器，而是**① 把书送进阅读器 = 往文档库**写文件**（sideload / `/upload`）② 抓用户划线 = 事后**读 `.rm`**反解——都不碰渲染进程，比"侵入渲染"干净安全一个量级。

**墙二 · 设备无通用运行时**：设备上**没有 python、没有通用包管理**（只有 busybox + glibc），/home 分区 45G 空，显示是 DRM 单 master 被 xochitl 独占。推论：① 设备端只能跑**自编的 aarch64 静态二进制**，不能依赖 host 的 python；② 自绘 reader 是死路（显示独占），唯一"零切换感"的路是**注入 QML** 进 xochitl（面板 / 阅读器菜单）。两墙合起来钉死了本项目的形态：**全设备自足 Rust 二进制 + QML 注入**。

### 5.2 现架构（三层，全设备自足）

**① 设备端 Rust 二进制**（`weread-client/device-rs/`，交叉编 `aarch64-unknown-linux-musl` **全静态**：ring 的 C 用 aarch64-gcc、musl 链接靠 rustc 自带 rust-lld）：

- **`wr-serve`**——常驻本地 HTTP 服务（`127.0.0.1:8777`，tiny_http），墨香面板与阅读器菜单都调它。端点：GET `/ping /status`(登录态) `/shelf /chapters /search /notebook/list /profile`；POST `/publish`(下书,可带拉笔记) `/notebook/sync_book`(双向智能同步) `/notebook/sync_current`(阅读器发送画线) `/login/*` `/logout`。
- **`wr-download`**——整本下载→组 EPUB→`/upload` 免重启注入；**`wr-renew`**（`wr-renew.timer` 定时续期 `wr_skey` 保活）。
- 三块"皇冠明珠"逐字节移植 Rust、真机对拍 Python 一致：`sign`（web_sign 签名）、`obfuscate`（bookId 编码）、`codec`（正文解码，**AGPL 派生·个人自用不分发**——编进 wr-\* 二进制、只跑在用户自己设备上处理自己账号的数据，不进任何分发渠道）。
- 网络：设备 wifi **独立可达**微信读书（经全局 fake-ip 代理，28.0.2.x DNS 劫持）。全 musl 静态，`wr-serve.service` + `wr-renew.timer` 持久自启（rootfs `/usr`，OTA 后 `install.sh` 一键恢复）。

**② 墨香面板 = xochitl 里的整页 app**（`device/moxiang-*.qmd`，qmldiff 明文注入，qt-resource-rebuilder 加载；明文失配只 log 不崩）：

- **入口**：xochitl 首页侧边栏「墨香」菜单项（注入 `Sidebar.qml` 的 `filterColumn`，图标=水墨"香"logo 经 rcc 重打）。
- **形态**：从早期**弹窗(Popup)** 演进为**整页**（注入 `Navigator.qml`：全屏 Loader z:100 盖住文件浏览器，系统同款 ✕ 关闭，点侧栏文件视图自动退出，独占全屏不透出）。演进动因：弹窗会盖住屏幕键盘、且透出后面书库；整页像打开文档一样干净、休眠即挂起。
- **视图**：**首页**（logo + 个人阅读概览卡[昵称/书龄/阅读时长/藏书/笔记/偏好，读 `/profile`] + 「下载新书」「我的笔记」两入口）；**下载新书**（书架浏览 / 全站搜索 / 选书 / 整本·前 N 章·章节区间 / 勾"同时拉取为划线笔记本"）；**我的笔记**（只列设备**真实存在**的书=云端有笔记 ∪ 本地墨香书，「智能同步」一键双向）；**扫码登录**（设备自绘二维码）。面板经本机 `127.0.0.1:8777` 调 wr-serve，不再走 host 桥。

**③ 阅读器集成**：EPUB 阅读器右上角「...」菜单加**「发送画线到笔记」**（注入 `Toolbar.qml`+`SettingsMenu.qml`+`DocumentView.qml` 三处，沿原生 toolbar 发信号→DocumentView 接的模式）——点了把当前书的本地画线发送成/并入笔记本（调 `/notebook/sync_current`→`sync_book`），底部原生 toast 反馈（进度常驻→结果替换）、点击即关菜单防误触。

**组件 3 · 划线 设备→云端（首版做这个，✅ 2026-08-14 真机端到端闭环，用户 App 确认位置准确）**——原生**荧光笔 + 对齐到文本(snap-to-text)** 划线 → `.rm` 的 `GlyphRange`（自带可见 text）→ `rm-export/export.py` 的 `page_highlights()` 反解 → `highlights/reverse.map_to_weread` 在**章节原始解码全文**（`download.fetch_chapter_document`，即微信读书 range 的 offset 坐标空间，真机 0 误差对拍）里字符串定位算 range（单段整段 find、跨段用 head/tail 锚，标签计入区间）→ `protocol/sync.add_bookmark` 写 `/web/book/addBookmark`（cookie 鉴权、无签名）。全程**只读设备文件 + 网络上行，不往原生阅读器写**。**关键突破：offset 坐标空间就是我们下载的章节内容，绕开了 `.epubindex` 二进制逆向**（见未决⑥修正）。踩坑：marker/普通荧光笔只留视觉笔画、无文本锚，必须 snap-to-text。

**组件 3b · 双向·笔记方案（✅ 2026-08-14 真机端到端闭环，用户 App 确认位置准确）**——不"画回阅读器"，而是把云端划线+想法落成一篇 reMarkable 原生**「笔记(notebook)」**，并做双向。先前把"云端高亮塞进 EPUB 原生目录(TOC)"的展示方案被用户否决（观感 + 与阅读割裂）。新方案链路：`highlights/cloud_index.fetch_book_highlights` 拉云端划线/想法 → `highlights/notebook.build_page_text` 排成纯文本（每条划线=`· 原文` / `〔wr chapterUid:start-end〕`定位锚 / `想法：`空行）→ `write_notebook_files` 用 `rmscene.simple_text_document` 写成一页 `.rm`（+`.content`/`.metadata`）→ scp 落文档库 + `restart xochitl` → 原生笔记正常渲染。用户在「想法：」后**打字**（靠 cangjie IME）→ 拉回 `.rm` → `read_page_text` 读 `RootTextBlock` → `parse_thoughts` 按锚点切块 → `protocol/sync.add_review` 写 `/web/review/add`。工具：`tools/highlights_to_notebook.py`（云→端，`--push --restart`）、`tools/notebook_to_reviews.py`（端→云，`--pull --dry-run`）。

**为何这是关键突破**：它给了"回写"**第三条路**——既不写原生阅读器内存（下方 Step S 最高危），也不靠 evdev 模拟笔（下方 📌，语义不等价的近似）。**建/读一篇独立笔记文件**是文件级操作，安全一个量级、可删回退，且回写的是**真·结构化文字**（不是画上去的笔划）。真机钉死的事实：① rmscene 写的 `.rm`，xochitl .166 正常渲染、rm-sync 不删凭空注入的新文档（读设备写的 `.rm` 有 "newer format" 警告，只影响场景元数据、不碰 `RootTextBlock` 文本）；② `/web/review/add` = cookie 鉴权、无 web_sign，body 的 `abstract`(划线原文)/`content`(想法) **明文不 base64**（区别于 addBookmark 的 markText=base64）、`type=1`；③ 读路径走 Agent 网关 Bearer `api_key`、写路径走 web 主机 cookie(wr_skey)，**独立过期**，`errCode -2012 登录超时`=cookie 失效（api_key 读仍好用）重登。**代价/脆弱点**：笔记与阅读页分离（不像 TOC 在书内）、"关联书籍"非 reMarkable 原生能力（靠命名/映射约定）、想法↔划线锚定靠可见 `〔wr〕` 锚点（用户删改锚点行会失配）。。

**组件 3c · 云端高亮/想法/评论 内联"画回"正文（✅ 2026-08-16 高亮 · 2026-08-17 想法+评论，真机验证通过——原"首版后置·不做"项已实现）**——此项原判为"画回阅读器"=写 `GlyphRange`=Step S 级最高危而后置。**风险重估（关键纠正）**：真正危险的是 hook xochitl **进程内存**（langhook）；"画回"其实是写 **`.rm`/EPUB 数据文件**，不进渲染进程地址空间，安全一个量级，真障碍是"坐标从哪来"而非"写的动作"。落地两条路、均真机验证：

- **路 A · GlyphRange 写渲染页 `.rm`**（`highlights/render_map.py` + `tools/inline_highlights.py`）：把云端高亮在 xochitl 渲染出的 `<uuid>.pdf` 文本层定位，用最小二乘标定的仿射变换 `scene = 3.1545·pdf + off`（各向同性、页局部、≈屏宽/页宽）算出 `Rectangle`，`rmscene` 写成高亮页 `.rm`（多高亮走 CRDT 链、页首/中页 start 均真机正确）→ scp + 安全重启。产出**可编辑的原生高亮对象**（进高亮面板、可点选）。**三缺陷**：① 必须先渲染出 PDF（要坐标）② 必须重启感知 ③ 改字号/布局→绝对坐标错位。
- **路 B · EPUB 烤高亮（✅ 优路）**：把云端**高亮+想法+评论**直接烤进 EPUB HTML 正文（`highlights/cloud_index.wrap_highlights_inline`：区间 `background-color` span + 高亮末尾内联 `〔想法：…〕`、评论渲成 `作者：内容`）→ `tools/publish_book.py --inline-highlights`/`--inline-comments` 随书 `/upload` **免重启**导入（MainPID 不变）。相对路 A **三缺陷全解**：① 不用渲染（章节 HTML 就有文本、offset 由组件3 已 0 误差掌握）② `/upload` 免重启 ③ **样式跟着文字重排流动，换布局/字体不错位**（2026-08-17 真机确证：改字号/换字体后画线+想法+评论不丢不错位）。跨标签按文本 run 拆 span 不破 HTML；**划线-only 想法**（微信读书"划线并写想法"未必生成 bookmark 条目）由 `fetch_book_highlights(include_review_only=True)` 补齐不漏。代价：是"印"的只读样式（不进高亮面板、不可点选编辑）。真机验证书：《Tell Me Your Dreams》（3 想法）、《13.67》（想法+评论 `知猪瞎:评论1条`）。已接进统一入口 `weread publish <id> --inline-highlights`/`--inline-comments`（内联改正文→不吃"已发布跳过"、每次重发）。

**取舍**：对"**看**云端高亮/想法/评论"这个目标，**路 B（EPUB 烤）更优**（不依赖渲染坐标、免重启、扛重排）；要"**可编辑的原生标注对象**"才用路 A（GlyphRange）。组件 3b 的"独立笔记"仍是"写想法回云端"的双向通道；3c 解决的是"把云端已有批注就地显示在正文里"。。

> #### 📌 竞品借鉴（rmkit-cn）·"回写"还有一条不碰内存的路：evdev 模拟笔
>
> 来源：`boangs/rmkit`（GPL-3.0）`upload-server-go/internal/handwriting/handwriting.go`、`internal/server/evdev_input.go`。**源码研读结论，未在本项目验证。** `handstrokes.json` 矢量笔画字体血缘 [ghostwriter](https://github.com/rmkit-dev/rmkit)（README 鸣谢）。
>
> 上面把"画回"判为最高危，前提是"往原生阅读器数据结构写内存"。rmkit-cn 的手写 AI 回写给出**另一种回写范式**——完全不碰 xochitl 内存/数据结构：把要写的内容（它那里是 AI 生成的文字）先经 `handstrokes.json` 查成矢量笔画，再逐笔通过 `/dev/input/event2` 写 evdev 事件（`EV_ABS` 坐标/压力 + `BTN_TOOL_PEN` + `EV_SYN`）**模拟一支真笔在屏上画**，由 xochitl 自己按正常笔输入落墨。它按 `/proc/device-tree/model` 区分 Chiappa(Move 960×1696/输入 6760×11960) 与 Ferrari(Paper Pro) 的坐标映射。
>
> **对本项目的意义**：如果 P0 组件3 的"云端高亮画回"改成"把高亮位置模拟成笔的划线动作"，风险模型从"Step S 级内存写"降到"模拟输入事件"（最坏是画歪/画错位，不会崩 xochitl）。**代价**：evdev 注入的是"新笔迹"而非"原生高亮对象"，画上去的是普通笔划、不是可被 xochitl 识别为 highlight 的结构化对象，语义不等价；且坐标映射要精确到字符位置（依赖组件3 的 `.epubindex` 映射，见未决⑥）。**置信度：中**——机制在 rmkit-cn 源码坐实，但"高亮↔笔划"语义差决定它只是"画回"的近似替代，不是等价方案。是否采用取决于本项目要的是"真高亮对象"还是"视觉上标出来即可"。

### 5.2.1 全设备化落地：Rust 移植 + 墨香 app + 阅读器菜单（2026-08，全真机验证）

组件 3/3b/3c 的机制（`.rm` 反解划线、`rmscene` 造笔记页、云端批注双向、EPUB 烤）**已全部逐字节移植成 Rust、搬上设备**（`weread-client/device-rs/`）——host 的 `tools/*.py`/`highlights/*.py` 从"运行时依赖"降为"对拍参照"（下方组件段里的 Python 文件名读作历史实现，现由 Rust 等价件承担）。此外补齐了一批只在设备端才有的能力：

- **扫码登录（设备自足）**：`login.rs`+`qr.rs` 设备自绘二维码，`/login/start` 取 uid、`/login/poll` 长轮询 `getLoginInfo` 到确认落盘 + 续期激活；退出=`/logout` 改名凭证文件。踩坑：`webLoginVid` 是**数字**非字符串；二维码无过期信号→定时 120s 换一张。
- **可手写批注的 `.rm` 笔记页 + 想法 CRUD**：`notebook_rm.rs` 自写**字节对拍 rmscene 的 v6 `.rm` 写入器**（`simple_text_document`，ascii+CJK 逐字节一致；`read_root_text` 读回编辑版一致）→ 把云端划线+想法排成可**手写批注**的原生笔记页 → `.rmdoc` `/upload` 免重启进库。设备上在「想法：」后打字 → 读回 `.rm` → 按 `〔wr cu:s-e〕` 锚 `parse_thoughts` → 增/改/删同步回微信读书（`/web/review/add`、`/web/review/delete`）。
- **本地画线推荧光 + 虚线根因**：读书正文 `.rm` 划线 → `reverse.map_to_weread`（章原文字符定位算 range，`canon` 规整 Kangxi Radicals 码位失配）→ 幂等 `addBookmark`（colorStyle 取笔色）。**关键纠偏**：weread 划线只有 `colorStyle` 无"笔形"字段；"有想法无划线"weread 渲染成**虚线**，故同步时**必须同时补推 bookmark** 才是荧光笔（早先只推 review 出虚线的根因）。"笔形语法"判死（无此维度）。
- **智能同步（一键三合一）**：`sync_book`——① 笔记本想法回传 ② 本地划线推荧光 ③ 拉云端重建笔记本（无改动不重建、省换 UUID）。第一次按=创建。取代早先"拉成笔记本 / 获取画线 / 同步"三个易混按钮。
- **回传划线（Phase B，2026-08-18/19）**：`.rm` 解析用 vendored `remarkable_lines`（补丁 `PenColor::Unknown`）；`reverse`(canon+locate_range+map_to_weread) 真机对拍 Python 一致；weread colorStyle 0黄1红2紫3蓝4绿（设备荧光笔 3黄4绿9蓝）；`autosync` 按 title 认领本地文档、去重、记 mtime。
- **我的笔记只列本地真实存在的**：`/notebook/list` = 云端有笔记 ∪ 设备墨香本地书（按 book_id 去重）；`DocMeta.active()` 排除**回收站**（`parent=="trash" ≠ deleted`——xochitl 回收站是改 metadata `parent`，旧筛选只看 `deleted` 会漏，回收的书误判在库）。
- **脚注跳转根因破案 + 修复（2026-08-20 真机端到端通过，推翻旧结论）**：旧白皮书曾断言"脚注跳转 xochitl 只认小文件同文件 `#` 锚点（跨文件/大文件/弹注全不跳）"——**此结论错误，已纠正**。真机逐层二分锤定：reMarkable 导入 EPUB 时把章内锚点**烘焙成静态索引**，一旦某章存在**"互指对"**（marker→注释、注释又→marker 的**双向脚注**，几乎所有电子书的标准结构），其索引器会把这一对链接**整对丢弃**，导致脚注在设备上连"可点黑块"（rM 给识别为有效链接处打的点击高亮）都没有。普通阅读器实时解析 DOM 无所谓环不环，rM 烘焙期一遇环全灭。与文件大小/跨文件/热区**无关**。诊断关键信号=**有无黑块**（同页跳转看不出移动也能判链接认没认）；上机注入靠 USB Web UI `POST http://10.11.99.1/upload`（multipart，字段 `file`，免重启、秒建 `.epubindex`+`.pdf`）快速迭代。**修法** `htmlproc::break_footnote_cycles`（接在 `epub::assemble` 的 `fix_internal_links` 之后）：找章内 2-环，把"源元素较晚"那条（注释里的回链）**去链化**（`<a>`→`<span>`、删 `href`、**保留 id**——它是正向 marker 的落点；weread 形态 id 与回链同在一个 `<a>` 上，只能变 span 留 id 不能整条 unwrap），正向 `marker→注释` 恢复可点、返回交给 rM 原生"返回第 X 页"条。host 单测覆盖 Kindle(sup/p 分离)+weread(a 自带 id) 两形态；真机重下《喜鹊谋杀案》8 条脚注全跳+返回。**惠及以后新下/重下的书，旧书需重下一次**。（这也顺带解释了当初为何"做独立笔记本 + 阅读器菜单一键发送"而非改阅读器——阅读器内跳转当时被误判判死；现脚注可跳，两条路并存。）
- **书内目录页删除（同批处理）**：书内"目录"页是**跨文件链接**且我们重命名 spine 后 href 还指着旧文件名（`index_split_NNN.html`，成品 EPUB 里不存在）→ 彻底死链；而 rM 原生目录（`nav.xhtml`）已覆盖章节导航，书内目录纯冗余。故 `pipeline::is_toc_title` 按章标题精确匹配（目录/目錄/目次/Contents 等，`目录导读` 类不误判）在下书时跳过该章——注意判据**必须按标题**而非"链接密度"（书中书的正文页可能含大量死链，按密度会误删正文）。真机重下 122→116 章（"目录"章 6 块一并去除），nav 同步不再列它。

> #### ⚠️ QML 注入纪律（整页导航血泪）
>
> 往 `Navigator.qml`/`DocumentView.qml` 等核心 QML 注入前**必走离线 qmllint**：把要 INSERT 的片段单独包成一个 QML（`import QtQuick` + `import QtQuick.Controls 2.15 as MXControls` + dummy 依赖）用 `/usr/lib/qt6/bin/qmllint` 编译，`missing-property`/`no matching signal`/`is not a type` 即致命（`inputMethod.hide` 是 qmllint 存根不全的误报，忽略）。**血泪**：Popup→Item 转换漏剥 `background:`/`contentItem:`/`onClosed:`（Item 无这三个属性）→ **整个 Navigator.qml 编译失败 → 启动崩溃循环 → StartLimitAction 整机重启**（uptime 归零）。部署一律带 `NRestarts` 连续监控（攀升立即删 qmd 回退）。另：`LOCATE` 不接 `?` 通配（要具体类型名，`TRAVERSE` 才可 `?#id`）；xofm 模块的真实资源路径不靠推断、查 qrr `hashtab`（`/home/root/xovi/exthome/qt-resource-rebuilder/hashtab`）——xofm 在 `/qt/qml/xofm/…` 不是 `/qml/xofm/…`；`SidebarItem.iconSource` 不吃 dataURI（rcc 重打）；toast 用 `showNotification` 字段是 `message` 不是 `text`。

### 5.3 借鉴逻辑蓝图（须以真机流量二次确认；现由 Rust 等价实现）

以下是读 MiuRead 得到的机制，作为**理解微信读书协议该去哪抓包**的指路牌——不是照抄对象，落地时一律以自己账号的真机流量二次推导为准：

- **登录**：`/api/auth/getLoginUid` 取 uid → 屏显二维码 `/web/confirm?uid=` 手机扫码 → 轮询 `/api/auth/getLoginInfo`（支持手机 4 位 OTP）→ 领**微信读书官方 Skills/Agent API Key**（`/api/skills/apikeyGet`）作主凭证。
- **下载**：章节走 web 分片端点（正文 `t_0`/`t_1`、EPUB 版式 `e_0..e_3`、`/web/book/chapterInfos` 拿目录），正文是**混淆编码**，需自定义解码还原 HTML 后本地组装 EPUB，付费/受限章节占位。
  - **真机深挖（2026-08-13，两份自有账号 HAR + 借鉴 MiuRead 逻辑 + host 侧重放实验）**：协议层拆得很透，但**撞上一道"200 空响应"墙，自动下载能否走通仍未定**。已攻克并落地（`weread-client/`）：① 请求形状 `POST /web/book/chapter/{shard}` + JSON body（`b`/`c`/`r`/`st`/`ct`/`ps`/`pc`/`sc`/`s`）+ 头 `x-wrpa-0`；② **签名 `s` 复现**（`0x15051505` 的 XOR+移位滚动散列，HAR 6/6 命中，`sign.py`）；③ **obfuscate 编码复现**（`b`=obfuscate(bookId) 逐字节对拍通过，`obfuscate.py`）；④ **codec 完全攻克并落地**（`codec.py`，经所有者授权移植 codec.lua + 真机对拍）：响应 `32hex(md5校验)+1字符+base64`，base64 **文本层**做了少量字符位置置换（positions/unswap）；正文 `e_0`+`e_1`+`e_3` **拼接**后去置换再 base64 解码 = 完整 XHTML，`e_2` 单分片是全书 CSS。对拍：CSS 4/4、正文拼接后 utf8 完整。**→ P0 下载链全线打通（真机端到端验证）**：`download.py` 用真实自造请求从《喜鹊谋杀案》下到正文、codec 解码成可读中文、组装出合法 EPUB（mimetype 正确、含 container/opf/nav/章节）。链路 = 登录 → 目录（Agent Bearer）→ 自造章节请求（obfuscate 编码 + web_sign 签名 + 新鲜时间戳 + **有效登录 cookie**）→ codec 解码 → EPUB。**鉴权只靠 cookie，不需要 x-wrpa-0，不需要 curl_cffi/浏览器指纹（标准库 urllib 即可）。** **"200 空 {}" 卡了很久，真相是没带对 cookie**——那份 HAR 导出未含 HttpOnly cookie，实验一直用空/过期 cookie，{} 就是"未登录"；换 `tools/login.py` 的新鲜 cookie 一发即得正文。**教训串**：x-wrpa-0 认知反复两次全错（"前端签名"→"服务器票据"→实为客户端埋点但服务器不强校验）、指纹/nonce 假设也全错——**先确认最基本的鉴权（cookie）到位，再怀疑高级反爬**。curl_cffi 仅用于排除指纹假设，不进依赖。
- **划线/想法**：读走 `/book/underlines`、`/book/readreviews`；写走 `/web/book/addBookmark`、`/web/review/add` 等。微信读书批注是"字符 range"，与设备侧位置要来回映射。
- **进度/时长**：`/web/book/read` 上传进度（带精确锚点定位）；阅读时长单独上报以计入账号。
- 统一入口 `https://i.weread.qq.com/api/agent/gateway`，`Authorization: Bearer <api_key>`。

### 5.4 净室纪律（"不偷源码只借鉴逻辑"落地成规范）

**诚实前提**：本项目调研中已读过 MiuRead 的 AGPL 源码，纯净室（一拨写规格、另一拨没见过原件照做）的法律姿态已不完全具备。因此采用可操作的最强姿态：

1. **协议事实从微信读书自己的真机流量二次推导**——端点、鉴权流、请求/响应形状以抓包为准，MiuRead 只当"该去哪抓包"的指路牌。这些是腾讯服务器的接口事实，非 MiuRead 的版权物。
2. **解码算法从真实响应重新推导，不照搬 `codec.lua`**——套用 `pinyin-engine/c` 现成的"Python 参照 → C 移植 → 逐行 diff 差分测试"脚手架，实现血缘是"观测到的微信读书行为"。
3. **不复制 MiuRead 的代码结构/命名/注释**；可把 MiuRead 当**黑盒对拍参照**（跑它、比对输出）验证自研实现——这是合法的行为对拍，不涉及抄代码。
4. 由此产出的实现是**自有版权、自选许可证，非 MiuRead 派生作品，不受 AGPL 传染**。KOReader 在本路线完全不涉及，其许可证与本路线无关。
5. 沿用项目许可证纪律：以实测 `LICENSE` 为准；不代替法律意见，只记录事实与架构取舍。

### 5.5 推进历程（"一步一确认"，全部完成、全部上设备）

沿"零设备风险先证协议层 → 只读注入 → 上行同步 → 双向 → 全设备化"逐步推进，每步真机验证再进下一步：

1. **协议层（host 侧）**——✅ 自己账号流量二次推导 + 差分测试跑通登录/下载/解码，MiuRead 作黑盒对拍。
2. **设备端注入 EPUB 原生渲染**——✅ `/upload` 免重启导入、xochitl 原生阅读。
3. **设备→云端划线同步**——✅ 反解 `.rm` + 映射 + push（只读设备文件）。
4. **双向笔记（想法 CRUD）+ 云端批注内联"烤进正文"**——✅（组件 3b/3c；路 B EPUB 烤免重启、扛重排，是"看云端批注"的优路）。
5. **★整条搬上设备 Rust（host 脐带剪断）**——✅ `sign/obfuscate/codec/下载/组 EPUB/注入/扫码登录/双向同步/续期/本地 UI 服务端`全 Rust 静态二进制，设备自足（见 5.2）。
6. **★墨香面板从弹窗→整页 app、阅读器「...」菜单发送画线**——✅ 顶层页导航注入 + 三处协同注入 + toast（见 5.2 ②③、5.2.4）。

> 进度/时长**上报**（原第 4 步）暂缓：读端个人数据（时长/天数/排行）已经能取（`/readdata/detail`，用于首页个人卡），但"往云端**写**阅读进度以计入账号"未做——价值不高、且写端受 cookie 保活与协议脆弱性约束，需要时再补。

### 5.6 为什么值得，及未决

- **独家 + 原生**：社区无人在 reMarkable 上碰微信读书，且这是**用原生笔在原生阅读器里读**的完整闭环，不是又一个割裂的第三方 app。
- **与阅读 QoL 叠加**：P0 注入的 EPUB 走 xochitl 原生阅读器渲染，9.1 节立项的点击翻页 / 快速黑白同样作用于原生阅读器——两者天然叠加，P0 主链打通后可顺带受益（点击翻页尤其能缓解 Move 小屏滑动翻页的别扭，翻页卡顿的硬件天花板则见本节未决②）。
- **站在现成肩膀上**：hook 韧性机制（特征码自定位 + trampoline）、`.rm` 划线反解、差分测试脚手架都现成可复用（见第 10 节）；真正从零硬啃的是协议/解码层 + 文档库注入这道集成。
- **进度（2026-08-13 host 首版闭环；2026-08-18 起整条已 Rust 移植上设备）**：**P0 全链路打通**——host 首版由 `tools/publish_book.py` 一键串起：book/info 取书名/作者/出版社/封面 → 下载正文 → codec 解码 → `epub.assemble`（封面+元数据）→ `inject.py` 生成四件套 → 推送 → 设备原生阅读；**此管线现已逐字节移植成设备端 Rust（`wr-download`/`wr-serve`），host 脐带剪断**（下述 `tools/`/`download.py` 等读作历史实现，真书组装踩坑结论仍适用于 EPUB 组装本身）。真机跑通《喜鹊谋杀案》。**真书踩坑（手写测试片段全没暴露，真书才触发，均已修在 `download.py`）**：① codec 解码是完整 XHTML 文档→取 `<body>` 内层（否则嵌套 body 空白）；② calibre 宽松 HTML 的 void 元素 `<br>`/`<hr>`/`<img>` 非自闭→XHTML 化（否则严格 XHTML mismatched tag、整章翻不动）；③ 每章一个大 xhtml→xochitl 翻页极卡→`split_blocks` 按块切 ≤1800 字符小片（原生书拆 55 小 part 不卡）；④ codec 间歇解码失败→同章各分片共用一个时间戳（真实浏览器 ct/pc/ps 同、仅 r 变）+ 重试 + 容错跳过。字体走 `.content` 的 `fontName`（见 08 节）。**第 1 步（协议层）+ 第 2 步（注入原生阅读）+ 字体优化 均真机验证通过。**
- **未决**（**状态盘点**：① 注入、④ 免重启、⑤ 凭证续期、⑥ offset 映射 **均已解决**，见各项内 ✅；当前真实剩余风险只有 **② 翻页硬件天花板** 与 **③ 协议脆弱性**，两者都非软件可根治、只能监控/回归）：① **第 2 步——设备端注入 EPUB 到 xochitl 原生阅读，✅ 已真机验证通过（2026-08-13 host 首版，现由设备端 Rust `wr-download`+`/upload` 承担）**：host 生成测试 EPUB + **最小 `.content`（`fileType:"epub"`+`formatVersion:2`+`documentMetadata`+渲染参数，`pageCount:0`、无 cPages）** + `.metadata` + `.local`，scp 推送 4 文件（md5 核对）→ 重启 xochitl 重扫 → 书出现在书库、点开 → **xochitl_pdf_renderer 渲染成功**（自动生成 `.pdf`/`.epubindex`/`.thumbnails`，并回写 `.content` 把 `pageCount` 0→4、补全 cPages）。**结论**：**最小 `.content` 即可，cPages/pageCount 靠 xochitl 首开渲染补全**；**感知机制 = 重启 xochitl 重扫**（硬证 xochitl inotify 不监视文档目录，不会自动感知）；`source` 自定义标识被接受。小瑕疵：无封面的 EPUB 日志报 null cover（真实书加封面即可）。**剩余工程化**：把这套注入折进守护进程/`fetch_book.py`（全书下载→组装→推送→触发重扫）、EPUB 加封面（已在 `publish_book` 做）、`e_2` CSS 与图片资源并入；② **翻页卡顿 = 设备硬天花板（已认，非软件可解）**：原生书也卡→Gallery 3 彩色墨水屏物理刷新慢（~1s）+ 官方无 speed mode（评测证实）+ EPD 在独立 T2000 TCON、无 `/dev/fb` 接口、刷新控制在 xochitl 私有 EPD 代码；改波形=本文 4.1 否决（残影/烧屏永久损伤、无回退、rMPP 无先例）。应用层杠杆（切小片）已用尽，改不了物理刷新下限。**附·章内空白与切片的权衡（2026-08-17）**：xochitl 把每个 spine 文件当独立 reflow 单元、**文件边界强制改页**，故我们按 `max_chars` 切出的每个 part 末尾都会留一次"该页剩余=空白"；切片越大→空白位置越少但单文件 reflow 越重、翻页越卡（放大到 3600 时 Chapter One 12→7 片、空白减半但换来卡顿）。真机权衡后**切片保持 1800、翻页顺滑优先，接受结构性空白**（结构性空白只能减、不能归零）。calibre 自带的 `mbppagebreak` 显式改页 + 连续 `<br>` 已在 `download.body_inner` 清掉（纯赚、与卡顿无关，消掉扉页/短章的空页）；③ **协议脆弱性**：整条链依赖微信读书 web 端点/编码/签名不变，上游一改需真机样本回归；`codec` e_0/e_1/e_3 拼接对个别章有间歇失败（已加重试+容错，根因仍需更多样本收敛）；④ **免重启注入（✅ 已解决，2026-08-16）**：走 USB Web UI `POST http://<host>/upload`（xochitl 内嵌 qtwebapp）当场导入 **EPUB 与 `.rmdoc`**，MainPID 不变、NRestarts 不增、**免重启**（B3 + 组件 3c 路 B 真机验证）。`publish --upload` / `notebook --upload` / 内联印书都走它，纯 urllib multipart 不依赖 curl。代价：`.content` 由 xochitl 生成，字体走书内菜单选（不像 scp 文件注入能预置 `fontName`）；且云同步会 churn 掉 `/upload` 的文档 UUID，故按 `visibleName` 认领而非记死 UUID。D-Bus `documentFinished` 信号那条不再需要；rmkit-cn 的管道 broker（进程内调导入）仍是备选、未用。**📌 竞品借鉴（rmkit-cn，源码研读未验证）**：`boangs/rmkit` 用一条不同的免重启路线——一个 `librarian.so` xovi 扩展 + `xovi-message-broker.so`（两者均预编译在其 `vendor/extensions/`，**源码未开放**），host 侧经命名管道 `/run/xovi-mb`(+`-out`) 发 `>eimportDocument:<路径>\n`，broker 在 xochitl 进程内当场入库并回一个 UUID（`upload-server-go/internal/librarian/librarian.go` 是调用端，记了两个真实坑：必须先 open OUT reader 再 write IN 否则应答错位一格；读到 EOF 后不要 reopen 否则打断正在执行的命令）。这证明"进程内扩展直接调 xochitl 导入 API"比 D-Bus 信号更直接可行，是本未决项的第二条候选；但因 broker/librarian 闭源，本项目只能**借协议思路**（xovi 扩展常驻 + 管道 broker + 进程内调导入），实现得自己写；⑤ 凭证（✅ 过期已根治，2026-08-16）：**正文只能走 web 端点 + cookie(`wr_skey`)**（Agent 网关 Bearer `api_key` 17 接口无正文），而 `wr_skey` 实测 **~5400s(90min) 过期**（早先误记"长期有效"）→ 复刻 MiuRead 的 `POST /web/login/renewal`（body `{"rq":"%2Fweb%2Fbook%2Fread","ql":false}`）**保活续期**，已接进 `auth.finish`（登录即激活）+ `_wr_common.load_client`（**一处保活**，`renew=True` 默认，所有 weread 工具每次加载即续期），做到"扫一次码用很久"；`session.py` 的 `cookies_export()` 存完整 expires。设备端凭证已落地（`credentials.json` + `wr-renew.timer` 定时续期，全设备自足）；⑥ **字符 offset → 微信读书 range 映射 ✅ 已绕开 `.epubindex` 逆向**——关键突破是"offset 坐标空间就是我们下载的章节原始解码全文"（`fetch_chapter_document`），在其中字符串定位算 range 即 0 误差对拍，无需逆向 `.epubindex` 二进制（见组件 3、第 07 节）。**下载+注入+字体+双向同步+整页 app 均真机验证；当前真实剩余风险=② 翻页硬件天花板（非缺陷）+ ③ 协议脆弱性（上游一改需真机回归）。**

### 5.7 通用 EPUB 优化器 + 阅读体验做到极致（2026-08-21/22，全真机验证）

定位复位为 PKM 工作台后（砍回传/想法、微读只留下书+进度），把"阅读"做到极致成为主线。四块落地：

- **A. 下书 EPUB 组装升级（微信读书书）**：① **整章一页**——不再按 1800 字硬切成多 xhtml（那切断段落/脚注锚点、只首片带标题、目录塌陷），交 xochitl 自己 reflow 分页；② **多级目录**——`Chapter` 加 `level` 字段，nav 生成部/章嵌套 `<ol>`；③ **脚注内联（两套机制）**——导入版（`CB_` 书）脚注跨文件汇总在某"部"末章（**按注释 id 反查定位注释章**，实测 part 编号 = uid−2），数字正版脚注是 `<img class="qqreader-footnote" alt="注释全文">`（内容就在 alt）。统一内联成**朴素同章锚点**（marker `<a href="#id">`、注释聚章末 `<div class="footnotes">`）；数字版自造 id 用**全局递增**防跨章撞车（reMarkable 把 EPUB 拍平成单文档、锚点是全书空间，每章从 1 重编会撞车跳错章）。

- **B. xochitl 弹窗脚注判死（穷尽实测，重要负结论）**：造对照书真机点验，`epub:type="noteref"`/`aside="footnote"`、ARIA `role="doc-noteref"`/`doc-footnote"`、双语义、`<sup>` 包裹**全部只当普通跳转**——xochitl 闭源渲染器**不实现任何弹窗脚注**；正文点击也**不通知可注入的 QML 层**（外链/scheme/data URI 根本不激活）。**可点性铁律**：只有朴素同章锚点 `<a href="#id">文字</a>` 可点；**图片链接 `<a><img></a>` 不可点、文字链接才可点**；`<sup>` 内文字热区太小点不中、要脱离 sup 独立成块；**注释区必须在 `</body>` 之内**否则 marker 死链。竞品「**镇纸**」的弹窗真相 = 设备跑**浏览器/WebView 加载微信读书网页版**（弹窗/字体/社区全是网页自带），绕开了 xochitl 阅读器；KOReader 能装 Move（MobileRead 有帖，推翻旧"显示路堵"判断）自带弹窗/字体自由——但都属"换阅读器"，本项目定为**继续 xochitl 务实路线**。

- **C. 通用 EPUB 优化器（`device-rs/src/optimize.rs`，对任意结构第三方 EPUB）**：解包 → 只就地改每个 (x)html（**保结构不重组**）→ 重打包 → 新导入替换。① **字体/字号解锁**：剥内联 `style` 里 `font-family`/`font-size`（第三方书硬写死覆盖 xochitl 设置致"改不动字体/字号"；值含 HTML 实体 `&#39;` 要跳实体匹配否则截断）；② **脚注保留原样式**：`preserve_relink_footnotes` **保留脚标原图标**（去 `epub:type`、href 同章）+ 补一个**独立可点 `[N]` 文字角标**（图片脚标点不了，图标留 `<sup>` 内、`[N]` 移到 sup 外正常大小）+ 注释移章末。**四个上机才暴露的坑**：**(a)** zip 依赖必须开 `deflate` feature（第三方 EPUB 多 Deflated 压缩，否则 `Compression method not supported`）；**(b)** 读条目失败必须**报错中止**（静默跳过会产出残缺 epub 破坏原书，踩过 8MB→134 字节）；**(c)** 注释区必须插 `</body>` **之内**（加到完整 xhtml 文件末尾=落在 `</body></html>` 外=无效 HTML，marker 死链点不动，这是"角标点不了"真凶）；**(d)** **就地替换（`update_epub_inplace`）对第三方书判死**（xochitl 缓存旧渲染，覆盖 epub + 删派生件后重开**还是旧内容**，《喜鹊谋杀案》坐实；墨香自家书可就地重渲、第三方书不行）→ 只能**完整导入（`/upload` 当新书）**。⚠ `/upload` 的书名取 **EPUB 内 `dc:title`**、不是上传文件名——优化版与原书必然同名，区分靠内埋标记。后端 `/optimize`+`/library` 端点就绪。

- **D. 无感自动优化 + 原生回收站通道（2026-08-22，《喜鹊谋杀案》三项全对 + 端到端真机闭环）**：
  - **优化器补三能力**（对任意 calibre/第三方书，喜鹊上逐项真机验证）：① **破脚注互指**——calibre filepos 脚注是**同文件双向互指**（`<sup><small id=X><a href="#Y">[N]</a></small></sup>` ↔ 注释处回链），正中 rM"互指对整对丢弃"雷 → `break_footnote_cycles` 进优化器（喜鹊 `href#filepos` 310→155 正好减半），真机**脚注能跳+能返回**；② **封面拉伸修复**——calibre `titlepage.xhtml` 是 SVG `preserveAspectRatio="none"`，**xochitl 改 aspect 属性不吃**（真机复验仍放大），`svg_cover_to_img` 把 SVG 整体换 `<img max-width:100%>` 才修好；③ **去冗余目录页**——判据 = 指向 ≥10 个**不同 html 文件**的页（正文脚注是同文件 `#frag`，天然区分），`remove_toc_from_spine` 只删 spine `itemref`、manifest 保留不产悬空（喜鹊 93→91，开头+书末两个目录页都去掉）。**幂等标记** = 内埋 `META-INF/com.cangjie.optimized`（版本号，跟书走、云同步/换设备不丢，比记 uuid/书名后缀都鲁棒）。**deflate 坑**：重打包纯 STORED 文本不压缩体积近翻倍（621K→1.2M），mimetype STORED、其余 Deflated。
  - **判死：外部进程直接写 `.metadata`**——归档原书把 `parent→"trash"` 后被**运行中 xochitl 的内存文档模型覆写回 `""`**（首页两本赎罪坐实）。逆向 UI Delete 按钮（`qml_00dd24e1` trashActionButton）找到正解：`EntitySelection.selectionMoveToTrash()`（C++ 进程内方法），**QML 单例路径可达** `NavigationManager.activeContext.selection`（来自 `import xofm.libs.explorer`，Sidebar.qml 本就 import）；回收站 Restore = `selectionRestoreTrashed()`。**软删真相：reMarkable 软删 = `parent:"trash"`，没有 `deleted` 字段**。
  - **`trash-agent.qmd`**（注入 Sidebar）：读 wr-serve `GET /trash/pending` → 对每个 uuid `selection.add()` + `selectionMoveToTrash()`——**与用户手点 Delete 同一条原生代码路**（模型/持久化/云同步都由 xochitl 自己做），真机通、不被覆写。队列 `pending-trash.json` 自清洁（见 parent=trash 即出队、QML 端无需 ack）。**触发方式后来从 Timer 15s 轮询演进为纯事件驱动**（挂文档模型 `explorer.entityListModel` 的 `onRowsInserted`/`onModelReset`+4s 防抖，新卡进库即触发、休眠零开销）——详见 §5.8 的踩坑翻案（`elementCount` 属性绑定不触发、`onRowsInserted` 才是 xochitl 自用的可靠信号）。**扫描跳过排队中的书**——堵死"归档未生效就再优化一轮"的重复产书竞态。
  - **无感终形态**：设置页「系统增强」第 5 开关「**导入书籍自动优化**」（**默认关**；写 `reading-qol.json` 的 `autoOptimize`，wr-serve 每轮扫描前现读现判，改开关即时生效无需重启；`CANGJIE_AUTO_OPTIMIZE=0` 是紧急总闸）。开后：导入 EPUB → 60s 内后台优化（**只动 `lastOpened=="0"` 的未读书**——完整导入产新书丢进度，绝不碰在读的书）→ 完整导入优化版 → 原书入队 → trash-agent 原生归档 → **首页只留优化版，全程零操作**。原书 epub 另备份 `cangjie-backups/<uuid>.epub.pre-optimize`。已知局限：CSS 文件内的字体锁未剥（现只剥内联 style；喜鹊字号是 em 相对不受影响，遇到 CSS px 锁死的书再扩展）。

### 5.8 ★ 全局待办 —— PKM 语义引擎首个能力（2026-08-22，全真机端到端）

PKM 回归后 reMarkable 复位为**阅读/笔记工作台**，`pkm-semantic/` 是这条线的"语义引擎"：只读扫 `.rm` 笔迹 → 识别约定符号 → 输出独立文档（绝不回写原件）。首个能力选 **★ 全局待办**（灵感来自 `2.md`「后台解析矢量笔迹」构想）：**阅读时用红笔在某页画一颗五角星，几秒后后台 Rust daemon 自动生成/更新这本书的「《书名》- 总结卡片」笔记本**。选它打头因为①不依赖底层文本 ②颜色/形状能从 `.rm` 直接读出 ③输出独立文件天然绕开 inplace 判死 + 云同步冲突。产物：`pkm-semantic/proto/`（Python 原型 + 差分测试）→ `device-rs/src/{stardetect,cardsync,cardnote,epubindex}.rs`（生产 Rust）+ `bin/wr_stars_daemon.rs` + `device/trash-agent.qmd` + 设置页开关。

![★ 全局待办数据流（画星 → fswatch → 识别 + 页→章名 → 卡片 merge → /upload → 事件驱动去重）](../weread-client/docs/star-todo-flow.svg)

**A. 检测引擎（自相交 + 颜色门控，先 Python 后 Rust 逐字节对拍）**。真机对账《缺失功能》笔记（含红手绘星 + 红干扰 + 黑笔记）后三大发现改写了设计：① **真手绘星 ≠ 理想五角星**——是多笔叠加、外廓圆钝、内部自相交的松散手势，原"5 尖角 + 闭合"阈值全漏 → 改判据为**自相交计数**（pentagram 不变量：clean 恒 5、真机松散星 10~20、圆/方框/对勾/正常字母 = 0）+ 空间合并多笔星（并查集 bbox 聚类）；② **黑对黑纯几何判死**——满页黑手写里草书汉字也有 4~11 自相交，每页约 8 假阳 → 坐实必须**颜色门控**；③ **颜色 + 形状缺一不可**——红笔里也有干扰（红对勾/红方框），双条件下 RED 门控检出星、忽略干扰。用户拍板**用一种记笔记不用的笔色（红）画星**。**踩坑真 bug**：`PenColor` 是 IntEnum，Py3.11+ 的 `str()` 返数字 `"7"` 非 `"RED"` → 颜色归一必须走 `.name`。Rust 移植与 Python `star_scan` 逐字节全等（200 笔逐字段 + 各色门控），vendored `remarkable_lines` 打两处补丁容忍新固件格式（`ParagraphStyle::Unknown`、块少读跳块尾）。

- **页 → 章名映射**（`epubindex.rs`，设备任何 EPUB 通用，不限墨香书）：逆向 xochitl 的 `<uuid>.epubindex` 二进制格式 = 头 `"rM epub index"` + 若干长度前缀 UTF-16BE 路径条目，每条后 3 个大端 u32，**起始页 = (中间 int==0 ? 第一 int : 第三 int)**（两内部表一致）→ `page_section` 找起始页 ≤ 页号的最后一条 spine 文件；再读 `.epub`(zip) 的 `nav.xhtml`/`toc.ncx` 解 `spine 文件 → 章名`。真机《赎罪》验：第 8 页 → 洋娃娃、第 74 页 → 狗熊兄妹。**EPUB 页号**：`stardetect` 的 `page_order` 有 fallback 读顶层 `pages`（EPUB 的 `cPages` 为空、页 uuid 在顶层 `pages` 列表），0-based 与 `.epubindex` 对齐。

**B. 注入机制真机命门（挖出三条硬事实，纠正了 §5.7 时期的错误归因）**。做"给已有笔记本追加一页"验证时连挖：① **xochitl 对文档目录零 inotify 监听、维护全内存文档模型**——运行时完全无视磁盘直写，连它自己 native 建的笔记本、直写追加一页重开仍只见旧页；② **`/upload`（`10.11.99.1:80`，本机走本地路由可达、无需真插 USB）是活注入唯一路 → 但永远"新建文档"且强制重新分配 uuid**（指定 uuid 被忽略，连传已存在的 uuid 也另生副本），无任何原地更新路；③ **重启 xochitl 能让直写现身但打断阅读**。→ **纠正**：§5.7 说的"直写文档免重启即显"实为部署时 `systemctl restart xochitl` 顺带重读目录，归因错了；纯运行时直写更新从未真正免重启可见。**旧墨香笔记同步的"追加"真相**（读 `sync_book` 坐实）：从不真追加 = 每次从 weread 云端重建整本 + trash 所有旧同名本 + `/upload` 新本；用户输入不丢是靠想法回传 weread 云再拉下来重建 = **云端往返**。★ 卡片是纯本地笔迹无云可往返 → "重建整本必抹手写"，故 **"后台自动 + 活注入 + 保住本地手写批注"三者不可兼得**。

**C. B 模型卡片设计（打字卡片 + 全自动 merge 重传，用户拍板）**。破局分叉：卡片批注用 **Text 工具打字**（非手写）——真机复验当前固件 Text 打字能被 `notebook_rm::read_root_text` 读回，于是可做"本地版云端往返"：daemon 每次画星后 **读回卡片现有打字 → `parse_card` 拆出每页批注 → `render_card` 按当前星重生成（老星保留用户编辑、新星注入 `2.md` 模板）→ `pack_rmdoc` → `/upload` 新本 + 旧本入 pending-trash 队列**。卡片形态：**一星一页**（一主题一卡），每页 `★ 章名·第 N 页` + `[ID: 章名-pN]`（跨页软链接检索）+ `2.md`《13.67》结构（线索框 🔵🔴🟢 / 逻辑推演网 / 标签锚点 🔖🟡🔗）；打字批注跨重建逐字保留（`cardsync.rs` 纯逻辑 + host 单测）。**爆炸 bug 根治**：去重跳过条件原为 `existing.len()==1`，trash 没删掉时 len 恒 >1 → 每次 fswatch（**开书也触发**）都重生成 → 卡片无限增殖；改成 **"最新卡内容 == 新内容就绝不上传"**（无论几张），多余的只入队列 → 开书不再重复生成、每书恒 1 张。

**D. 纯事件驱动去重（trash 的曲折，最终挂 `onRowsInserted`）**。旧卡删除只能走原生 `selectionMoveToTrash`（外部改磁盘 `parent=trash` xochitl 内存不认、界面残留旧卡，且让 wr-serve 误判提前出队）。触发方式踩了两次坑才对：**× `ViewManager.activeViewChanged` / `entityListModel.elementCount` 属性绑定——真机不触发**（`elementCount` 非 NOTIFY 属性，绑定永不刷新）；**√ `explorer.entityListModel` 的 `onRowsInserted`/`onModelReset`**——这是 QAbstractItemModel 的真信号、**xochitl 自己的文档网格就在用**（离线提取 `qml_00dabb43`/`qml_00dae885` 确证 `Connections{target:model; function onRowsInserted(){}}`）。最终 `trash-agent.qmd`：挂 `onRowsInserted`（新卡 `/upload` 进库）+ `onModelReset`（回书库重载）→ 4s 防抖（覆盖 upload→queue 写入间隙）→ `selection.add(uuid)+selectionMoveToTrash()`，`size>0`（用户手动选中）守卫防误删。**头 less 验证两次通过**（/upload 垃圾 A 入队列 → /upload 垃圾 B 触发 rowsInserted → 4s 后 A 被 xochitl 自写 parent=trash 出队），用户真机确认"2 秒内消失"。**教训翻案**：xochitl 确有可靠信号给注入 QML——**找信号要看 xochitl 自己 QML 怎么连，别猜属性绑定**。

**E. 部署形态 + 设置开关**。daemon 事件驱动省电：`fswatch.rs`（inotify + 防抖，空闲阻塞睡死、零周期唤醒）监视文档目录，`wr-stars.service` 装 `/usr`（硬 `CPUQuota=30%` + `MemoryMax=64M` + `Nice=10` + 开机自启）。**同一改法把 wr-serve 原来 60s 自动优化轮询也改成事件驱动**（同病同治）。配置 `reading-qol.json`：`starTodoEnabled`（默认关）/`starTodoColor`（RED）/`starTodoGap`（25）。**设置页开关**：「系统增强」门户加第 4 分类「**笔记增强**」→ 二级页 `★ 全局待办` 开关（`SettingsCheckBoxItem` + file XHR 读写 `reading-qol.json`）；**全量防覆盖铁律**——每个写 `reading-qol.json` 的二级页都必须读写全量键（翻页页/书籍页原本只写 7 键，加 starTodo 三键，否则切翻页设置会抹掉 `starTodoEnabled` → daemon 读成 false 功能被意外关）。离线 `qmldiff apply-diffs` + host `qmllint` 验过（`1 diff applied`、零语法错误）再上机。**放置决策**：用户否掉"墨香面板"（那面板只有下载选项、无设置区），定「设置页·笔记增强」。。

**最终工作流**：红笔画五角星 → daemon（8s 防抖）重扫 → 内容变才 `/upload` 新卡 → 新卡进库触发 `onRowsInserted` → **2 秒内旧卡经原生 `selectionMoveToTrash` 无形消失**。全设备自足、纯事件驱动、每书恒 1 张、开书不重生成、打字批注永久保留。

## 06｜P1：手写 OCR → Markdown → Obsidian 数据流转

**做什么**：设备上的手写中文笔记，导出到 host/手机侧做 OCR 识别，输出 Markdown 进入 Obsidian 库（或任意笔记生态），让手写笔记可检索、可引用。

**为什么是 P1**：生态盘点确认的最大差异化空白；也是 Supernote"实时识别"卖点的**流转版**替代——不追求设备上实时（那需要设备端算力和深度 hook，风险高），追求"写完之后笔记不死在设备里"。

**架构上的关键取舍**（立项时的第一个决定）：

| 方案                       | 优势                                                        | 劣势                                                  |
| -------------------------- | ----------------------------------------------------------- | ----------------------------------------------------- |
| OCR 在 host 侧（推荐起步） | 零设备风险；可用大模型/成熟 OCR，中文识别质量高；管线纯离机 | 需要设备连 host 才更新                                |
| OCR 在设备侧               | 离线即时                                                    | 设备算力弱、需部署推理运行时、hook 面大、OTA 受灾面大 |

**依赖与顺序**：笔迹提取走 rmscene 反解 `.rm`（拿到笔画坐标序列）——这套文件反解基础设施是 P2 的产物（原"闭环3"成果，见第 07 节），所以 P1 与 P2 共享它、排在 P2 一起推进。

**劣势/风险**：管线长（提取→渲染→识别→版面还原→输出），中文手写识别质量决定体验上限，需先做小样本识别质量验证再决定投入深度。置信度：方向价值高；实现体验能否达标为中，取决于识别质量验证结果。

### 📌 竞品借鉴（rmkit-cn）·手写识别的一条捷径：截屏喂多模态 vision，绕开 rmscene 反解坐标

> 来源：`boangs/rmkit`（GPL-3.0）`upload-server-go/internal/server/ai_glyph.go`。**源码研读结论，未在本项目验证。**

本节 P1 的默认管线是"rmscene 反解 `.rm` 拿笔画坐标序列 → 渲染 → 识别"。rmkit-cn 的手写 AI 走了一条**明显更短**的路，值得作为 P1 识别环节的备选评估：**截屏 → 按选区裁剪 → base64 PNG → OpenAI vision 格式（`messages[].content` 里 `image_url` + `text`）流式识别**。截屏它用两级：优先 proc/mem 方案，失败降级 DRM ioctl（`drmScreenshot`）。它甚至不做纯 OCR，而是直接让多模态模型"先在心里识别成文本、再按指令处理"，一步出结果。

**两条路的取舍**（客观对比）：

|            | 截屏→vision（rmkit-cn）                                        | rmscene 反解坐标（本项目 P1 原计划）                 |
| ---------- | -------------------------------------------------------------- | ---------------------------------------------------- |
| 实现复杂度 | 低——截屏+裁剪+一个 vision 请求，不碰 `.rm` 格式                | 高——反解 v6 格式、笔画序列、版面还原                 |
| 识别质量   | 吃多模态模型能力，中文手写可直接识别；受截图分辨率/UI 遮罩干扰 | 有矢量笔画，理论上信息更全，但要自己接 OCR           |
| 依赖       | **强依赖云端 vision API + 联网 + 凭证**                        | 可 host 侧离线跑成熟 OCR（本节推荐的零设备风险路线） |
| 结构化输出 | 弱——模型出的是文本，位置/版面信息丢失                          | 强——保留笔画坐标，利于 P2 的锚点混排导出             |

**结论**：对 P1"手写→可检索文字"的**纯识别**目标，截屏→vision 是一条低成本捷径，适合先做识别质量摸底（小样本验证阶段直接可用）；但对 P2"混排笔记按锚点结构化导出"，它丢失位置信息、且把离线管线绑成联网，与本项目"host 侧、数据流转"定位有张力。**建议**：P1 小样本质量验证阶段可两条都试（vision 截屏 vs 反解+OCR），按中文手写实际识别率定主路线；P2 的结构化导出仍走 rmscene 反解。置信度：中（机制坐实，识别率对比需真机小样本实测）。

### 📌 竞品借鉴（rmkit-cn）·`.rm` 文件不实时刷新——本项目读 `.rm` 反解同样会踩

> 来源：`boangs/rmkit`（GPL-3.0）`upload-server-go/internal/server/ai_page.go` 头部注释。**源码研读结论。**

rmkit-cn 的文本 AI 原本在服务器端扫 `.rm` 拼整页文字，后来**改掉了**，注释写明原因：**`.rm` 不实时刷新，用户点 AI 要等几秒**；改成客户端从剪贴板拿当前选中文字自己拼 prompt，才即时。这条对本项目是直接警示——**P0 组件3 与 P2 都依赖读 `.rm` 反解**（高亮/笔画），若期望"用户刚划完线立刻就能同步/导出"，会撞上同样的落盘延迟：xochitl 不会在用户每次操作后立即把 `.rm` 写盘。落地时要么接受"隔几秒/切页后才有数据"，要么找一个 xochitl 主动 flush 的时机（如切文档、退出阅读）触发。置信度：高（注释明确，且与本项目已知"xochitl inotify 不监视文档目录"这一硬证据相互印证）。

## 07｜P2：手写笔记的反解与导出（含手写-文字 anchor）

**做什么**：用 rmscene 反解设备上的 `.rm` 笔记文件，把手写笔记结构化导出。核心抓手是真机反解已验证的事实——reMarkable 原生 notebook 已把手写笔迹 anchor 到 typed text 的字符位置（`anchor_id` + `origin_x`）——据此把"打字正文 + 手写批注"的混排笔记，按锚点位置合并导出成一份结构有序的 Markdown（手写部分经 P1 的 OCR 管线转文字，或以内嵌图片形式按位插入）。

**这里承接原"闭环3"的反解成果**：此前为微信读书方案做的 rmscene 反解探针不随 P0 改向而废弃，其文件反解能力正是 P2 的地基。但要**区分"探针结论"与"落地代码"**——

- **已落地**：`rm-export/export.py` 的 `page_highlights()` 能读一页 `.rm` 的 `GlyphRange`，取出高亮文本 + 字符 offset + 颜色，并把相邻段拼回整句。文档库遍历骨架（`glob *.metadata` → uuid → `<uuid>/*.rm`）也现成。
- **~~只是探针结论、尚无落地代码~~ → 组件3 已绕开、无需 `.epubindex`（2026-08-14）**：原以为 P0 组件3 要"渲染页字符 offset → `.epubindex` → EPUB 位置"这段二进制逆向。真机对拍（自有账号 underlines）发现：**微信读书 range 的 offset 坐标空间 = 我们下载的章节"原始解码全文"里的字符位置**（`download.fetch_chapter_document`，body_inner 之前含 `<?xml…>` 前缀），差=0。既然我们下载的就是 weread 章节内容，设备高亮的 `GlyphRange.text` 直接在这份全文里字符串定位即得 weread offset，**`.epubindex` 完全用不上**。组件3 已真机闭环。（P2 手写笔记导出若仍需"笔迹坐标→字符位"另说，但那是坐标问题、不是本 offset 映射。）

**为什么值得**：手写-文字联动的**输入侧**（写字变文字）已有实现，**导出侧**（混排笔记完整结构化离开设备）无人做。这是"设备上创作 → 生态里沉淀"闭环的最后一环。全程 host 侧、只读设备文件、设备侧零 hook，OTA 受灾面≈0。

**已知边界与未决**：原生 EPUB 阅读页只能手写批注、不能打字——所以 anchor 混排导出只适用于 notebook 场景。批注（note）尚未在反解数据中定位（目前只通了高亮）；`.rm` 格式无文档，固件升级可能引入新 block 类型需跟进。

**依赖**：P1 的 OCR 管线吃 P2 的反解基础设施。若 OCR 质量不达标，P2 可降级为"手写区块按锚点位置以图片嵌入"，仍有独立价值。

## 08｜P3：中文字体渲染优化

**做什么**：xochitl 自带西文字体无 CJK 字形，中文走 fallback 渲染，小字号下笔画粗细与排版未为汉字调校。替换/补充一套为墨水屏调校的中文字体（如思源黑体加重字重版、霞鹜文楷），提升中文"观感清晰度"。

**为什么排 P3**：这是"显示清晰度"诉求里唯一软件可达的子集。文件级改动（中文化线已有字体安装管线，`rmfw/fonts/`），风险低、独立性强，适合作为 P0–P2 大项目间隙的小胜。注意 OFL 等字体许可证照旧实测确认，不凭印象。

**实现认知（2026-08-13 真机验证，随 P0 注入书顺带做通）**：**reMarkable 有两套字体路径，控制点不同**——① **UI 界面**由 xochitl 主进程（Qt）渲染，走 `~/.config/fontconfig/fonts.conf`（汉化线已配 HarmonyOS Sans SC/TC）；② **文档正文**由**独立进程 `xochitl_pdf_renderer`** 渲染，它用**具体字体名**（西文默认 `EB Garamond`，CJK 靠 fallback 补），**不吃泛 `serif`/`sans-serif` 的 fontconfig 别名**。血泪教训：改全局 fontconfig `serif→某字体` 对文档正文**无效**（pdffonts 实测正文没变），只会误伤 UI 中文。**文档正文字体的正确控制点 = 文档 `.content` 的 `fontName` 字段**——注入时设 `"fontName":"LXGW WenKai"`，pdffonts 实测正文简繁英**全部霞鹜文楷**（英文也走楷体拉丁）、UI 不受影响。**落地方案**：设备装霞鹜文楷（OFL，单字体含简繁+拉丁）到 `/home/root/.local/share/fonts/`（持久、非 overlay），注入模块给 `.content` 默认 `fontName="LXGW WenKai"`；装字体纳入 install.sh（OTA/重置后重装）。**已随 P0 注入实验一并真机验证通过。**

**置信度**：可行性高（字体管线已验证）；观感提升幅度中——需真机 A/B 对比几款字体后定夺，不预设结论。

## 09｜P4：笔记 QoL——先用社区现成扩展

**原则：不重造轮子。** 先安装社区现成 xovi 扩展验证价值：

- [rm-hacks-xovi-qmd](https://github.com/Samarkin/rm-hacks-xovi-qmd)：双笔切换手势、Move 小屏适配（隐藏关闭按钮）等。
- [rmitchellscott/xovi-qmd-extensions](https://github.com/rmitchellscott/xovi-qmd-extensions)：可折叠目录等。

用真实使用一段时间后，只对"确实高频且三方都没有"的缺口考虑自研。注意：第三方扩展与本项目 `cangjie-langhook.so` 共存时，遵守既有纪律——一步一确认、独立部署验证、qt-resource-rebuilder 的 qmd 补丁逐个启用。

### 9.1 阅读体验增强（点击翻页 + 快速黑白 + 清残影 + 字体，设置页「系统增强」面板）—— ✅ 已真机端到端验证（2026-08-14 首版；2026-08-17 接进设置页面板 + 清残影独立/字体增强）

> 缘起：竞品 `pretenderlu/rmtool`（GPL-3.0）的 `tap-page-turn` 与 `fast-mono-reading`。用户明确这两个功能对阅读体验很重要（前提是不损伤硬件——已确认无风险，见 4.1 修正块）。这是从 4.1"显示优化"里拆出、独立立项的一项。

**做什么**：给 xochitl 原生阅读器加两个 QML 层增强，**都不碰硬件、不改波形、不进 `xochitl_pdf_renderer`**，纯 QMLDiff 注入：

1. **点击翻页（tap-page-turn）**：阅读视图分区点击——左/中区点击上一页、右/下区点击下一页，保留原生滑动、笔、缩放、菜单、选择手势。机制核到的实现（rmtool `tap-page-turn-3.28.qmd`）：在 `SceneViewGestures.qml` 的 `TouchArea` 里按归一化坐标判方向，`view.moveForward()`/`view.moveBackward()` 翻页，并用一串守卫条件（`zoomedIn`/`notePage`/`textMode`/`textSelectionMode`/文件类型等）避免误触。Move 屏小、滑动翻页别扭，点击翻页是刚需级改善，且与 P0 微信读书注入的 EPUB 阅读体验直接叠加。
2. **快速黑白（fast-mono-reading）**：阅读时把屏幕模式切 `Epaper.ScreenModeItem.Mono` 加速刷新，每 N 页（5/10/20/30/从不）用 `ghostBuster.forceClearNow` 清残影，"更多"菜单加原生开关。机制与硬件安全见 4.1 修正块。

**本次进度（2026-08-14）**：**按 .166 真实 QML 重写 + qmldiff 离线实跑 + 分两次真机部署验证通过**。

**建立的离线验证管线（方法论资产，务必复用）**：host 无法运行 xochitl，但能用官方 qmldiff 工具离线实跑补丁——① `cargo build` 出 `asivery/qmldiff` CLI；② 从设备 scp `.166` 的 `/usr/bin/xochitl`（md5 `5215ef7ab…`；本地 `rmfw/` 那份是旧 .164 别用）；③ `xovi-extensions/reading-qol/tools/extract_qml.py`（zstd magic 全局扫）解出 556 个真实 QML，靠内容 grep 认领目标；④ 设备 `hashtab` 解析出全部真实资源路径明文（AFFECT 路径权威来源）；⑤ 把目标 QML 放真实路径下 `qmldiff apply-diffs` 实跑，验解析/选择器命中/emit 合法 QML——与设备端 qt-resource-rebuilder 走同一份 qmldiff 代码。**这条抓出了 `;`→`//` 注释 bug**（INSERT 块内是 QML，注释须 `//`）。

**核对修正的草稿臆想**（rmtool ferrari 推断 vs .166 实际）：`view` = `DeviceSceneView` 的 `FocusScope#root`（`view: root` 注入）；`zoomedIn/zoomedOut` 是 SceneView 上的 bool（草稿臆想的 `itemSelectionMode/textSelectionMode` 不存在）；`Epaper.ScreenModeItem` 无 id 选择器要写全点号（裸 `ScreenModeItem` 不匹配）。

- ✅ **点击翻页**：REBUILD `SceneViewGestures.qml` 的 `touchClick.onClick` 注入分区判方向，`view.moveForward/Backward()`。真机验证通过（`READING-QOL-TAP` 命中日志 + 用户确认）。**分区最终用左右三分**（左 1/3 上一页、右 1/3 下一页、中间中性）——初版按"纵向中段 + 底部通栏"分区，真机发现左下角误翻下一页（底部通栏抢判定），改左右三分修正。
- ✅ **快速黑白**：3 文件 AFFECT——状态注入 `DeviceSceneView#root`、4 指手势切换（`SceneViewGestures`）、周期 `ghostBuster.forceClearNow`（`DocumentView`，`root.ghostBuster` 显式）。真机 Mono 生效。**踩坑·改对对象**：最初 REPLACE `DocumentView` 的 ScreenModeItem，但它阅读时 `visible: globalScreenMode != undefined` 为 false、不控屏，改了没反应；真正控屏的是 `DeviceSceneView` 的 `Epaper.ScreenModeItem{id:content;visible:!screenDriver.globalMode}`（:852）——**apply-diffs 能验"选择器命中"、验不了"是不是真正控屏的那个"，靠真机才暴露**（"形状像不等于对"典型）。
- ✅ 已并入 `deploy/install.sh`（3b 段，与 candidatebar 同目录），规范安装包重建 `cangjie-ime-installer.tar.gz`（含 3 qmd），OTA/重装重跑即恢复。

**★2026-08-17 更新：TODO[集成] 已完成——整套接进设置页控制面板，4 项开关真机端到端验证通过**（当前设备真二进制 md5 **3356dde7**，非旧 .166；裸机恢复后这套本没重铺，此次连面板一起重新落地）。**2026-08-22 面板加第 5 开关「导入书籍自动优化」（默认关）**——消费方是 wr-serve（Rust）非 QML：开关写 `reading-qol.json` 的 `autoOptimize`，wr-serve 后台扫描每轮现读现判、即时生效（详见 §5.7-D）。细节与配方，要点：

- **设置页入口不走阅读器 FormatMenu，而是设置 App**：`settings-reading-enhance.qmd` 往 `Settings.qml` 左侧菜单最下方插「系统增强」`ArkControls.SidebarItem`（设置页用 `onTriggered`）+ 内联「阅读增强」内容页（`SettingsCheckBoxItem` 开关，`selected`+`clicked` 手动翻转）。内容切换=`_selectedPage`(int) 哨兵 990001 → `payloadLoader.sourceComponent` 用 `REBUILD`+`LOCATE AFTER STREAM /{/` 注入早返回。菜单是 `SettingsModel` 驱动的 `Repeater`（项不在 QML 里），故往 `ColumnLayout#settingsColumn` 插静态项。
- **跨 QML 树共享状态 = `reading-qol.json`（QML XHR 读写，/home 持久）**：`QML_XHR_ALLOW_FILE_{READ,WRITE}=1`。**大坑：同步 PUT 到 `file://` 只截断不写体 → 写必须异步；读同步 GET 正常。** 传播靠 `reading-qol-config.qmd` 在 `DeviceSceneView#root` 的 **1.5s 轮询 Timer**（onCompleted 只触发一次、返回阅读器不重建，"改了不生效"就是这个）；字体菜单例外（构建那刻读一次、退出重开生效）。
- **默认全关**：部署后阅读行为零变化，用户在设置里逐项开。fast-mono/清残影页数不再硬编码。4 指手势保留作快捷切换。
- **新增能力**：① 清残影从 fast-mono 拆成**独立开关 `cjRefresh`、彩屏&黑白都生效**（`forceClearNow` 是与屏幕模式无关的硬件全刷），支持**按章**（`tocModel`+`currentPage` 派生切章，`DocumentView.onCurrentPageChanged`）或按 N 页（默认 15）。② **阅读字体增强=菜单追加 3 项不替换**（`add-reading-fonts.qmd` 按 `fontEnhance` 往 `FormatFont` 的 `fontModel.append` 霞鹜文楷/霞鹜新致宋/KF Readerly；**仅 EPUB**——`epub.setFontName`，PDF 固定版式无字体菜单；ttf 装 `/home/root/.local/share/fonts/`）。
- **qmldiff 纪律补充**：嵌套深埋节点 TRAVERSE 必须用通配 `?#id`（非通配只匹配直接子节点，直配 `ColumnLayout#settingsColumn` panic "Cannot locate"）。已并入 `deploy/install.sh`（qmd 列表 + 3 字体 + 初始 `reading-qol.json`）与 uninstall.sh。
- **本地化 en/简/繁（2026-08-17 真机三语验证过）**：面板/菜单/字体名随 UI 语言切换实时跟随。检测踩坑——本机 `Qt.locale().name` 卡 `en_US`、UI 语言不落 `xochitl.conf`（cangjie 还 hook 了 `setLanguageCode`）、`languageSettings` 是下传 property 拿不到；正解用**响应式 `qsTranslate("SettingsModel",{Help,Cloud,Accessibility})`** 判简繁(帮助/幫助、云端/雲端、无障碍/無障礙、未翻译=en)，判别串靠 `lconvert` 反编译 `reMarkable_zh_{CN,TW}.qm` diff 得到。**QML 硬坑**：`property var T` 大写开头→设置页加载失败 `Property names cannot begin with an upper case letter`（改 `i18n`）。
- **崩溃自愈 fail-safe**：`cangjie-qrr-failsafe.sh`(xovi pre-start) 用持久 journald `-b -1` 数上个 boot 崩溃签名 ≥3 次即隔离 6 个阅读增强 qmd（只动自己的），一个重启周期内自愈；干净 restart 不误计。。**一次真实隔离与恢复（2026-08-21）**：2026-08-19 墨香 Navigator 整页化 Popup→Item 编译失败致 xochitl 崩溃 4 次，fail-safe 把 6 个阅读增强 qmd 移进隔离区 → 系统增强菜单/功能全掉。真凶（坏的 moxiang-navigator）修好后，隔离区 qmd 移回 `qt-resource-rebuilder/` + 清 `qrr-failsafe.TRIGGERED` + 重启即恢复（hashtab 未过期不必 rebuild）。**教训：fail-safe 隔离的是"阅读增强这批"、不管真凶是谁——排查看崩溃真凶，别错怪被隔离的无辜 qmd。**
- **返回浮标常驻/延长——判死（2026-08-21）**：脚注跳转后 xochitl 显示"Back to page X"返回浮标、默认 8 秒消失（`DeviceSceneView.qml` 的 `showNotification(...,8000)`，长注释超时就无法返回）。逆向定位到位（`showNotification` 有未用的 `showWithoutTimeout` 参、`messageTimer.interval` 是属性绑定），但两次实测（复杂三元+`2147483647`、简化单行+`45000`）**都连累同 `DeviceSceneView.qml` 的阅读增强全失效**（移除返回浮标 qmd 立即恢复）——是 **REPLACE messageTimer.interval 机制本身**（改大文件深层 Timer 属性→整文件注入回滚）连累，非表达式/大数。**彻底放弃、返回靠原生 8 秒**。规律：改 `DeviceSceneView.qml` 只有 **root 直接子**（fast-mono `FocusScope[#root] > Epaper.ScreenModeItem[#content]` REPLACE mode）安全，**深层节点属性 REPLACE 会拖垮整文件、连累同文件其它 qmd**；且 qmldiff `REPLACE` **只能改属性绑定、不能改函数/信号处理器**（REPLACE 函数报 `Cannot LOCATE Type`）。**"霞鹜文楷 屏幕版"冗余项**：`reader-font-lxgw.qmd`（旧的单字体注入，value 硬编码中文）与 `add-reading-fonts.qmd` 的楷体重复，英文系统显中文碍眼——移除 `reader-font-lxgw.qmd` 即去掉、楷体在 add-reading-fonts 保留。

**许可证**：rmtool QMD 是 GPL-3.0，本项目未复制其文件，按机制自写 QMD，沿用"补丁独立文件、不编译进 `.so`"隔离取舍（不代替法律意见）。

## 10｜候选方向：设备端 AI 助手（借鉴 rmkit-cn，未定级）

> #### 📌 竞品借鉴（rmkit-cn）·选中→AI 助手，机制现成、但与本项目定位有张力
>
> 来源：`boangs/rmkit`（GPL-3.0）`upload-server-go/internal/server/ai_page.go`（文本 AI）、`ai_glyph.go`（手写 AI）、`qmd-src/ai_text_button.qmd`/`glyph_selection_ai.qmd`（选中入口）。**源码研读结论，未在本项目验证。**

**做什么**（rmkit-cn 已实现的形态）：在阅读/笔记里选中一段文字或一片手写笔迹，弹出 AI 操作——润色 / 翻译 / 总结 / 问答，结果以浮层流式显示，可插入或（手写）模拟笔迹回写。技术上全部复用本文其它节已拆解的机制，**没有新的硬骨头**：

- 文字入口：QMLDiff 在选择菜单加"AI"按钮（`ai_text_button.qmd`），从剪贴板拿选中文字（避 `.rm` 延迟，见 06 节）→ 设备端 Go 服务 `/ai-page-chat` → OpenAI 兼容流式转发。
- 手写入口：`glyph_selection_ai.qmd` 取选区坐标 → 截屏裁剪 → 多模态 vision 识别+处理（见 06 节）。
- 回写：文字直接插入，或走 evdev 模拟笔（见 5.2 组件3 的 📌 块）。
- 配置：OpenAI 兼容 URL/Key/Model，扫码从手机填（见 08 节 📌 高级面板）。

**为什么标"未定级候选"而非直接立项**（客观优劣）：

| 优势                                                                                                                  | 劣势 / 与本项目定位的张力                                                                                                                  |
| --------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------ |
| 机制全现成——四个底层能力（截屏 vision、剪贴板取词、evdev 回写、OpenAI 流式）本轮已在别的 P 项借鉴清楚，边际实现成本低 | **偏"消费型 AI 功能"，不是本项目"数据流转率=最大乘数"的主线**——它让内容停在设备上被处理，而非流转进生态                                    |
| 即时、原生浮层体验好，是高频可感知的增益                                                                              | **强依赖云端 AI API + 常态联网 + 凭证存储**——与本项目 P1"OCR 放 host 侧、设备零风险/可离线"的取舍相反；凭证安全又添一处（见 11 节第 4 条） |
| 与已立项功能共享基础设施，不新增受灾面                                                                                | **同质化**——镇纸未做但 rmkit-cn 已做且开源，本项目做它既非首发也非差异化，价值主要是"补齐体验"而非"占空白"                                 |

**结论**：记为候选、**不挤占 P0–P2 的排序与工时**。若将来 P0/P1 落地后设备端已常驻守护进程 + AI 配置（P0 的凭证/联网基础设施已在），加这层 AI 助手是低成本增量，届时再按"用户是否真高频用 AI"定级。置信度：中（机制可行性高，方向价值取决于本项目是否愿意从"数据流转"扩展到"设备端 AI 消费"）。

## 11｜共同风险与架构原则

1. **OTA 与依赖是最大结构性风险，但 P0 与 P1/P2 的风险形态不同，要分开看**。
   - **P1/P2**（rmscene 数据流转线）延续"host 侧为主、设备侧零 hook"的护城河：均只读设备 `.rm` 文件、host 侧反解，不叠加汉化/输入法线的 xovi 受灾面。
   - **P0**（微信读书/墨香）：重活是**用户态 Rust 二进制**（wr-serve 等），与 xochitl 解耦、不进其地址空间，**不共享 langhook 那种进程内 hook 的受灾面**；UI 层是 QML 明文注入（墨香整页面板 + 阅读器菜单），落在 qrr/qmldiff 受灾面上、靠"注入锚点少 + 明文失配只 log 不崩"抗 OTA（不像 langhook 要特征码自定位）。P0 的真实依赖是**微信读书协议稳定性**（端点/编码/签名变了下载同步就断，需真机样本回归）+ **`xochitl_pdf_renderer` 进程边界**（决定不能 hook 渲染、只能走文件系统）；"文档库注入感知新书"这道集成点**已真机验证**（`/upload` 免重启导入 / scp+重启重扫）。
2. **两个不同的单点依赖**。P1/P2 依赖 `.rm` 格式（rmscene 反解）；P0 依赖微信读书 web 端点 + 章节编码方式。两者都遵守同一条纪律的数据线变体：**格式/协议变化后，先用真机样本回归通过，才算兼容**，再谈功能。
3. **许可证：P0 是净室自研，不受 AGPL 传染**（净室纪律见 5.4）。诚实前提是调研已读过 MiuRead 源码，故采可操作的最强净室姿态——协议/解码从微信读书真机流量二次推导、不照搬 `codec.lua`、不复制其代码结构。由此产出为自有版权、自选许可证，非 MiuRead 派生作品；KOReader 不涉及本路线。rmscene（MIT）等 P1/P2 依赖照旧实测 `LICENSE` 确认。所有下载/导出物均为用户自己账号的数据，微信读书书籍内容不进入任何分发渠道。不代替法律意见，只记录事实与架构取舍。
4. **设备端凭证安全**：墨香在设备上存微信读书登录凭证（`/home/root/weread/credentials.json`：api_key + cookies，`wr-renew.timer` 定时续期保活）。注意 `xochitl.conf` 已有明文 SSH 口令/云 token 的泄露前例（见《中文化白皮书》2.5 节）——凭证落在 /home 用户目录、不进 `.so`、不随书分发；进一步的加密存储可后续加固。
5. **可复用的现成肩膀**（实现时直接站上去，不重造）：① hook 韧性机制——`xovi-extensions/cangjie-langhook/` 的 LD*PRELOAD + 特征码自定位（含掩码通配跨固件韧性）+ AArch64 trampoline + 手工构造 QString/QStringList，及已摸到的 `EpubProperties` 阅读管线锚点；② rmscene 反解——`rm-export/export.py` 的 `page_highlights()` + 文档库遍历骨架；③ 差分测试 + 离线 blob——`pinyin-engine/c` 的 `make test`/`make diff-check`（Python 参照→C→逐行 diff）+ `gen*\*\_blob.py`离线生成、设备端`mmap` 只读加载。
6. **本文只保持"当前优先级共识"的单一事实来源**；优先级变动时更新本文并记录调整理由，遵守"发现即写"。P0 已立项，设计不另开方案文档——落在 `weread-client/` 的代码 + README + `ATTRIBUTION.md`（借鉴 MiuRead 之处的标注约定与逐处登记）。

---

**参考来源**：[觅阅 MiuRead](https://github.com/miumiupy98-art/miuread-koreader)（AGPL-3.0，微信读书客户端功能参照）· [KOReader Paper Pro Move 移植 PR #14284](https://github.com/koreader/koreader/pull/14284) · [MobileRead：KOReader on rM Paper Pro Move](https://www.mobileread.com/forums/showthread.php?p=4555909) · [T2000 TCON 分析（myereader）](https://myereader.substack.com/p/remarkable-paper-pro-and-gallery) · [RemarkableFramebuffer](https://github.com/ichaozi/RemarkableFramebuffer) · [rm-hacks-xovi-qmd](https://github.com/Samarkin/rm-hacks-xovi-qmd) · [rmitchellscott/xovi-qmd-extensions](https://github.com/rmitchellscott/xovi-qmd-extensions) · [awesome-reMarkable](https://github.com/rehackable/awesome-remarkable)
