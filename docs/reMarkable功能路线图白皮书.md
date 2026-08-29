# reMarkable Paper Pro Move 功能路线图白皮书

> 方向评估与优先级白皮书 · v1.0 · 2026-08

在 UI 中文化与拼音输入法两条线基本收工（见姊妹文档《[reMarkable 中文化白皮书](../chinese-ime/docs/reMarkable中文化白皮书.md)》《[reMarkable 拼音输入法白皮书](../chinese-ime/docs/reMarkable拼音输入法白皮书.md)》）之后，本文回答一个问题：**这台设备上，下一步做什么最值。**

> 注：中文化核心已归拢进 `chinese-ime/` 子文件夹（2026-08-15 重构）。本文中出现的 `xovi-extensions/cangjie-langhook/`、`pinyin-engine/` 等旧路径一律对应 `chinese-ime/langhook/`、`chinese-ime/pinyin-engine/`；`rm-export/` 仍在顶层。**`weread-client/` 已于 2026-08-23 拆成 `reading/`（阅读）+ `knowledge/pkm/`（PKM 生产 crate），本文旧引用一律对应新目录。**

**范围声明：** 本文是方向评估文档，不是实现方案——只做候选功能的价值排序、可行性论证和否决理由，具体实现细节待立项后另开方案文档。评估范围同样限定"本机个人定制"，不涉及破解 DRM、绕过付费校验或分发修改版固件。

## 本章目录

