# pkm —— PKM 知识管理设备端生产实现（块5）

> **完整设计 + 真机调试记录见《[PKM 白皮书](docs/reMarkablePKM白皮书.md)》**；**设备上怎么用这套 PKM 读书
> —— 读→生→炼→网→观→理 六阶段闭环见《[PKM 白皮书](docs/reMarkablePKM白皮书.md)》§01。** 本 README 只讲 crate 结构与构建。

把 reMarkable 变成 Zettelkasten/PKM 工作台的设备端 Rust 实现。首个能力=**★ 全局待办**：
阅读时用红笔在书上画五角星 → 后台守护进程自动把每本书的星汇总成一个「总结卡片」
笔记本（一星一页 + 章-节名·页号 + **按书的原生 Tag 选模板**：通用/原文英文原版/悬疑/科幻
共 4 套，多 Tag 合并），打字批注跨重建保留；另出 MOC 死链体检（`cardindex`）。

> **与 `knowledge/pkm-semantic/` 的关系**：`knowledge/pkm-semantic/` 是同一算法的 **Python 原型 + 阈值标定**
> （穷举/边界差分测试的对拍基准）；本目录是**逐结果对拍后的 Rust 生产移植**。两者同属块5。
> `tests/stars_fixture.rs` 用 `knowledge/pkm-semantic/proto/testdata/` 的真机 fixture 锁死"与
> `star_scan.py` 全等"（6 个红星），是两边不漂移的锚点。

## 架构：依赖共享底座 `device-core`（方案B，2026-08-25）

PKM 要读书页→章、写笔记注入书库，这些**低层设备能力**抽在 `../../device-core` 共享底座 crate
（`epubindex` 页→章 / `fswatch` inotify 封装 / `inject` 写 xochitl 书库 / `notebook_rm` 造 .rm 笔记页；
块3阅读也用它）。本 crate **只依赖 `device-core` 这一小坨**——**生产 daemon 构建不再全量编译整条微信读书
管线**（weread-device 的下载/codec/login/qr…，5657 行）。`device-core` 的四模块本就零 crate 内依赖、自足；
`weread-device` re-export 它们（`pub use device_core::…`）保 `weread_device::epubindex` 等路径不变。

> **历史**：曾 pkm 直接 path 依赖整个 `weread-device`（拖入全部 reading 编译）。方案B 抽 `device-core` 后依赖精确。
> 仅 `tests/stars_fixture.rs` 走 `weread-device`（**dev-dependency**，测 `epub::assemble` 组装 A 模型待办 EPUB），
> 不影响生产构建。reading **不反向依赖** pkm，方向干净。

## 二进制

| bin | 作用 |
|-----|------|
| `wr-stars-daemon` | **生产守护进程**：挂 systemd 常驻，`fswatch` 监听文档目录（**增量：只扫变更书**）→ `stardetect` 扫红星 → `epubindex` 页映射到章-节名 → 按书的原生 Tag（文档级+页级）为**每星选模板**（4 套：通用/原文/悬疑/科幻，`cardsync`）→ 增量 merge → `cardnote` 造/更新总结卡片笔记本 → `/upload` 注入；另 `cardindex` 出全库锚点索引 + MOC 死链体检（SSH 报告 + 库内「🔗 卡片索引」笔记本）。事件驱动去重。**外加一条解耦的附加扫描——生词本**：全库源书**⚪灰色荧光笔划过的词**（脱离画星）→ `dict` mmap 二分查本地词典（用户自备牛津英汉双解/现汉派生的排序 TSV）→ `locate` 在 EPUB 章全文定位取原句 → `cardvocab` 汇成一本「📕 生词本」。查词与 星→卡片 管线互不干扰（灰高亮照常进卡片灰槽）。 |
| `wr-stars` | 手动扫描 CLI：一次性扫库出 ★待办 markdown（调试/对拍用）。 |
| `wr-nbtest` | 笔记本造页测试件（不部署到设备）。 |

## 构建 & 部署

```sh
./build.sh              # host 自测 + 交叉编 aarch64-unknown-linux-musl 全静态
./deploy.sh [host]      # scp wr-stars-daemon + wr-stars 到设备 /home/root/weread
```

设备端落点 `/home/root/weread`（与块3阅读同目录，`wr-stars-daemon` 的 systemd 单元
路径不变——抽 crate 只改仓库侧、不动设备布局）。前置工具链见
`../../reading/device-rs/build.sh` 注释（`rustup target add aarch64-unknown-linux-musl`
+ aarch64-gcc）。

## 词典数据（生词本）—— 用户自备、不入库

> **分块归属**：查字词/生词本概念属**块4 系统增强**（阅读辅助），代码在此是跨块（复用荧光笔读回+笔记本注入
> 管线，不单拆二进制）。完整设计见《[系统增强白皮书](../../xovi-extensions/docs/reMarkable系统增强白皮书.md)》§08。


生词本查词用**用户自备的正版 Kindle 词典**（牛津高阶英汉双解 / 现代汉语词典 MOBI）离线预处理成
排序 TSV（**牛津 en.tsv 164,493 条 · 现汉 zh.tsv 62,642 条**，2026-08-25 已部署真机
`/home/root/weread/dict/`），daemon `mmap` 只读二分查。构建：`python3 tools/build_dict.py --lang en
--html <calibre 转出的 index.html> --out en.tsv`（calibre 完整转 htmlz 对大词典会在 CSS-flatten 报错，
用 `ebook-convert x.mobi out.epub --debug-pipeline=D` 取 `D/input/index.html` 输入阶段产物即可，解析同）。
**版权红线**：MOBI 与派生 TSV 均只个人自用、`.gitignore`、绝不入库/分发；仓库只留不含词典内容的
`tools/build_dict.py`。缺词典文件时 daemon 该向不查词（功能自动降级，不报错）。
