# shelf · 书架

reMarkable Paper Pro Move 的**书籍搬运层**。它是跑在设备上的一组网页服务：用手机或电脑浏览器把书（EPUB/PDF）传进设备，
再加入设备自带的阅读器 xochitl。字体和壁纸也做成"上传即可用"。**不修改 xochitl 本体，也不改书**。

> **书的优化不在这里做**（2026-10-07 起）：先在电脑上用 [sheng-ren](https://github.com/bbq191/sheng-ren) 的 booklib，按 `xochitl` 阅读模式把书优化成 EPUB，
> 再传上来。书架原样投书：设备上的「优化」、PDF 转 EPUB、原 PDF 备份、抓网文、联网补封面都删了。
> **这次改动已合 master（15b4069），2026-10-07 13:02 已部署（`deploy.sh --only book`），部署自检 37✓ 0⚠ 0✗，功能未真机手测。**

> **2026-09-29 起只剩 xochitl 一个阅读器**：设备上卸掉了 KOReader、第三方 WeRead 与 appload。书架随之撤掉「加入 KOReader」、
> KOReader 字体/词典/配置、高亮回流等全部网页入口，`koreader-serve` 不再安装；它的源码和 `koreader/` 配置补丁 2026-09-30 也
> 已从仓库删除（见 git 历史）。下文凡提到 KOReader 的都是历史。

> **部署状态**：设备上跑的仍是 2026-09-30 部署的版本（那时还在设备上优化书），部署自检通过，**功能还没逐项手测**；2026-10-07 的改动待部署。清单见书架白皮书附录 §05。

> 这份 README 只讲能做什么、有哪些服务、怎么构建部署。细节去看文末「文档索引」（第一次来先看书架白皮书开头的「现状」）；全项目概览看
> [`../docs/OVERVIEW.md`](../docs/OVERVIEW.md)，用户可见的更新历史看 [`../docs/CHANGELOG.md`](../docs/CHANGELOG.md)。

## 能做什么

![一本书的旅程](docs/diagrams/book-journey.svg)

用户视角三步：**电脑上用 sheng-ren 优化** → **网页上传（或 scp 进 inbox）** → **母版库勾选「加入 xochitl」**。书**原样**进**母版库**（设备上永久保存原书的地方），之后可以反复加入：

| 动作 | 说明 |
|---|---|
| 入库 | 网页上传（多文件，有进度）、scp 进设备 `inbox/` 目录（写完、文件静止 5 秒后才收）。**只收 EPUB / PDF**，字节原样不动；同名同内容不重复存；文件名规范成 `书名 - 02卷` |
| 加入 xochitl | ≤90MB 网页上传；更大的走"占位 + 磁盘替换"，上限 1GiB；再大就整本拒收。可选文件夹（没有就让 xochitl 自己建）。加完自动记下渲染页数（不判断好坏）。母版不会因此删除 |
| 漫画页边距 | sheng-ren 优化的漫画带页边距标记，加入后首次打开一律自动把页边距设到最小（2026-10-07 起没有开关；想要回原来的边距就在阅读器里自己调回，每本只设一次）；不带标记的书不碰。书架以前自己优化的漫画不带标记，要用 sheng-ren 重新优化 |
| 其它 | 只选一本时可**下载原件**、**改名**；母版库筛选 全部 / 未加入 / 已加入（默认「未加入」）；批量加入由网关排队逐本执行、可全部中止。书架不管翻页方向（xochitl 里日漫一律从左往右翻） |
| 周边 | xochitl 字体上传即装、休眠壁纸上传即用 |

网页有四个固定标签页：**传书**（入库、母版库）· **笔记**（笔记线）· **其他**（xochitl 字体 / 壁纸，只列已装的）· **管理**（基石与模块、设备健康、模型、系统增强、实验室；「电池刺客」2026-09-30 已移除）。

**已移除的能力**（历史叙事留在书架白皮书，这里只列结论）：

| 能力 | 移除时间 | 现在怎么办 |
|---|---|---|
| 设备上的「优化」、入库 PDF 转换（有文字层转 EPUB、无文字层裁边）、原 PDF 7 天备份、抓网文、联网补封面、旧版产物兼容、中途取消、优化徽章与筛选 | 2026-10-07（已部署，功能未真机手测） | 书在电脑上用 sheng-ren 优化好再传；网页链接也用 sheng-ren 收书 |
| 网关并发闸门与行内"取消排队"、渲染自检的"页数远低于预期"警告 | 2026-10-07 稍后（已合 master（9c2571f），10-07 15:54 已部署并整机重启，部署自检 36✓ 1⚠ 0✗，功能未手测） | 网页加入只走批量队列，本来就一本一本来；渲染好坏由 sheng-ren 的质量门把关，母版库只显示页数 |
| 电脑端命令行 `shelf`（push / Calibre 管线 / doctor 等） | 2026-09-18 | 没有网页替代；其它格式先在电脑上自行转成 EPUB/PDF 再上传 |
| KOReader 一切（加入 KOReader、字体/词典/配置同步、高亮与生词回流） | 2026-09-29 | 只用 xochitl；`koreader-serve` 与 `koreader/` 补丁已从仓库删除（2026-09-30），见 git 历史 |
| 按卷拆分投递（EPUB 按目录、PDF 按书签切成多份） | 2026-09-30 | 超过网页上限走大文件通道，超过 1GiB 整本拒收 |
| 按书手动设置翻页方向 | 2026-09-30 | 当时改成只认书里自带的方向标记；旧的手动清单 09-30 起只读 |
| 日漫翻页（按书里的方向标记把滑动方向对调，「日漫翻页规则」开关） | 2026-10-07 稍后（已合 master（9c2571f），10-07 15:54 已部署并整机重启，部署自检 36✓ 1⚠ 0✗，功能未手测） | 书架只管入库；xochitl 不看方向标记，日漫一律从左往右翻（用户接受）。单击翻页保留 |
| 漫画 EPUB→PDF 转换器（`comic_pdf.rs`） | 2026-09-20 起不用，2026-09-30 代码删除 | 漫画由 sheng-ren 出 EPUB |

## 服务与端口

![shelf 架构](docs/diagrams/architecture.svg)

| 服务 | 端口 | 职责 | 源码 |
|---|---|---|---|
| gateway | `0.0.0.0:443`，唯一对外 | HTTPS + 登录密码、网页 UI、反向代理、批量队列 | `../gateway` |
| book-serve | 127.0.0.1:8790 | 母版库：入库、加入 xochitl、下载/改名/删除；直接导入 xochitl（sheng-ren 用，不进母版库）；xochitl 回收站 / 建文件夹 / 漫画页边距的设备端代理（见书架白皮书第 C 章） | `services/book-serve` |
| ~~koreader-serve~~ | ~~127.0.0.1:8791~~ | 2026-09-29 退役：不再安装、网关不再代理 `/api/koreader/*`；重新部署时 `install.sh` 会清掉旧设备上的单元与二进制 | 源码已从仓库删除（2026-09-30），见 git 历史 |
| font-serve | 127.0.0.1:8792 | xochitl 字体上传即装（改写 fontconfig 中文回退链） | `../enhance/font-serve` |
| wallpaper-serve | 127.0.0.1:8793 | 休眠壁纸上传即用（写 xochitl 的 `SleepScreenPath` 键） | `../enhance/wallpaper-serve` |
| 笔记线四服务 | 8795–8798 | 见 [`../notes/README.md`](../notes/README.md) | `../notes` |

- 服务启动时往 `$XDG_RUNTIME_DIR/shelf/services/` 写一份注册信息，网关据此出标签页、按 `/api/<段>/*` 转发（`books`→book-serve、`fonts`→font-serve、`wallpapers`→wallpaper-serve；段与服务的对应只在 `../gateway/src/manage.rs` 的 `MODULES` 表里写一次）。装/卸一个服务 = 一个二进制 + 一个 systemd 单元。
- 全部接口清单见 [`docs/传书EPUB线架构.md`](docs/传书EPUB线架构.md) §9。
- systemd：`shelf.target` + 各服务 `PartOf=shelf.target`；`systemctl disable --now font-serve` 即拔掉字体服务。**绝不给 xochitl 加启动依赖**（曾因此变砖）。

## 访问与密码

- 地址 `https://shelf.local/`（网关自带 mDNS；**安卓不解析 .local**，见书架白皮书 §03j）或 `https://<设备IP>/`。**传大书走 USB 的 `https://10.11.99.1`**，WiFi 热点上行弱（书架白皮书 §03l）。
- 登录只要密码：首次默认 `shelf`，登录后强制改（≥6 位）。网页右上「改密码」，或设备上 `gateway passwd <新密码>`；忘记用 `gateway reset-password`。
- 证书由私有 CA 签发：登录页「下载 CA 证书」装进手机/电脑信任库一次，此后没有"不安全"提示。

## 目录

```
shelf/
├── Cargo.toml · build.sh · .cargo/   内部 workspace；aarch64 musl 全静态交叉编译
├── crates/shelf-conv/                只读不改书的读书工具：读 EPUB（书名、封面、漫画页边距标记）、第三方 PDF 页数、大文件通道占位文档、文件名规范化
├── services/book-serve/              母版库服务
├── systemd/                          shelf.target + book-serve 单元
├── xovi/                             注入 xochitl 的 qmd：字体菜单、回收站/建文件夹/漫画页边距代理、阅读器单击翻页
├── install.sh · uninstall.sh · manifest.sh   设备端安装/卸载与共用清单
└── docs/                             书架白皮书、传书线架构 + diagrams/
```

依赖单向无环：`services/* → ../rmsvc-core`；`book-serve → shelf-conv`。
2026-10-07 稍后起**不再依赖 sheng-ren 的 `bookconv`**（此前 git 依赖它读 EPUB），读 EPUB 用 `shelf-conv` 自己的 `epub` 模块。
有两处是从 sheng-ren 抄来的、**sheng-ren 改了这边要手动跟着改**：漫画页边距标记名 `READER_MARGINS_MARKER`（`META-INF/eink-reader-margins`），
书名规范化 `canonical_book_name`（`shelf-conv/src/naming.rs`）。没有自动检查，见书架白皮书 §03bx。
设备上的路径（XDG，HOME=/home/root）：母版库 `~/.local/state/shelf/books/staging/`；配置 `~/.config/shelf/<服务>.json`；
二进制 `~/.local/bin/`；安装备份 `~/cangjie-backups/shelf-<时间戳>/`（留最近 5 份）。完整路径表见书架白皮书附录 C。

## 构建 · 部署 · 卸载

一次性准备：`rustup target add aarch64-unknown-linux-musl`，再装 aarch64 交叉 gcc（Arch：`pacman -S aarch64-linux-gnu-gcc`，只用来编 `ring` 的 C 部分）。

```sh
cd shelf && sh build.sh                        # host 测试 + aarch64 构建（gateway / enhance / notes 在的话一起编）
cargo test --workspace                         # 只跑测试：2026-10-07 代码审查修复后实跑 book-serve 81 + shelf-conv 26 个通过
cd ../packaging && sh deploy.sh 10.11.99.1     # 打包 → 传到设备 → install.sh（先备份旧文件）；只有 WiFi 时给 WiFi IP
sh deploy.sh 10.11.99.1 --only font,wallpaper  # 只装部分服务；SHELF_NO_BUILD=1 跳过编译
sh deploy.sh 10.11.99.1 --password '新密码'     # 顺便设网关密码（经 ssh 标准输入传，不上命令行）
ssh root@10.11.99.1 '~/.local/bin/shelf-uninstall' [--only font] [--purge]
```

- `--only` 令牌：`gateway book font wallpaper ink transcribe mind note`（网关总会装；未知令牌 install.sh 退出码 2；`koreader` 2026-09-29 起不再是合法令牌）。清单的唯一事实源是 `manifest.sh` 的 `SHELF_ALL`。
- `install.sh` 依赖同目录的 `manifest.sh` 与 `devlib.sh`，`deploy.sh` 会一起打包，**只拷一个 install.sh 到设备不够**。安装幂等：先校验载荷，二进制/qmd 原子替换，只重启有变化或没在跑的服务。
- `--only` 部署也会刷新设备上的 `shelf-uninstall` 与它 source 的 `~/.local/lib/shelf/{manifest.sh,devlib.sh}`（2026-09-30 起；此前只有整包安装才刷，`--only` 部署新 qmd 后卸载会拿旧清单漏删）。
- 旧设备上的退役件（`koreader-serve`、网关旧名 `shelf-gateway` 等，见 `manifest.sh` 的 `SHELF_LEGACY_*`）在每次安装时顺手清掉。
- 整套设备增强（固件安全门、wifi-watch、enhance 扩展、书架、笔记线）一条命令：`packaging/install-all.sh <host>`，见 `../packaging/README.md`。
- ⚠ **qmd 改动怎么生效**：`install.sh` 只把 qmd 放到位、不重启（会打断阅读）；要生效就**整机重启**——电脑上跑 `packaging/deploy-xovi-apply.sh <设备>`，或设备上 `reboot`。2026-09-25 起**不再单独 `systemctl restart xochitl`**（xochitl 退出时自身有概率崩溃，再由系统整机重启）；xovi 已生效时**绝不**跑 `xovi/start`（2026-09-20 事故）。`deploy.sh` 只装不重启；`install-all.sh` 最后的 `xovi-apply` 步会在有改动时自动整机重启。详见书架白皮书第 F 章「速查」。

**固件升级（OTA）之后**：`/home` 数据不丢，但要重装功能。权威步骤在 [`../docs/INSTALL.md`](../docs/INSTALL.md)「固件升级（OTA）之后」：设备旁手动 `xovi/rebuild_hashtable`，再在电脑上重跑 `packaging/install-all.sh <host>`。

## 文档索引

![书架文档地图：带着问题找文档](docs/diagrams/sh-doc-map.svg)

| 文档 | 管什么 |
|---|---|
| [`docs/传书EPUB线架构.md`](docs/传书EPUB线架构.md) | **传书线架构（现状）**：书在各服务间怎么流动、母版库状态、落库通道、内存设计、并发与锁、全部 API 与配置 |
| sheng-ren 仓库 `docs/typesetting.md`、`docs/xochitl.md` | **书该被优化成什么样**：排版、注释、目录、漫画规则；xochitl 阅读器的实测踩坑（只认外链 CSS、书内跳转、目录查找等） |
| [`docs/reMarkable书架白皮书.md`](docs/reMarkable书架白皮书.md) | **现状总览 + 真机历史与坑**：各章"现状结论"、决策来由、事故与教训、待办、已砍能力（含 2026-10-07 删除的设备端优化） |
| [`../rmsvc-core/README.md`](../rmsvc-core/README.md) | 书架、笔记、系统增强、网关共用的 Web 服务底座（路径、注册、HTTP、上传、往 xochitl 投书） |

本仓库原有的《EPUB 优化规范白皮书》《bookconv 优化白皮书》2026-10-07 删除（规则在 sheng-ren），原文用 `git show 76fd031:shelf/docs/EPUB优化规范白皮书.md` 查。