1. [01 · 结论速览](#01结论速览)
2. [02 · 评估框架](#02评估框架)
3. [03 · 生态盘点](#03生态盘点)
4. [04 · 已否决方向及理由](#04已否决方向及理由)
5. [05 · P0：微信读书集成——「墨香」（MoXiang），全设备自足](#05p0微信读书集成墨香moxiang全设备自足)
6. [06 · P1：手写识别（手写 → 文字/结构 → 笔记生态）（块⑥）](#06p1手写识别手写--文字结构--笔记生态块)
7. [07 · P2：手写笔记反解与导出（含手写-文字 anchor）（块⑥）](#07p2手写笔记反解与导出含手写-文字-anchor块)
8. [08 · P3：中文字体渲染优化（块2 中文化）](#08p3中文字体渲染优化块2-中文化)
9. [09 · P4：笔记 QoL——先用社区现成扩展](#09p4笔记-qol先用社区现成扩展)
10. [10 · 候选方向：设备端 AI 助手（借鉴 rmkit-cn，未定级）](#10候选方向设备端-ai-助手借鉴-rmkit-cn未定级)
11. [11 · 共同风险与架构原则](#11共同风险与架构原则)

## 01｜结论速览

| 优先级          | 方向                                                | 一句话理由                                                                                                                                                                              | 地基完成度         | 置信度 |
| --------------- | --------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------ | ------ |
| **P0 ✅**       | 微信读书集成——「墨香」，全设备自足                  | 用原生阅读器+原生笔读微信读书的书；**全设备自足 Rust 二进制 + 墨香整页 app + 阅读器菜单**（host 脐带剪断）；扫码登录/下载/划线想法**双向同步**全设备端；净室自研借鉴 MiuRead 逻辑不抄码 | **端到端真机验证** | 高     |
| **PKM ✅**      | ★ 全局待办 —— PKM 语义引擎首个能力                  | 阅读时红笔画五角星 → 后台 Rust daemon 自动汇总成每本书的「总结卡片」笔记本（一星一页 + 章-节名·页号 + **按书原生 Tag 选 4 套模板**：通用/原文/悬疑/科幻，多 Tag 合并）；另出 MOC 死链体检（`cardindex`）。daemon 走 `fswatch`/inotify 事件驱动（**增量：只扫变更书**，旧卡清理另由 `trash-agent.qmd` 挂 `onRowsInserted`）、全设备自足、设置页开关  | **端到端真机验证** | 高     |
| **P1（块⑥）**   | 手写识别：手写 → 文字/结构 → 笔记生态               | 生态盘点确认的差异化空白，识别放 host/设备侧调云绕开设备算力                                                                                                                            | de-risk 通过 + host cardhw/export + **端化 A1/A2/B 真机通**（A=设备调云+事件触发，B=设置面板；见块⑥白皮书）；仅 C 通知桥/token 统计待建 | 中     |
| **P2（块⑥）**   | 手写笔记反解与导出（含手写-文字 anchor）            | 承接原"闭环3"rmscene 反解成果；原生已把笔迹锚到字符位置，"混排笔记结构化导出"没人做过                                                                                                   | 反解已生产落地（cardhw 复用；见块⑥白皮书） | 中     |
| **P3 ✅**       | 中文字体渲染优化（块2）                             | 显示清晰度唯一可行子集，文件级改动、低风险、独立小胜                                                                                                                                    | **核心随 P0 真机通**（`.content` fontName=霞鹜文楷）；观感 A/B 待定，见中文化 §3.7 | 高     |
| **P4 ✅**       | 笔记 QoL（系统增强 + 社区扩展）                     | 系统增强面板自研真机通；其余社区 xovi 扩展现成，先装后评                                                                                                                                | **系统增强真机通**（点击翻页/快刷/清残影/字体菜单等，块4）+ 社区扩展可装 | 高     |
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

### 3.1 项目分块地图（5 块）

项目从"中文输入法 + 汉化"长成一套设备增强套件后，按功能/产品线组织成 5 块。这是全项目的组织骨架，本文的 P0–P4 路线都落在其中某一块内。目录名已与分块对齐（2026-08-23 把历史名 `weread-client/` 拆成 `reading/`+`knowledge/pkm/`，2026-08-25 再抽共享底座 `device-core/`）；块3阅读与块5 PKM 都依赖 `device-core`，方向单向无环，见下方 ⚠️。

| # | 块 | 落点 | 本文对应 | 状态 |
|---|---|---|---|---|
| 1 | **逆向基座** | `ghidra-project/` · `rmfw/` | 全线共享地基（第 11 节架构原则） | 持续维护 |
| 2 | **中文化**（显示 + 输入法） | `chinese-ime/` | 两本姊妹白皮书；本文 P3 字体 | 收尾维护 |
| 3 | **阅读**（微信读书 + EPUB 优化） | `reading/` 主体 | **P0 墨香** + EPUB 优化器（见[阅读白皮书](../reading/docs/reMarkable阅读白皮书.md)） | 端到端真机验证 |
| 4 | **系统增强**（阅读/显示/笔记 UX） | `xovi-extensions/` + `chinese-ime/langhook`（笔记增强） | **P4**（见[系统增强白皮书](../xovi-extensions/docs/reMarkable系统增强白皮书.md)：点击翻页/快刷/清残影/字体/键盘Mono + 荧光笔吸附） | 真机验证 |
| 5 | **PKM / 知识管理** | `knowledge/pkm-semantic/`（原型）+ `knowledge/pkm/`（Rust 生产 crate，依赖共享底座 `device-core/`） | **★全局待办**（见 [PKM 白皮书](../knowledge/pkm/docs/reMarkablePKM白皮书.md)）；P1/P2 数据流转是其上游 | 首个能力真机端到端 |

> ⚠️ **跨块共享 + 依赖结构**（详见《[设备端 Rust 架构](reMarkable设备端Rust架构.md)》）：① 块 4 的荧光笔吸附逻辑在 `langhook`（块 2 的 .so）、开关 UI 在 `reading-qol`（块 4），一颗 .so 服务两块，不拆二进制；块 4 的划词查字典代码骑 `knowledge/pkm/` daemon（跨块，见系统增强白皮书 §08）。② 低层设备能力（`epubindex`/`fswatch`/`inject`/`notebook_rm`）抽成**共享底座 `device-core/`**，块3阅读（`weread-device`）与块5 PKM（`pkm-device`）都依赖它；`knowledge/pkm/` **生产只依赖 `device-core`**（weread-device 降 dev-dep）→ 生产 daemon 不再编译整条 weread 管线。`reading/` 不反向依赖 `knowledge/pkm/`，方向单向无环。

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

## 05｜P0：微信读书集成——「墨香」（MoXiang）+ 通用 EPUB 优化（块3 阅读）

> **完整设计与真机调试记录见《[阅读白皮书](../reading/docs/reMarkable阅读白皮书.md)》。** 本节只保留路线图层面的定位与状态。

**做什么**：在 reMarkable Paper Pro Move 上用微信读书——扫码登录自己账号、书下载进 xochitl 书库**原生阅读、原生笔划线**、划线/想法**双向同步**到云端；顺带把任意第三方 EPUB 也优化到能读。做成设备端功能「墨香」：侧边栏入口 + 整页 app + 阅读器「...」菜单。功能逻辑**借鉴觅阅 MiuRead（AGPL-3.0）、净室自研不抄源码**（详见阅读白皮书 §04）。

**★架构定案（已真机端到端）**：从早期"host Python 大脑 + 设备薄 hook"彻底演进到**全设备自足**——扫码登录/分片下载+解码/组 EPUB/注入书库/划线想法双向同步/会话续期全由设备上的 aarch64 Rust 静态二进制（`reading/device-rs`：`wr-serve`/`wr-download`/`wr-renew`）完成，host 脐带全剪断。两堵硬墙定了形状：① 渲染器是独立进程 `xochitl_pdf_renderer`（hook 触达不到）→ 集成靠"写文档库 + 读 `.rm` 反解"；② 设备无通用运行时 → 只能跑自编 aarch64 静态二进制 + 注入 QML。

**状态**：**P0 全链路真机端到端验证通过**——下载/注入/双向划线想法同步/内联画回/EPUB 优化/整页墨香 app/阅读器菜单/荧光笔汉字吸附。当前真实剩余风险只有 ② 翻页硬件天花板（Gallery 3 物理刷新，非软件可解）+ ③ 协议脆弱性（上游一改需真机回归）。

- **§5.7（并入阅读白皮书 §07）· 通用 EPUB 优化器 + 阅读体验做到极致**：下书 EPUB 组装升级（整章一页/多级目录/脚注内联）、xochitl 弹窗脚注判死（穷尽实测负结论）、通用 EPUB 优化器（字体解锁/破脚注互指/封面拉伸/去冗余目录页）、无感自动优化 + 原生回收站通道（`trash-agent.qmd` 走 `selectionMoveToTrash` 原生代码路）。全真机验证。
- **§5.8（迁至《[PKM 白皮书](../knowledge/pkm/docs/reMarkablePKM白皮书.md)》）· ★ 全局待办**——PKM 语义引擎首个能力（红笔画星 → 后台 Rust daemon 自动汇总总结卡片）已迁入 PKM 白皮书，见块5。

## 06｜P1：手写识别（手写 → 文字/结构 → 笔记生态）（块⑥）

**做什么**：设备手写中文笔记 → 识别成文字/结构，进 PKM/笔记生态，让手写可检索、可引用。**定位**：不追设备实时识别（算力 + 深 hook 风险高），追「写完不死在设备里」——Supernote「实时识别」卖点的流转版替代。

**为什么是 P1**：生态盘点确认的最大差异化空白（三家中文化竞品都没碰）。

**立项判据 AMBER-GREEN + 已落地**：识别质量 de-risk 通过（工整~100%/快写~60%，需人工校对）；host cardhw（卡片手写批注→内联注入）+ freeform export 真机通；**端化 cardhw A1/A2/B 真机通**（A=设备自主调云→/upload 注入+关笔记事件触发，B=设置面板开关/选后端/填 key，2026-08-29）；仅 C 通知桥+token 统计待建。

> **深内容全部下沉块⑥白皮书**：de-risk 结论、cardhw 空间关联/内联注入/泄漏过滤、端化 A1/A2 架构（含 SVG 流程图）、反解基础设施、rmkit-cn 借鉴——见《[手写识别白皮书](../knowledge/pkm-semantic/handwriting/docs/reMarkable手写识别白皮书.md)》。路线图只留优先级定位。

## 07｜P2：手写笔记反解与导出（含手写-文字 anchor）（块⑥）

**做什么**：rmscene 反解 `.rm`，按原生「手写 anchor 到 typed text 字符位」把「打字正文 + 手写批注」混排笔记结构化导出。是 P1 的反解地基（承接原「闭环3」成果，与 PKM 共用 `device-core`）。**导出侧（混排笔记完整离开设备）无人做**，是「设备创作 → 生态沉淀」闭环最后一环，全 host 侧、设备零 hook、OTA 受灾面≈0。

> 详见《[手写识别白皮书](../knowledge/pkm-semantic/handwriting/docs/reMarkable手写识别白皮书.md)》§05。

## 08｜P3：中文字体渲染优化（块2 中文化）

**做什么**：xochitl 文档正文对中文走 fallback、未为汉字调校。核心机制——**文档正文由独立进程 `xochitl_pdf_renderer` 渲染、不吃 fontconfig 别名，只认文档 `.content` 的 `fontName` 字段**（区别于 UI 界面走 fontconfig）；注入时设 `fontName="LXGW WenKai"`（霞鹜文楷），正文简繁英全楷体、UI 不受影响。

**为什么排 P3**：显示清晰度里唯一软件可达的子集，文件级改动、低风险、独立小胜。

**状态：核心已随 P0 注入实验真机验证通过（2026-08-13）**；观感 A/B 优选（几款字体对比）待定。

> 完整机制（两套字体渲染路径 + `.content` fontName + 落地/许可证）已下沉《[中文化白皮书](../chinese-ime/docs/reMarkable中文化白皮书.md)》§3.7。路线图只留状态指针。

## 09｜P4：笔记 QoL——先用社区现成扩展

**原则：不重造轮子。** 先安装社区现成 xovi 扩展验证价值：

- [rm-hacks-xovi-qmd](https://github.com/Samarkin/rm-hacks-xovi-qmd)：双笔切换手势、Move 小屏适配（隐藏关闭按钮）等。
- [rmitchellscott/xovi-qmd-extensions](https://github.com/rmitchellscott/xovi-qmd-extensions)：可折叠目录等。

用真实使用一段时间后，只对"确实高频且三方都没有"的缺口考虑自研。注意：第三方扩展与本项目 `cangjie-langhook.so` 共存时，遵守既有纪律——一步一确认、独立部署验证、qt-resource-rebuilder 的 qmd 补丁逐个启用。

### 9.1 系统增强（阅读/显示/笔记 UX，设置页「系统增强」面板）—— ✅ 已真机端到端验证

> **完整设计与真机调试记录见《[系统增强白皮书](../xovi-extensions/docs/reMarkable系统增强白皮书.md)》（块4）。** 本节只保留路线图层面的定位与状态。

给 xochitl 原生阅读器/系统加一层 **纯 QMLDiff UX 增强**（不碰硬件/waveform/渲染进程，全部默认关、设置页逐项开）：**点击翻页**（左右 ~10% 窄边缘 + 纵向上 25%/下 15% 排除，Move 小屏刚需）、**快速黑白**（锁 Mono 加速刷新）、**清残影**（`ghostBuster.forceClearNow` 硬件全刷，彩屏黑白都生效）、**阅读字体增强**（EPUB 字体菜单追加霞鹜文楷等）、**键盘 Mono**（组词态键盘区快刷）。设置页门户注入 `Settings.qml`、跨 QML 树状态走 `reading-qol.json`（XHR 写必须异步 + 全量防覆盖铁律）。「笔记增强」二级分类含荧光笔汉字吸附（本体见阅读白皮书 §03-3d）+ ★全局待办开关（本体见 PKM 白皮书）。参照 rmtool（GPL-3.0）机制净室重写、不复制其 QMD。缘起从 4.1「显示优化」拆出独立立项（硬件安全见 4.1 修正块）。全真机验证；建立的离线 qmldiff 验证管线是可复用方法论资产。

## 10｜候选方向：设备端 AI 助手（借鉴 rmkit-cn，未定级）

> #### 📌 竞品借鉴（rmkit-cn）·选中→AI 助手，机制现成、但与本项目定位有张力
>
> 来源：`boangs/rmkit`（GPL-3.0）`upload-server-go/internal/server/ai_page.go`（文本 AI）、`ai_glyph.go`（手写 AI）、`qmd-src/ai_text_button.qmd`/`glyph_selection_ai.qmd`（选中入口）。**源码研读结论，未在本项目验证。**

**做什么**（rmkit-cn 已实现的形态）：在阅读/笔记里选中一段文字或一片手写笔迹，弹出 AI 操作——润色 / 翻译 / 总结 / 问答，结果以浮层流式显示，可插入或（手写）模拟笔迹回写。技术上全部复用本文其它节已拆解的机制，**没有新的硬骨头**：

- 文字入口：QMLDiff 在选择菜单加"AI"按钮（`ai_text_button.qmd`），从剪贴板拿选中文字（避 `.rm` 延迟，见 06 节）→ 设备端 Go 服务 `/ai-page-chat` → OpenAI 兼容流式转发。
- 手写入口：`glyph_selection_ai.qmd` 取选区坐标 → 截屏裁剪 → 多模态 vision 识别+处理（见 06 节）。
- 回写：文字直接插入，或走 evdev 模拟笔（见[阅读白皮书](../reading/docs/reMarkable阅读白皮书.md) §03 组件3 的 📌 块）。
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
3. **许可证：P0 是净室自研，不受 AGPL 传染**（净室纪律见[阅读白皮书](../reading/docs/reMarkable阅读白皮书.md) §04）。诚实前提是调研已读过 MiuRead 源码，故采可操作的最强净室姿态——协议/解码从微信读书真机流量二次推导、不照搬 `codec.lua`、不复制其代码结构。由此产出为自有版权、自选许可证，非 MiuRead 派生作品；KOReader 不涉及本路线。rmscene（MIT）等 P1/P2 依赖照旧实测 `LICENSE` 确认。所有下载/导出物均为用户自己账号的数据，微信读书书籍内容不进入任何分发渠道。不代替法律意见，只记录事实与架构取舍。
4. **设备端凭证安全**：墨香在设备上存微信读书登录凭证（`/home/root/weread/credentials.json`：api_key + cookies，`wr-renew.timer` 定时续期保活）。注意 `xochitl.conf` 已有明文 SSH 口令/云 token 的泄露前例（见《中文化白皮书》2.5 节）——凭证落在 /home 用户目录、不进 `.so`、不随书分发；进一步的加密存储可后续加固。
5. **可复用的现成肩膀**（实现时直接站上去，不重造）：① hook 韧性机制——`xovi-extensions/cangjie-langhook/` 的 LD*PRELOAD + 特征码自定位（含掩码通配跨固件韧性）+ AArch64 trampoline + 手工构造 QString/QStringList，及已摸到的 `EpubProperties` 阅读管线锚点；② rmscene 反解——`rm-export/export.py` 的 `page_highlights()` + 文档库遍历骨架；③ 差分测试 + 离线 blob——`pinyin-engine/c` 的 `make test`/`make diff-check`（Python 参照→C→逐行 diff）+ `gen*\*\_blob.py`离线生成、设备端`mmap` 只读加载。
6. **本文只保持"当前优先级共识"的单一事实来源**；优先级变动时更新本文并记录调整理由，遵守"发现即写"。P0 已立项，设计不另开方案文档——落在 `reading/` 的代码 + README + `ATTRIBUTION.md`（借鉴 MiuRead 之处的标注约定与逐处登记）。

---

**参考来源**：[觅阅 MiuRead](https://github.com/miumiupy98-art/miuread-koreader)（AGPL-3.0，微信读书客户端功能参照）· [KOReader Paper Pro Move 移植 PR #14284](https://github.com/koreader/koreader/pull/14284) · [MobileRead：KOReader on rM Paper Pro Move](https://www.mobileread.com/forums/showthread.php?p=4555909) · [T2000 TCON 分析（myereader）](https://myereader.substack.com/p/remarkable-paper-pro-and-gallery) · [RemarkableFramebuffer](https://github.com/ichaozi/RemarkableFramebuffer) · [rm-hacks-xovi-qmd](https://github.com/Samarkin/rm-hacks-xovi-qmd) · [rmitchellscott/xovi-qmd-extensions](https://github.com/rmitchellscott/xovi-qmd-extensions) · [awesome-reMarkable](https://github.com/rehackable/awesome-remarkable)
