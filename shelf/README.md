# shelf · 书架

reMarkable Paper Pro Move 的**读书与阅读质量层**。它是跑在设备上的一组网页服务：用手机或电脑浏览器把书（EPUB/PDF）传进设备，
需要时点「优化」让书在墨水屏上排得更好，再选放进哪个阅读器：官方阅读器 xochitl，或第三方阅读器 KOReader。
字体和壁纸也做成"上传即可用"，KOReader 的调优配置写成代码、可以一键重新应用。**不修改 xochitl 本体**。

> 这份 README 只讲能做什么、有哪些服务、怎么构建部署。细节去看文末「文档索引」里的四份文档；全项目概览看
> [`../docs/OVERVIEW.md`](../docs/OVERVIEW.md)，用户可见的更新历史看 [`../docs/CHANGELOG.md`](../docs/CHANGELOG.md)。

## 能做什么

![一本书的旅程](docs/diagrams/book-journey.svg)

所有书先**原样**进**母版库**（设备上永久保存原书的地方），之后的动作互相独立、都可以反复做：

| 动作 | 说明 |
|---|---|
| 入库 | 网页上传（多文件，有进度）、抓网文（贴网址，组成 EPUB）、scp 进设备 `inbox/` 目录。**只收 EPUB / PDF**，其它格式拒收；书名规范成 `书名 - 02卷` |
| 优化（可选） | EPUB：清洗、排版、补目录和封面、处理图片，产物过质量门才替换原书；漫画自动识别、保画质、裁白边。PDF：有文字层的按原版式转成 EPUB（原 PDF 留 7 天可恢复），扫描件和漫画 PDF 只裁白边 |
| 落库 | 加入 xochitl（≤90MB 网页上传；更大的走"占位 + 磁盘替换"，上限 1GiB）或加入 KOReader（本地复制）。母版不会因此删除 |
| 其它 | 按书设**阅读方向**（自动 / 从右往左 / 从左往右，日漫用）；只选一本时可**下载原件**、**改名**；批量操作由网关排队逐本执行、可全部中止 |
| 周边 | xochitl 字体上传即装、休眠壁纸上传即用、KOReader 字体/词典上传与配置补丁、KOReader 高亮/生词导出给笔记线 |

网页有四个固定标签页：**传书**（入库、母版库）· **笔记**（笔记线）· **其他**（xochitl 字体 / KOReader / 壁纸，只列已装的）· **管理**（基石与模块、设备健康、模型、系统增强、电池刺客、实验室）。

2026-09-18 起**没有电脑端命令行**（原 `shelf` CLI 与 Calibre 管线整体砍除，没有网页替代）；其它格式请先在电脑上自行转成 EPUB/PDF 再上传。

## 服务与端口

![shelf 架构](docs/diagrams/architecture.svg)

| 服务 | 端口 | 职责 | 源码 |
|---|---|---|---|
| gateway | `0.0.0.0:443`，唯一对外 | HTTPS + 登录密码、网页 UI、反向代理、批量队列、并发/内存闸门 | `../gateway` |
| book-serve | 127.0.0.1:8790 | 母版库：入库、优化、落库、下载/改名、阅读方向；xochitl 回收站 / 建文件夹 / 漫画页边距 / 日漫翻页的设备端代理（见书架白皮书第 C 章） | `services/book-serve` |
| koreader-serve | 127.0.0.1:8791 | 书放进 KOReader、字体/词典/配置同步、高亮与生词只读导出 | `services/koreader-serve` |
| font-serve | 127.0.0.1:8792 | xochitl 字体上传即装（改写 fontconfig 中文回退链） | `../enhance/font-serve` |
| wallpaper-serve | 127.0.0.1:8793 | 休眠壁纸上传即用（写 xochitl 的 `SleepScreenPath` 键） | `../enhance/wallpaper-serve` |
| 笔记线四服务 | 8795–8798 | 见 [`../notes/README.md`](../notes/README.md) | `../notes` |

- 服务启动时往 `$XDG_RUNTIME_DIR/shelf/services/` 写一份注册信息，网关据此出标签页、按 `/api/<段>/*` 转发（`books`→book-serve、`koreader`→koreader-serve、`fonts`、`wallpapers`）。装/卸一个服务 = 一个二进制 + 一个 systemd 单元。
- 全部接口清单见 [`docs/传书EPUB线架构.md`](docs/传书EPUB线架构.md) §9；KOReader 配置接口见 [`koreader/README.md`](koreader/README.md)。
- systemd：`shelf.target` + 各服务 `PartOf=shelf.target`；`systemctl disable --now font-serve` 即拔掉字体服务。**绝不给 xochitl 加启动依赖**（曾因此变砖）。

## 访问与密码

- 地址 `https://shelf.local/`（网关自带 mDNS；**安卓不解析 .local**，见书架白皮书 §03j）或 `https://<设备IP>/`。**传大书走 USB 的 `https://10.11.99.1`**，WiFi 热点上行弱（书架白皮书 §03l）。
- 登录只要密码：首次默认 `shelf`，登录后强制改（≥6 位）。网页右上「改密码」，或设备上 `gateway passwd <新密码>`；忘记用 `gateway reset-password`。
- 证书由私有 CA 签发：登录页「下载 CA 证书」装进手机/电脑信任库一次，此后没有"不安全"提示。

## 目录

