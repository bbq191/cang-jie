# SceneLink / 原生链接子系统反解发现（xochitl 3.28.0.169）

> 2026-08-24 深挖。目标：钉死 reMarkable 原生"跨文档链接"（笔记↔书双向跳转的底层抓手）在 `.rm`
> 里的字节结构。方法：**不依赖真机采样**，直接反编译 xochitl 二进制里的读/写序列化代码。
> 工具链：ghidra 12.1.2 headless，程序 `xochitl_3.28.0.169.bin` @ base `0x400000`，脚本
> `scripts/DecompileSceneLink{,2,3}.java`（.rodata 串偏移 → xref 反查 → 反编译）。
>
> **本文按置信度分级（高/中/开放），并显式修正上一版 的一处误判。**

## 0. 一句话结论

**`.rm` 里没有独立的「SceneLink」块类型**。SceneLink 是 QML 层的**内存值类型**（QMetaType 注册），
链接在磁盘上以 **CrdtId 为键的成对 start/end 标记**形式散落在标准 item 块里；链接目标
（`LibraryId` = documentId+pageId，动作 GotoPage/GotoPageSelection）由 `LinkProvider` 模块运行期解析，
**是否落 `.rm`、落在哪，本轮未钉死**。要合成一条链接仍需先真机采样确认字节序——但采样目标已从
"整块未知结构"收窄到"标记项的字段 + 目标存储位置"。

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

## 2. 【高置信】链接的场景内表示 = CrdtId 成对 start/end 标记

链接装配 pass `FUN_0104d660`（含日志串 `- link start encountered:` / `- link end encountered:` /
`-- add link:` / `throwing away N links` / `parsing`）遍历场景节点流，把**一条链接识别成一对匹配的标记**：

- 每个场景 item 带 CrdtId（item `+0x10`，6 字节 `(author_u8, counter_varuint)`）+ 一个字符串值字段（item `+0x28`）。
- pass 扫描时遇 start 标记把其 CrdtId 收进 seen 集，遇 end 标记与之配对 → `-- add link:` 把
  `{CrdtId, QString}` 推进链接向量（`this+0x28/0x30/0x38`，stride 0x20）。
- JSON 导出器 `FUN_00e490f0` 把链接序列化成 `{first, second}` 的 **CrdtId 对数组**（字段 index 9），
  与装配 pass 互印证：**链接 = 源文档内一段内容的 (起点 id, 终点 id) 锚**。

这与原生"手写 anchor 跟随文字"是同一类"用 CrdtId 锚定字符/内容区间"的机制族（见），
不是独立块，而是寄生在 item 流里的边界标记。

**⚠ 未过度断言**：`FUN_0104d660` 里判定 start/end 用的是 item `+0x40` 字段值 5/6，但该枚举与 §1 映射器的
子类型枚举（6=Tombstone）**不是同一套**，故本文**不**声称"链接标记 = SceneItem 子类型 5/6"。可靠的只有
"CrdtId 成对 start/end"这一形态（日志 + JSON 对佐证），精确枚举值待进一步定位。

## 3. 【中置信】链接目标由 LinkProvider 运行期解析

- SceneLink 是 QML 值类型：`FUN_005e5a30`/`FUN_005eb1d0` 仅做 `QMetaType` / `std::vector<SceneLink>` 注册，
  无字段布局——它是**已解析的内存对象**，非序列化实体。
- 目标模型（符号级，.169 仍在）：`xofm::libs::linkprovider::LinkDetails` 目标 = `LibraryId`(documentId, pageId)，
  动作枚举 `GotoPage` / `GotoPageSelection`；`LinkProvider`/`LinkProviderWrapper`/`LinkProviderDevice` 模块 +
  QML `onLinkActivated`。`documentId`/`pageId`/`GotoPage(Selection)` 串均在（GotoPage 有 3 份副本 = QML 枚举注册表）。
- 这些串**无直接代码 xref**（经 QML meta-object 表间接引用），故本轮没能顺藤摸到"目标从哪读"的确切函数。

## 4. 开放问题（要么更深追踪、要么真机采样）

1. **标记项落在哪个块 tag、字段偏移几何**：start/end 标记是 CrdtLineItem/CrdtGlyphItem/CrdtTextItem 里的
   一个子字段，还是 SceneTreeNode 上的属性？未钉到字节。
2. **链接目标 (documentId, pageId, action) 存在 `.rm` 里还是外部**：可能是标记项 `+0x28` 的字符串值，
   也可能在文档 metadata / 独立 links 存储 / DB 里由 LinkProviderDevice 解析。**这决定"能否只写 `.rm` 造链接"**——
   若目标在外部，光合成场景标记不够。
3. 承 工程纪律 铁律（新写场景项先只读采样验证、不猜写，Step S 崩机教训）：合成链接前**必须**先让用户在设备
   原生创建一条跨文档链接当样本，diff（有链接页 vs 无链接页）+ 与本文的块表/标记形态对拍一致，才动手写。

## 5. 对"笔记↔书双向跳转"需求的判断（更新）

- 机制**存在且在 .169 存活**，符号完整 —— 方向没被证伪。
- 但**不是"改 `.rm` 就能造链接"的现货**：链接目标的持久化位置未明，且原生 UI 创建路径（选择工具能否跨文档选目标）
  未采样。**置信度：机制成立=高；纯注入层可造=中低（受目标存储与 UI 授权两个未知数制约）**。
- 定位为**中远景**，优先级低于 MOC 软链接（`[ID:]` + 全局搜索，已落地、零依赖）。软链接是现货，原生链接是备选升级。

## 附：复现

```bash
# 块类型名表（数据段偏移换算 vaddr-0x410000）
python3 - <<'PY' # 见本轮；输出 16 项类型名表
PY
# 三轮反编译（ghidra headless，程序已导入工程 xochitl_analysis）
~/.local/opt/ghidra_12.1.2_PUBLIC/support/analyzeHeadless \
  ghidra-project xochitl_analysis -process xochitl_3.28.0.169.bin -noanalysis \
  -scriptPath ghidra-project/scripts -postScript DecompileSceneLink.java   # 锚点 xref + 反编译
# DecompileSceneLink2.java：读/写分发器 + callers；DecompileSceneLink3.java：LinkProvider 侧
```

关键函数（vaddr）：读分发器 `FUN_00e46ba0`、unknown-block fallback `FUN_00e43550`、
子类型→tag 映射器 `FUN_00e43330`、链接装配 pass `FUN_0104d660`、JSON 导出器 `FUN_00e490f0`、
类型名表 `PTR_s_Conversion_0191dd40`（vaddr `0x191dd40`）。
