# SceneLink / 原生链接子系统反解发现（xochitl 3.28.0.169）

> 2026-08-24 深挖。目标：钉死 reMarkable 原生"跨文档链接"（笔记↔书双向跳转的底层抓手）在 `.rm`
> 里的字节结构。方法：**不依赖真机采样**，直接反编译 xochitl 二进制里的读/写序列化代码。
> 工具链：ghidra 12.1.2 headless，程序 `xochitl_3.28.0.169.bin` @ base `0x400000`，脚本
> `scripts/DecompileSceneLink{,2,3}.java`（.rodata 串偏移 → xref 反查 → 反编译）。
>
> **本文按置信度分级（高/中/开放），并显式修正上一版 的一处误判。**

## 0. 一句话结论

**`.rm` 里没有独立的「SceneLink」块类型**。一条原生链接由**两半**构成、分存两处：
① **源锚**在目标页 RootText 的**行内文本格式流**里，是一对 `码5/码6` 标记（与粗体码1/2、斜体码3/4 同族）框住的
文本 CrdtId 区间 —— 这半在 `.rm`，但**只标"哪段是链接"，不带目标**；
② **目标**是独立的 link 记录 `{sourceId → targetId}`，targetId 串解成 `LibraryId`(documentId,pageId)+动作
GotoPage/GotoPageSelection，由**库层打开文档时批量加载**（`loaded N links`）—— 这半**不在 `.rm`**。
SceneLink 本身是 QML 内存值类型（QMetaType 注册），是拼好后的运行期对象。
**命门结论：纯写 `.rm` 造不出可用链接**，必须同时往那个"独立 link 存储"写记录。而 2026-08-24 真机采样撞墙——
**设备无创建 UI、存量零样本**（§5），那个存储落点与字节偏移**无可观测来源可填** → **本方向对本项目判死（不可用现货）**，
MOC 软链接是唯一现货。引擎虽完整存在，除非未来固件放出创建 UI，不再投入。

## 1. 【高置信】.rm 分块格式与块类型全表

块 reader 诊断串（`strings -t x` @ 文件偏移 `1050e50`）坐实分块结构：

```
Unknown block starts at %u: tag=%#2x(%s), version=%d, minVersion=%d, extra len=%lld, payload size=%u
```

→ 每块头 = `payload_len(u32) + unknown(u8) + minVersion(u8) + currentVersion(u8) + blockType/tag(u8)`，
之后是 tagged-field payload（与项目自己的写入器 `reading/device-rs/src/notebook_rm.rs` 及开源 rmscene 一致：
`tag=varuint(index<<4 | type)`，type `0x1`=id/u8、`0x4`=u32、`0x8`=8字节、`0xC`=subblock/Length4、`0xF`=CrdtId）。

**块类型名全表**（16 项，从 JSON 导出器 `FUN_00e490f0` 用的类型名指针表 `PTR_s_Conversion_0191dd40`
逐项解出；数据段 vaddr→file 偏移换算 = `vaddr - 0x410000`）：

| tag | 类型名 | tag | 类型名 |
|-----|--------|-----|--------|
| 0x00 | Conversion | 0x08 | CrdtTombstoneItem |
| 0x01 | SceneTreeMove | 0x09 | Uuid2Index |
| 0x02 | SceneTreeNode | 0x0a | PageStats |
| 0x03 | CrdtGlyphItem | 0x0b | Array |
| 0x04 | CrdtGroupItem | 0x0c | MigrationInfo |
| 0x05 | CrdtLineItem | 0x0d | SceneInfo |
| 0x06 | CrdtTextItem | 0x0e | SceneImagesTable |
| 0x07 | RootText | 0x0f | **CrdtImageItem** |

读分发器 `FUN_00e46ba0` 对 `tag > 0xf` 走 unknown-block fallback `FUN_00e43550`（**逐字节原样保留**——
这就是 reMarkable 的前向兼容机制，也是 rmscene "Some data has not been read…newer format" 警告的来源）。
故**磁盘块 tag 恰好是 0x00–0x0f，全部有名，没有一个叫 SceneLink**。

**⚠ 修正记忆误判**：上一版记忆推断"SceneLink 是一种 rmscene 不解析的独立块"。**错**。rmscene 读不动的其实是
较新固件加的 `SceneInfo(0x0d)`/`SceneImagesTable(0x0e)`/`CrdtImageItem(0x0f)`/`Uuid2Index(0x09)`/`PageStats(0x0a)`/`Array(0x0b)`
这批块，与链接无关。链接不占独立块。

