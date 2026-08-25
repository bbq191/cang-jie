# pkm —— PKM 知识管理设备端生产实现（块5）

> **完整设计 + 真机调试记录见《[PKM 白皮书](docs/reMarkablePKM白皮书.md)》**；**设备上怎么用这套 PKM 读书
> —— 读→生→炼→网→观→理 六阶段闭环见《[PKM 白皮书](docs/reMarkablePKM白皮书.md)》§01。** 本 README 只讲 crate 结构与构建。

把 reMarkable 变成 Zettelkasten/PKM 工作台的设备端 Rust 实现。首个能力=**★ 全局待办**：
阅读时用红笔在书上画五角星 → 后台守护进程自动把每本书的星汇总成一个「总结卡片」
笔记本（一星一页 + 章-节名·页号 + **按书的原生 Tag 选模板**：通用/原文英文原版/悬疑/科幻
共 4 套，多 Tag 合并），打字批注跨重建保留；另出 MOC 死链体检（`cardindex`）。

> **与 `pkm-semantic/` 的关系**：`pkm-semantic/` 是同一算法的 **Python 原型 + 阈值标定**
> （穷举/边界差分测试的对拍基准）；本目录是**逐结果对拍后的 Rust 生产移植**。两者同属块5。
> `tests/stars_fixture.rs` 用 `pkm-semantic/proto/testdata/` 的真机 fixture 锁死"与
> `star_scan.py` 全等"（6 个红星），是两边不漂移的锚点。

## 架构：单向依赖块3阅读栈

PKM 建在阅读栈之上（读书要页→章、写笔记要注入书库），因此本 crate **单向依赖**
`../reading/device-rs`（`weread-device`），复用其 `epubindex`（页→章）/`fswatch`
（inotify 封装）/`inject`（写 xochitl 书库）/`notebook_rm`（造 .rm 笔记页）。
reading **不反向依赖** pkm——依赖方向干净单向。这也是"阅读+PKM 曾同居 weread-client
一个 crate"被拆开后的正确形状（2026-08-23 抽出）。

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
`../reading/device-rs/build.sh` 注释（`rustup target add aarch64-unknown-linux-musl`
+ aarch64-gcc）。

## 词典数据（生词本）—— 用户自备、不入库

生词本查词用**用户自备的正版 Kindle 词典**（牛津高阶英汉双解 / 现代汉语词典 MOBI）离线预处理成
排序 TSV，部署到设备 `/home/root/weread/dict/{en,zh}.tsv`，daemon `mmap` 只读二分查。构建：
`python3 tools/build_dict.py --auto --out-dir <本地目录>`（调 calibre 转 MOBI→HTML 再解析，慢、一次性）。
**版权红线**：MOBI 与派生 TSV 均只个人自用、`.gitignore`、绝不入库/分发；仓库只留不含词典内容的
`tools/build_dict.py`。缺词典文件时 daemon 该向不查词（功能自动降级，不报错）。
