# reMarkable 知识组织设计建议（EPUB / 剪藏 / Zettelkasten 卡片 / 手写笔记 的存放结构）

> 2026-08-25 初稿，**2026-08-26 §四翻案更新**。针对设备上异构知识产物「书 / 网页剪藏 / 卡片 / daemon 汇总本 / 手写笔记」平铺一堆的现状，基于真机实况提整理结构 + 落地路径。
> **命门（daemon 自动归档）2026-08-26 真机验证 = 可行**（§四翻案）——不走 metadata parent（那个确被 /upload 忽略），走**上传前 `GET /documents/<folderId>` 设当前文件夹（全局态）再 /upload**。已落地：daemon 生成物（卡片+4汇总+生词本）→ `zettelkasten`，书（墨香+优化版）→ `library`，无文件夹兜底 root。§五/§六 的"留 root+命名聚类现实版"是翻案前旧结论，已作废（保留备查）。

## 一、现状诊断（真机 22 文档，17 在 root，5 trash）

| 类别 | 数量 | 例 |
|---|---|---|
| 📗 正式书（EPUB+epubindex） | 混在下面 | 《13 67》 |
| 📰 网页/公众号剪藏（EPUB，短/无章节） | 混在下面 | 《我24岁患帕金森12年…》《200本豆瓣…》《干细胞"刹车"帕金森？》 |
| 🃏 Zettelkasten 卡片（daemon 生成） | 4 | 《13 67》- 总结卡片 |
| 🤖 daemon 自动汇总本 | 4 | 🔗卡片索引 / 🎨高亮汇编 / 📖复盘队列 / 📊阅读仪表 |
| ✍️ 手写笔记 | 2 | 任务记忆 / 缺失功能 |
| 📁 文件夹 | 2 | **library / zettelkasten（均为空）** |

**三个关键信号：**

1. **你已建了 `library` + `zettelkasten` 两个文件夹，但它们全空**——17 个文档全平铺 root。
   **有组织意图，但没落地**。这份建议就是把这个意图补全。
2. **根因（代码坐实）**：daemon 生成卡片/汇总本走 `inject::build_metadata`，其中 **`"parent": ""` 硬编码**
   = 一律落 root。所以就算你手动把卡片拖进 `zettelkasten`，**下次画星 daemon 换 uuid 重建、又跳回 root**——
   手动归档必然被冲掉。**这是"归档白做"的机制根因，也是自动归档方案的命门。**
3. **书与剪藏异质却同类混放**：正式书（《13 67》，长、多章、要精读）和网页剪藏（帕金森/豆瓣，短、无章节、
   读完即弃或转卡片）都是 `epub`，挤在一起。二者的生命周期、复习价值完全不同。

## 二、设计原则

- **按"知识流"分层，不按文件类型分**：输入（素材）→ 加工（卡片）→ 产出（汇总）。这比"所有 epub 一堆、
  所有 notebook 一堆"更贴合实际用法。
- **daemon 自动归档 > 手动拖**：Move 小屏 + 无键鼠，手动整理成本高且会被 daemon 重建冲掉。让 daemon
  生成时就落到正确文件夹，零维护。
- **极简克制**：7.3 寸小屏，文件夹别超两层；宁可少分类，不制造"找不到"。

## 三、建议存放结构

```
📁 library（原始输入·素材，只读多、加工少）
│   ├─ 📗 正式书          《13 67》《赎罪》…    ← 精读、长期留、反复复习
│   └─ 📰 剪藏            帕金森/豆瓣/干细胞…   ← chrome/公众号抓取，短，读完转卡片即可归档/删
│
📁 zettelkasten（知识加工·产出）
│   ├─ 🃏 卡片            《X》- 总结卡片       ← daemon 生成，一书一卡
│   ├─ 🗺 MOC             000-全局MOC、主题MOC  ← 你手动策展的主题地图
│   └─ 📊 仪表盘          🔗索引/🎨汇编/📖复盘/📊仪表  ← daemon 汇总本，收进一处不挤 root
│
📁 手写笔记（与阅读无关的原生 rm 本：任务记忆…）
```

要点：
- **书 vs 剪藏分开**：剪藏是"一次性素材"（价值转进卡片后原文可弃），正式书是"长期资产"。分开后
  `library/📰剪藏` 可以放心批量清理，不误伤真书。
- **4 个 daemon 汇总本收进 `zettelkasten`**（🔗卡片索引=死链体检/🎨高亮汇编/📖复盘队列/📊阅读仪表）：它们是"看板"，天天生成、不该和内容本挤在 root 顶部。
- **卡片和 MOC 同在 zettelkasten**：卡片是原子、MOC 是网，同属"加工层"，就近互链。

## 四、落地路径（★ 2026-08-26 翻案：自动归档 **可行**，走 GET-then-upload）