写路径印证：`SceneItem 子类型 → 块 tag` 映射器 `FUN_00e43330`：子类型 1→3(Glyph)、2→4(Group)、3→5(Line)、
6→8(Tombstone)、7→0xf(**Image**)，其余抛 `Unhandled SceneItem type ({})`。

## 2. 【高置信】链接源 = RootText 里的行内文本格式 run（与粗体/斜体同族）

**决定性发现**：`FUN_01062c20` 把 `encountered italic end with id` / `bold end with id` / `link end with id`
放在同一函数处理 —— **链接是一种行内文本格式属性，和粗体/斜体完全同类**，施加在正文某字符区间（由 CrdtId 定界）上。
这正对应动作枚举里的 `GotoPageSelection`（跳到某文本选区）。

格式解析链（RootText 块的行内格式流）：
- `FUN_0104f110`：FormatState 解析器，遍历格式标记（marker `+0x10`=CrdtId、`+0x40`=类型码、`+0x48`=存活标志），
  日志 `- len,marker:` / `FormatState(b:… i:… …)`。
- `FUN_0104cff0`：格式应用器，**3 个格式通道**（`state[0]/[1]/[2]`），每通道一对 开/关 标记码：
  **1/2 = 粗体 开/关、3/4 = 斜体 开/关、5/6 = 链接 开/关**（链接通道 `state[2]` 计数增减）。类型码封顶 0–7（3 bit）。
- `FUN_0104e770`：链接 id 栈 —— link-start(码 5) 压栈、link-end(码 6) 弹栈配对（`encountered link start:` /
  `link end match:`），**只搬 CrdtId，不读任何目标串**。
- `FUN_0104d660`：链接装配 pass（`-- add link:` / `throwing away N links`），把配好的一对 CrdtId 推进链接向量。
- JSON 导出器 `FUN_00e490f0` 把链接导成 `{first, second}` 的 **CrdtId 对数组**（字段 index 9）。

→ **一条链接的源 = RootText 里一对 码5/码6 标记框住的文本 CrdtId 区间**。**标记本身不带 documentId/pageId 目标**。
这与原生"手写 anchor 跟随文字"同属"CrdtId 锚定字符区间"机制族（见）。

**（修正上一轮的 hedge）**：`+0x40` 的 5/6 就是**格式标记类型码 link-start/link-end**，在 `FUN_0104d660`/`0104f110`/
`01062c20`/`0104cff0` 四处一致 —— 与 §1 写映射器的"SceneItem 子类型"枚举不是一套，上一轮的谨慎成立，现已定为格式码。

## 3. 【高置信】链接目标 = 独立 link 记录 {sourceId → targetId}，库层批量加载，**不在 .rm**

- **目标不随 `.rm` 场景存**：§2 的格式标记只标"这段是链接"（3 bit 码），零目标信息。
- 目标模型：`LibraryId` = (documentId `QByteArray`, pageId `QString`)，构造函数 `FUN_009f2300`（pageId 空则报
  `LibraryId referring to a page cannot have an empty pageId`），有**字符串序列化形态** `documentId<分隔符>pageId`，
  由 `FUN_009f3d50` 从串切出 documentId/pageId 再构造。动作 `GotoPage`/`GotoPageSelection`。
- **链接记录 = {sourceId, targetId}**（`.rodata` 相邻串 `sourceId`/`targetId`、`Links`、`sourceId=`、
  `multiple links to file: %s`），打开文档时**批量加载**（`-> loaded %d links in %.3fms`）。
- LibraryId 构造的 4 个调用者全在 **0x94–0x9e 库/文档子系统**，**不在 .rm 解析器（0x104/0xe4）**——坐实目标绑定是库层职责。
- SceneLink 本身是 QML 值类型（`FUN_005e5a30` 仅 QMetaType 注册），是**已解析的内存对象**，`onLinkActivated` 消费。

**闭环**：`.rm` 存**源锚**（哪段文字是链接，靠 CrdtId=sourceId）；独立 link 存储存**{sourceId → targetId 串}**，
targetId 串解成 LibraryId(documentId,pageId)+动作。两者由库层在打开时拼起来，交给 QML `onLinkActivated` 跳转。

## 4. 命门结论：**纯写 `.rm` 造不出可用链接**（可行性下调）

回答上一轮开放问题 #2：**不能只改 `.rm` 就凭空造一条能跳转的原生链接**。要造一条链接得**两处都写**：
1. 往目标页 RootText 的格式流插一对 码5/码6 标记，框住源文本区间（`.rm` 侧，字段偏移仍需采样钉死）；
2. 往库层加载的**独立 link 存储**写一条 `{sourceId → targetId=documentId<sep>pageId}` 记录 —— **该存储的确切落点
   （`.content`？`.metadata`？sidecar？DB？）本轮未定位**，是仅剩的关键未知数。

