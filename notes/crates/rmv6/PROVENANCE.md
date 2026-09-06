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
- `ParagraphStyle` 补 **CHECKBOX(6) / CHECKBOX_CHECKED(7)**（与 rmscene 一致；3.28 有序列表码待设备样本读回后补）。
- 新增高层入口 `page` 模块：一页 = 笔画（Stroke）+ 勾画（Highlight = GlyphRange）+ 打字文本，供 ink-serve 几何配对与 note-serve 读回；
  墓碑（删除的项）自动剔除。
