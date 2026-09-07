# rmv6 —— 来源与许可留痕

本 crate 是 **剥离移植**（copy，不是 path 依赖）自仓库内 vendored 的第三方 crate
`reading/device-rs/vendor/remarkable_lines`（上游 `remarkable_lines` 0.1.3，
https://github.com/Lyr-7D1h/remarkable-lines ，作者 `Lyr <lyr-7d1h@pm.me>`，`license = "MIT"`）。
许可全文见同目录 `LICENSE`（按 SPDX `MIT` 标签补齐的标准文本；上游未附年份，不杜撰）。
为什么 copy 而不依赖：笔记线的工程原则与书架一致——**不引用旧项目任何 crate**，能力只许剥离移植；
`reading/device-rs` 仍用它自己那份，两边互不影响。

## 相对上游 / vendored 副本的改动（2026-09-06）

- 只留 v6：删 `other/`（v3–v5 页/层/线解析），`RmFile` 只认 `reMarkable .lines file, version=6`，其它版本报 Unsupported。
- 保留 vendored 副本的两处兼容补丁：`PenColor::Unknown(u32)`、`ParagraphStyle::Unknown(u8)` 兜底；块尾多余字节跳过（新固件加尾部字段）。
- `ParagraphStyle` 补 **CHECKBOX(6) / CHECKBOX_CHECKED(7) / NUMBERED(10)**（后者 2026-09-07 真机样本坐实，rmscene 0.8.0 尚不认）。
- 新增高层入口 `page` 模块：一页 = 笔画（Stroke）+ 勾画（Highlight = GlyphRange）+ 打字文本，供 ink-serve 几何配对与 note-serve 读回；
  墓碑（删除的项）自动剔除。

## 2026-09-07 新增：`write` 模块（不是 vendored 代码的一部分）

`write.rs`（编 `RootTextBlock` 供 note-serve 投影用）是**本项目原创代码**，不是从 `remarkable_lines` 移植或改写——
上游那份只有读，没有写。wire 格式理解来自本项目自己这几轮真机样本逆向 + 独立 Python `rmscene` 交叉验证（见笔记线白皮书
§03h/§03i），不涉及上游的 MIT 代码，licensing 边界记在这里备查。