```
shelf/
├── Cargo.toml · build.sh · .cargo/   内部 workspace；aarch64 musl 全静态交叉编译
├── crates/bookconv/                  优化引擎（清洗、优化、质量门、图片、漫画、PDF 入库、占位文档、网文抽取、命名）
├── crates/pdf-extract-cj/            pdf-extract 0.12.1 的本地 fork（MIT）：给 PDF 转 EPUB 提供颜色、图片位置、正确的中文字宽
├── services/book-serve/              母版库服务
├── services/koreader-serve/          KOReader 服务（含纯 Rust 只读 SQLite 解析器）
├── systemd/                          shelf.target + 两个领域服务单元
├── xovi/                             注入 xochitl 的 qmd：字体菜单、回收站/建文件夹/漫画页边距代理、阅读器单击翻页与日漫翻页规则
├── koreader/                         KOReader 配置补丁 + merge.lua（见 koreader/README.md）
├── install.sh · uninstall.sh · manifest.sh   设备端安装/卸载与共用清单
└── docs/                             四份文档 + diagrams/
```

依赖单向无环：`services/* → ../rmsvc-core`；`book-serve → bookconv → pdf-extract-cj`；koreader-serve 不依赖 bookconv。
设备上的路径（XDG，HOME=/home/root）：母版库 `~/.local/state/shelf/books/staging/`；配置 `~/.config/shelf/<服务>.json`；
二进制 `~/.local/bin/`；安装备份 `~/cangjie-backups/shelf-<时间戳>/`（留最近 5 份）。完整路径表见书架白皮书附录 C。

## 构建 · 部署 · 卸载

一次性准备：`rustup target add aarch64-unknown-linux-musl`，再装 aarch64 交叉 gcc（Arch：`pacman -S aarch64-linux-gnu-gcc`，只用来编 `ring` 的 C 部分）。改代码前先看工程纪律。

```sh
cd shelf && sh build.sh                        # host 测试 + aarch64 构建（gateway / enhance / notes 在的话一起编）
cargo test --workspace                         # 只跑测试：2026-09-25 共 422 个通过、1 个忽略
cd ../packaging && sh deploy.sh 10.11.99.1     # 打包 → 传到设备 → install.sh（先备份旧文件）；只有 WiFi 时给 WiFi IP
sh deploy.sh 10.11.99.1 --only font,wallpaper  # 只装部分服务；SHELF_NO_BUILD=1 跳过编译
sh deploy.sh 10.11.99.1 --password '新密码'     # 顺便设网关密码（经 ssh 标准输入传，不上命令行）
ssh root@10.11.99.1 '~/.local/bin/shelf-uninstall' [--only font] [--purge]
```

- `--only` 令牌：`gateway book koreader font wallpaper ink transcribe mind note`（网关总会装；未知令牌 install.sh 退出码 2）。
- `install.sh` 依赖同目录的 `manifest.sh` 与 `devlib.sh`，`deploy.sh` 会一起打包，**只拷一个 install.sh 到设备不够**。安装幂等：先校验载荷，二进制/qmd 原子替换，只重启有变化或没在跑的服务。
- 整套设备增强（固件安全门、wifi-watch、enhance 扩展、书架、笔记线）一条命令：`packaging/install-all.sh <host>`，见 `../packaging/README.md`。
- ⚠ **qmd 改动怎么生效**：`install.sh` 只把 qmd 放到位、不重启（会打断阅读）；要生效就**整机重启**——电脑上跑 `packaging/deploy-xovi-apply.sh <设备>`，或设备上 `reboot`。2026-09-25 起**不再单独 `systemctl restart xochitl`**（xochitl 退出时自身有概率崩溃，再由系统整机重启）；xovi 已生效时**绝不**跑 `xovi/start`（2026-09-20 事故）。`deploy.sh` 只装不重启；`install-all.sh` 最后的 `xovi-apply` 步会在有改动时自动整机重启。详见书架白皮书第 F 章「速查」。

**固件升级（OTA）之后**：`/home` 数据不丢，但要重装功能。权威步骤在 [`../docs/INSTALL.md`](../docs/INSTALL.md)「固件升级（OTA）之后」：设备旁手动 `xovi/rebuild_hashtable`，再在电脑上重跑 `packaging/install-all.sh <host>`。

## 文档索引

![书架文档地图：带着问题找文档](docs/diagrams/sh-doc-map.svg)

| 文档 | 管什么 |
|---|---|
| [`docs/传书EPUB线架构.md`](docs/传书EPUB线架构.md) | **数据流与服务分工**：书在各服务间怎么流动、母版库状态、落库通道、内存设计、并发与锁、全部 API 与配置 |
| [`docs/EPUB优化规范白皮书.md`](docs/EPUB优化规范白皮书.md) | **规则**：书该被改成什么样、为什么；xochitl 实测渲染与跳转规则；质量门 |
| [`docs/bookconv优化白皮书.md`](docs/bookconv优化白皮书.md) | **实现**：优化引擎各模块的函数、常量、版本号与实现层的坑 |
| [`docs/reMarkable书架白皮书.md`](docs/reMarkable书架白皮书.md) | **现状总览 + 真机历史与坑**：各章"现状结论"、决策来由、事故与教训、待办、已砍能力 |
| [`koreader/README.md`](koreader/README.md) | KOReader 配置即代码：文字书/漫画两套方案、补丁与应用接口 |
