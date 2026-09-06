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
├── services/transcribe-serve/         转写（8796）：订阅矿的事件 → 裁图喂视觉模型（Qwen 缺省，OpenAI 兼容口可换）→ 草稿写回；唯一出网
├── services/{mind,note}-serve/        脑 待建；本 只有骨架（注册「笔记」tab）
├── testdata/                          真机 fixture（《人骨拼圖》.content/.epubindex/toc.ncx/墓碑页 .rm）
└── docs/reMarkable笔记白皮书.md          决策 / 真机 / 踩坑
```

## 增量规则（回答"二次识别会不会把改好的字覆盖回去"）

条目库是事实源，笔记本与 md 只是投影。每片手写按笔画集合算指纹：指纹不变 → 不重识别、不动校对文本；补了几笔 → 同一条目、新草稿只作建议；
笔画全擦 → 标「已撤销」不删。校对过的字永远不会被覆盖。

## 转写（transcribe-serve）

- 触发：订阅 ink-serve `/events`，`entries` 事件防抖 3 s 后跑一轮；启动追平一次；网页「转写」按钮同步跑一轮；单条「重转」强制跑。
- 一轮：列书 → 只看 `pending>0` 的书 → 条目 `needs_transcribe()`（簇指纹没有对应草稿）→ 取裁图 → 提示词（内置 + 勾画原文作语境）→ 模型 → `POST ink /books/{uuid}/entries/{id}` 写 `draft{text,backend,at,hash}`。
  已校对 `text` 由 ink-serve 保证不被覆盖。行首标记兜底：条目样式仍是正文时，转写文本开头 `-`/`1.`/`口` → 无序/有序/待办并剥掉标记（`notecore::marker`）。
- 节制：每轮最多 `maxPerRun`（缺省 20）条、请求间歇 `pauseMs`；同一条同指纹失败 `maxAttempts`（3）次后不再自动试（网页「重试失败」清）；一轮连续失败 3 次零成功即停（key 错/断网不一条条撞）。
- 配置 `~/.config/notes/transcribe.json`（0600）：backend/baseUrl/model/apiKey/timeoutSecs/maxPerRun/pauseMs/auto/maxAttempts/prompt。key 来源：文件 → 环境 `DASHSCOPE_API_KEY`；`GET /config` 只报 `hasKey/keySource` **不回显**。
- 用量 `~/.local/state/notes/transcribe.json`：次数、token 累计、最近错误、上轮报告；不存转写内容。

## 构建

```sh
cd notes && cargo build --workspace && cargo test --workspace     # host
cd ../shelf && ./build.sh && ./deploy.sh                           # 随书架一起交叉编译/打包/装机（NOTES_BINS）
```