**★ 翻案（2026-08-26 真机验证，肯定）**：§四原判死是因为只测了 metadata `parent` 这一条路。用户提示「web UI 先进文件夹再 upload 就落进去」→ 逆向 web UI 前端（`/assets/index.js`）发现真机制：
上传前先 **`GET /documents/<folderId>` 设「当前文件夹」（全局服务端状态）**，随后 **`POST /upload` 就落进该文件夹**（FormData 只带 file，parent 不在 metadata）。真机实测：分开连接 GET zettelkasten/library → POST upload，
测试 EPUB **准确落入对应文件夹**（全局态、不必绑连接）。

**故 daemon 自动归档现已落地**（`device_core::inject::{find_folder_by_name, set_upload_folder}`）：
- daemon 生成物（卡片 + 4 汇总 + 生词本）→ 上传前 GET `zettelkasten` → 落卡片盒；
- wr-serve 的墨香书 + 自动优化版 → GET `library` → 落书库；
- Chrome 抓取文章由外部工具传、不经我们代码 → 不处理（落哪算哪）；
- 文件夹按名认领 uuid（`CollectionType`+`visibleName`），**不存在 → 空串 → 兜底落 root**（web UI 无建文件夹端点，用户手动建一次即自动归档）。

**原判死的三条约束现状**（仍成立但不再挡路）：metadata `parent` 确被忽略（本翻案绕开它）；daemon 换 uuid（无妨——每次新卡直接 GET-then-upload 进文件夹，不需认领旧卡）；xochitl 无视磁盘直写（无妨——不走磁盘，走 web 上下文）；无 move 接口（无妨——GET /documents 即"进文件夹"上下文，等价定位）。
`cardnote::pack_rmdoc_in`（塞 metadata parent）确认无效、弃用，改走 `set_upload_folder`。

<details><summary>历史：2026-08-25 的否定结论（已被上面翻案，保留备查）</summary>

造 metadata `parent=<zettelkasten uuid>` 的 rmdoc upload 到 `/upload` → 设备上 `parent` 被重置为 `""` 落 root、uuid 也被换（`cardnote::pack_rmdoc_in` + `examples/verify_parent.rs` 实测）。当时据此判「自动归档不可行」——**错在只测了 metadata parent 一条路，没测 GET-then-upload**。
</details>

→ **daemon 生成物自动归档已落地**（见上 §四 GET-then-upload），素材仍靠用户手动归档一次，见下 §五。

## 五、落地方案（§四翻案后）

**分两类处置：**

1. **daemon 生成物（卡片 + 4 汇总本 + 生词本）——自动归档进 `zettelkasten`（已落地，§四）。**
   - 上传前 `GET /documents/<zettelkasten uuid>` 设当前文件夹 → /upload 落卡片盒；文件夹不存在兜底 root。
   - 既有散落卡片在**下次内容变更重传**时迁入（内容哈希跳过不强制 churn，避免整库 churn）；新卡直接进。
   - 用户无需手动拖卡片（拖了下次重传也会回到 zettelkasten，一致）。

2. **原始素材（正式书 / 网页剪藏 / 手写笔记）——书由 wr-serve 自动归 `library`，其余手动归档一次即稳。**
   - 墨香下的书 + 自动优化版 → wr-serve 上传前 GET `library` → 自动进书库（§四同机制）。
   - Chrome 抓取文章（rmdoc/EPUB）由外部工具传、不经本项目代码 → **不处理**，落哪算哪（用户可手动归档）。
   - 手写笔记 / 你策展的 MOC → 用户拖 `笔记`/`zettelkasten` 一次即固定（daemon 不碰、uuid 不变）。
   - **书 vs 剪藏区分**：书走 wr-serve 天然进 library；剪藏是外部工具产物、不自动归档。若要自动分流剪藏，
     靠**来源标签**（`#书`/`#剪藏`）或启发式（剪藏多单章/无 TOC/短），由抓取工具侧处理，与本项目 daemon 无关。

**净结论**：reMarkable 扁平库 + 忽略 metadata parent 的 `/upload`，曾让人误判「自动归档不可行」；**GET-then-upload
（先设当前文件夹再上传）绕开了这道坎** → daemon 生成物自动进 zettelkasten、书自动进 library，只有外部剪藏留给用户。

## 六、落地状态

1. **daemon/wr-serve 自动归档**：`device_core::inject::{find_folder_by_name, set_upload_folder}` 已接进
   `sync_one_card`/`sync_auto_notebook`（卡片盒）与 `autoopt`/`pipeline`/`wr_serve`（书库）。文件夹不存在兜底 root。
   （`cardnote::pack_rmdoc_in` 塞 metadata parent 无效、弃用。）
2. **无对应文件夹**：web UI 无建文件夹端点、xochitl 无视磁盘直写 → 无法自动建；兜底落 root + 用户手动建一次即自动归位。
3. 剪藏 `#书`/`#剪藏` 区分标签由抓取环节打，非 daemon 职责。