且原生 UI 创建路径（选择工具能否跨文档选另一本书为目标）未采样。承 工程纪律 铁律，动手前**必须**先让用户在设备
原生建一条跨文档链接当样本：diff（有链接页 vs 无链接页的 `.rm`）钉格式标记字节偏移，再找出那次操作**新写/改动了哪个文件**
（`.content`/sidecar）定位 link 存储，Python 差分对拍一致才写。

## 5. 真机采样验证（2026-08-24）：无法采样 → 方向判死

计划的"真机采样一条原生链接（diff `.rm` + 文件系统）"落地即撞墙，**采样这条路本身走不通**：

1. **无创建 UI（实测 + 官方核实）**：设备（固件 .169，56 个文档）用**选择工具（套索）**圈选文字后的菜单里
   **没有**"链接/Link/跳转"入口。官方所谓 "Create a link" 是**分享菜单里生成 web 网页链接发邮件**
   （[Software release 3.23](https://support.remarkable.com/s/article/Software-release-3-23)），**不是**文档间跳转；
   设备能"点击跳转"的链接主要来自**导入 PDF 的内嵌超链接**（pdfium `FPDFLink_*`，另一套、与 `.rm` 无关）。
   *（typed-text 文本菜单 / 页面缩略图菜单两处未逐一实测，但见下条三重否定已足够。）*
2. **存量零样本**：扫全设备 55 个 `.content`，**0 个**含 `targetId`/`sourceId`/`SceneLink`/`links` 记录；
   文档目录里**没有独立的 link 存储文件类型**（只有 metadata/content/thumbnails/local/tombstone/textconversion/
   highlights/pagedata/pdf/epubindex/epub）。即**从没有、也无法**产生过一条原生笔记本链接。
3. → **无创建入口 + 无存量样本 = 无从 diff、无从定位 link 存储落点**。那两个关键未知数（字节偏移 + 存储落点）
   在这台设备上**没有可观测的现实来源**可填。

**终判定**：原生 SceneLink 引擎在二进制里完整存在（本文 §1–§3 反解闭环），但 **Move 固件 .169 没给用户/我们任何创建入口，
设备上也无一条样本** → **对本项目判死（不可用现货）**，从"中远景"降级。纯注入造链接因此彻底不成立：
既缺原生样本对拍（无"正确性锚"，违反 工程纪律"先采样不猜写"铁律），又无法定位 link 存储落点。

**MOC 软链接（`[ID:]` + 全局搜索，已落地、零依赖）是唯一现货路径**，白皮书 §07 已采用。除非未来固件放出原生"创建链接"UI
（届时可重启采样），此方向不再投入。PDF 内嵌链接（pdfium）是独立的另一套，与笔记本/EPUB 跳转无关，本项目用不上。

## 附：复现

```bash
# ghidra headless（程序已导入工程 xochitl_analysis，base 0x400000；数据段偏移换算 vaddr-0x410000）
~/.local/opt/ghidra_12.1.2_PUBLIC/support/analyzeHeadless \
  ghidra-project xochitl_analysis -process xochitl_3.28.0.169.bin -noanalysis \
  -scriptPath ghidra-project/scripts -postScript DecompileSceneLink<N>.java
```
六轮脚本：`1` 锚点 xref+反编译 · `2` 读/写分发器+callers · `3` LinkProvider 侧 ·
`4` LibraryId 校验/详细链接解析器/setTarget+装配上游 · `5` LibraryId 构造的 callers+格式应用器 ·
`6` LibraryId 工厂（串→documentId/pageId）。

关键函数（vaddr）：
| 角色 | 函数 |
|------|------|
| .rm 块读分发器 / unknown fallback | `FUN_00e46ba0` / `FUN_00e43550` |
| 子类型→块 tag 映射器 | `FUN_00e43330` |
| 类型名表（16 项） | `PTR_s_Conversion_0191dd40` (vaddr `0x191dd40`) |
| RootText FormatState 解析 / 格式应用器（3 通道） | `FUN_0104f110` / `FUN_0104cff0` |
| 链接 id 栈（码5压/码6弹） / 装配 pass / 上游 | `FUN_0104e770` / `FUN_0104d660` / `FUN_0104f110` |
| 粗/斜/链接 end-with-id | `FUN_01062c20` |
| JSON 导出（links={first,second}对） | `FUN_00e490f0` |
| LibraryId 构造(校验) / 串工厂 | `FUN_009f2300` / `FUN_009f3d50` |
