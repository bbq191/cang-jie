# notes · 笔记线

reMarkable Paper Pro Move 的**笔记增强层**：书架（`shelf/`）补读书短板，本目录做强它的强项——**荧光笔勾书 + 在勾出来的内容旁边手写**。
凡是勾了、写了的，合上书就自动汇入条目库；在**手机**上改错、分区；按分区调智能；投影成设备笔记本（一章一本）与 Obsidian md（一章一文件）。

> 工程原则与书架一致（白皮书 §00）：XDG 路径 · 设计模式去重解耦 · 专项专用可插拔多服务 · **不引入任何旧代码**（`knowledge/pkm`、`reading/`
> 只借鉴功能与踩坑；`.rm` 解析与页→章映射剥离移植成新 crate）· 设备只负责写，不负责改；改在手机，回写靠重建；**条目库是唯一事实源**。

## 四步闭环

```
① 合上书  ──事件驱动──►  ink-serve 矿     解析书页 .rm：勾画(GlyphRange 原文+矩形) ↔ 旁边手写(笔画簇) 几何配对 → 条目库 + 裁图
② 手机改  ──「笔记」tab─►  改转写 / 选分区 / 选样式（设备只读，不再在 e-ink 上打字）
③ 智能    ──按分区────►  transcribe-serve 转写手写；mind-serve 按「分区名 + 简述」跑模型（"背诵"类不调）
④ 投影    ──note-serve──►  设备《书名》文件夹一章一本（xochitl 7 种打字样式）· vault/书名/第N章.md（反链）
```

## 目录

```
notes/
├── Cargo.toml · .cargo/               内部 workspace（与 shelf 同款 musl 全静态）
├── crates/rmv6/                       .rm v6 只读解析（剥离移植 remarkable_lines 0.1.3，MIT，PROVENANCE.md 留痕；page::Page 高层视图）
├── crates/epubmap/                    .epubindex 起始页 + nav/ncx 目录 → 页号→章/小节
├── crates/notecore/                   领域核心（纯函数）：条目/分区模型 · 簇指纹 · 聚簇+配对 · 增量合并规则
├── services/ink-serve/                矿（8795）：监听书库、摄取、裁图、条目库唯一写者、HTTP + 事件
├── services/{transcribe,mind,note}-serve/   待建
├── testdata/                          真机 fixture（《人骨拼圖》.content/.epubindex/toc.ncx/墓碑页 .rm）
└── docs/reMarkable笔记白皮书.md          决策 / 真机 / 踩坑
```

## 增量规则（回答"二次识别会不会把改好的字覆盖回去"）

条目库是事实源，笔记本与 md 只是投影。每片手写按笔画集合算指纹：指纹不变 → 不重识别、不动校对文本；补了几笔 → 同一条目、新草稿只作建议；
笔画全擦 → 标「已撤销」不删。校对过的字永远不会被覆盖。

## 构建

```sh
cd notes && cargo build --workspace && cargo test --workspace     # host
sh build.sh                                                        # aarch64 musl（待补）
```
