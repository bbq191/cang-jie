# 全貌：10 分钟看懂这套系统

> **读者与用途**：第一次接触这个仓库的人（使用者、贡献者、接手维护的人）。读完你应该能回答：
> 这套东西解决什么问题、由哪几块组成、一本书怎么从手机走到 reMarkable 上、各服务跑在哪个端口、东西装在设备的哪里、去哪里找细节。
> 只想装：直接看 [`INSTALL.md`](INSTALL.md)；最近改了什么：[`CHANGELOG.md`](CHANGELOG.md)。

## 1. 它解决什么问题

reMarkable Paper Pro Move 是一台彩色墨水屏平板，官方阅读/笔记应用叫 **xochitl**（读作"沙特尔"，设备上的官方主程序）。日常用起来有几处不顺手：

- **书难弄进去**：网页上传有约 100MB 上限，传进去之后建文件夹、删书、查渲染结果都不方便；漫画留白多。（很多 EPUB 在墨水屏上排版不理想——这件事 2026-10-07 起交给姊妹项目 [sheng-ren](https://github.com/bbq191/sheng-ren) 在电脑上做，本仓库不再改书。）
- **中文与手写场景欠缺**：荧光笔划中文会"吸一整行"；手写笔画不像中文书写（曾做过手写笔画优化，2026-09-30 已移除）；没有把"勾画 + 旁边手写批注"变成可整理笔记的通道。
- **系统层面的小痛点**：字体/壁纸不能自己上传、耗电难排查（曾做过耗电诊断「电池刺客」，2026-09-30 已移除）、时区与校时不合国内环境。

这个项目**不修改 xochitl 本体**，而是用两种"旁路"手段增强它：一是 **xovi 扩展**（xovi 是第三方的扩展加载框架，能在 xochitl 启动时加载我们的小插件）；二是一组**跑在设备上的独立 Web 服务**，通过网页操作，需要时再借 xochitl 自己的上传接口、直接读写它的书库目录，或用 **qmd**（对 xochitl 界面 QML 描述文件的补丁）往界面里注入少量入口。

## 2. 七个目录一览（按"运行形态"分）

功能清单和各线入口见 [根 README](../README.md#能做什么)，这里只讲每个目录**以什么形态跑在设备上**：

| 目录 | 一句话 | 运行形态 |
|---|---|---|
| [`shelf/`](../shelf/README.md) 书架 | 电脑上用 sheng-ren 优化好的书（EPUB/PDF）上传 → 母版库 → 原样加入 xochitl | 1 个 Web 服务（book-serve）+ 4 个 qmd 界面补丁（回收站代理、建文件夹代理、漫画页边距代理、阅读器翻页）。旧的 `koreader-serve` 已退役（2026-09-29 设备卸了 KOReader） |
| [`notes/`](../notes/README.md) 笔记线 | 荧光笔勾画 + 旁边手写批注 → 手机整理/转写/问 AI → 投回设备笔记本或 Obsidian | 4 个 Web 服务（ink / transcribe / mind / note） |
| [`enhance/`](../enhance/README.md) 系统增强 | 荧光笔 CJK 精确吸附、阅读器单击翻页（日漫翻页规则 2026-10-07 删除）、字体/壁纸上传即用（手写笔锋渲染、电池诊断 2026-09-30 已移除） | 1 个 xovi 扩展（hl-snap）+ 2 个 Web 服务（font / wallpaper）+ 字体菜单 qmd；翻页补丁随 shelf 一起装 |
| [`gateway/`](../gateway/README.md) 网关 | 上面三条线共用的唯一对外入口：HTTPS + 登录密码 + 反向代理 + 批量队列 | Web 服务（`0.0.0.0:443`） |
| [`rmsvc-core/`](../rmsvc-core/README.md) 服务基座 | 各 Web 服务共用的基础库，不含业务逻辑 | Rust 库（编进各服务，不单独运行） |
| [`defw/`](../defw/README.md) 固件逆向 | xochitl 3.28.0.172 的 Ghidra 逆向产物，给扩展定位 hook 用 | 逆向资料（不上设备） |
| [`packaging/`](../packaging/README.md) 安装器 | 全新设备一条命令装完（含固件兼容性校验），以及对称的卸载 | host 侧 shell 脚本（在你的电脑上跑，经 ssh 装到设备） |

## 3. 整体架构与部署

![整体架构与部署拓扑](diagrams/architecture-topology.svg)

要点：

- **所有服务都跑在设备上**，用浏览器访问 `https://10.11.99.1/`（USB 连接时）或 `https://shelf.local/`（同一 WiFi 下，安卓不解析 `.local`）。
- **只有网关对外**（`0.0.0.0:443`）。它用设备自己生成的私有 CA 签 HTTPS 证书，再加登录密码（默认 `shelf`，首次登录强制改）。领域服务只听 `127.0.0.1`，由网关按"服务注册表"转发。装/卸一个服务 = 一个二进制 + 一个 systemd 单元。
- **安全上的两条限制**（2026-09-24 起）：私有 CA 只能给局域网名字和内网 IP 签证书，即使设备上的 CA 私钥泄露，也伪造不了别的网站；登录输错按来源 IP 分别限速（每个 IP 60 秒内错 5 次就锁这个 IP），同一 WiFi 下别人乱试不会把你锁在外面。从旧版升级时网关会自动换一张新 CA，**手机和电脑要重装一次证书**，见 [`INSTALL.md`](INSTALL.md#装完之后)。
- **xochitl 阅读器单击翻页**（2026-09-24，「管理 → 系统增强」开关，默认关，真机验证过）：点屏幕左右边缘翻页，每次打开书读一次开关。原来还有「日漫翻页规则」（按书里的从右往左标记对调滑动方向），2026-10-07 用户定书架只管入库、方向交给书本身，整个删掉：xochitl 自己不看这个标记，所以日漫在 xochitl 里一律从左往右翻（未合 master、未部署）。
- **扩展和界面补丁怎么生效**：改动先落盘，再**整机重启一次**（约 20–60 秒）。不单独重启 xochitl：它退出时有概率崩溃（xochitl 自身的问题，2026-09-25 查清）。安装脚本会自动做这件事，重启回来后自动核对；细节见 [`INSTALL.md`「重复运行」](INSTALL.md#重复运行什么时候才重启)。
- **开关开了不等于生效**：网页「管理」页在每个扩展开关旁显示"已加载 / 未加载"，（实验室的漫画页边距开关 2026-10-07 删除。）数据直接读运行中 xochitl 进程加载了哪些文件。
- **设备健康与 OTA 提示**（2026-09-25，已部署并用真实登录看过页面；清理按钮还没在真机点过）：「管理 → 设备健康」分五个小标签（概览 / 服务 / 扩展 / 日志 / 清理），按需显示各服务状态、内存、启动耗时、xovi 是否生效、换了文件还没重启的扩展、上次开机最后 20 行日志。固件升级冲掉服务单元或 xovi 没生效时，网页顶部提示并给出恢复命令。「清理」可以删早期遗留文件；xochitl 书库里的重复副本勾选后移进 xochitl 自己的回收站。
- **移进回收站、新建文件夹由 xochitl 里的小代理执行**：这两件事不直接改书库文件，而是由 book-serve 排进队列；xochitl 里常驻的界面补丁（`shelf-trash-agent.qmd`、`shelf-mkdir-agent.qmd`）长轮询取任务，调用 xochitl 自己的接口去做，几秒内生效，跟你正在看哪个文件夹无关。
- **服务与端口一览**（设备上 `/usr/lib/systemd/system/` 里共 **12 个**本项目的 systemd 单元）：

  | 单元 | 形态 | 端口 / 作用 |
  |---|---|---|
  | `gateway` | 常驻 Web 服务 | `0.0.0.0:443`，唯一对外入口 |
  | `book-serve` | 常驻 Web 服务 | `127.0.0.1:8790` 书架 |
  | `font-serve` · `wallpaper-serve` | 常驻 Web 服务 | 8792 · 8793 |
  | `ink-serve` · `transcribe-serve` · `mind-serve` · `note-serve` | 常驻 Web 服务 | 8795 · 8796 · 8797 · 8798（笔记线） |
  | `shelf.target` | 分组 | 上面 8 个服务一起启停 |
  | `wifi-watch` | 常驻脚本 | WiFi 假死看护，不听端口 |
  | `xovi-reenable` · `chrony-boot-wakelock` | 开机跑一次 | 开机让 xovi 重新生效 · 开机头一小段不让设备休眠、等校时 |

  8791、8794 空着（8791 原属已退役的 `koreader-serve`）。领域服务只听回环，都经网关访问。2026-09-30 真机：9 个常驻进程开机后 7–9 秒起齐，各占 1–4.5MB 内存。
- 只支持 **reMarkable Paper Pro Move、固件 3.28.0.172**；`/home` 数据在固件升级后保留，`/usr`、`/etc` 里的东西会被冲掉，要重新安装（见 [`INSTALL.md`](INSTALL.md#固件升级ota之后)）。

以前装过、现在已移除的组件（电池刺客、手写优化、KOReader 服务与侧栏入口）在设备上留下什么、怎么清，见 [`INSTALL.md`「从旧版升级」](INSTALL.md#从旧版升级清理已移除的组件)；重跑 `install-all.sh` 会自动清掉前两样。

开机时谁先谁后（xochitl 永远不等我们；常驻服务排在 xovi 恢复之后、不等网络）见 [`packaging/README.md`「开机启动顺序」](../packaging/README.md#开机启动顺序2026-09-24)。

### 东西装在设备的哪里

| 内容 | 位置 | 固件 OTA 后 |
|---|---|---|
| 各服务的二进制（`gateway`、`book-serve` …） | `/home/root/.local/bin/` | 保留 |
| 各服务的数据 / 配置 / 状态（母版库、证书、密码、字体壁纸池…） | `~/.local/share/shelf`、`~/.config/shelf`、`~/.local/state/shelf`；笔记线同理放在 `~/.config/notes`、`~/.local/share/notes`、`~/.local/state/notes` | 保留 |
| 上传途中的暂存文件 | 字体、壁纸在 `~/.local/state/shelf/upload/`（2026-09-25 起放在 `/home`、装好时直接改名；以前在内存里的 `/tmp`，会计入服务的内存上限）。服务启动时清掉上次中断留下的半成品 | 保留 |
| xovi 扩展 `.so`（`hl-snap`） | `/home/root/xovi/extensions.d/`（**只放扩展，备份绝不能放这里**：xovi 会把目录里每个文件都当扩展加载） | 文件保留，重跑安装后生效 |
| 等着换入的新版扩展 `.so`（xochitl 正在用旧版时先放这里；整机重启前由部署脚本、或开机时由 `xovi-reenable` 换进 `extensions.d/`） | `/home/root/.cangjie-stage/so-pending/` | 保留 |
| 界面补丁 qmd（字体菜单、回收站/建夹代理、漫画边距代理、阅读器翻页） | `/home/root/xovi/exthome/qt-resource-rebuilder/` | 文件保留，要先在设备上重建 hashtable 再重跑安装 |
| systemd 单元（共 12 个，见上面服务一览） | `/usr/lib/systemd/system/` | **被冲掉**，重跑安装 |
| 国内 NTP、默认时区 | `/etc` | **被冲掉**，重跑安装 |
| 安装前的旧文件备份（每类保留最近 5 份） | `/home/root/cangjie-backups/` | 保留 |
| 扩展共享开关文件 | `~/.local/share/cangjie-ime/reading-qol.json`（沿用旧目录名，历史原因） | 保留 |

## 4. 一本书的旅程（shelf）

![一本书的旅程](../shelf/docs/diagrams/book-journey.svg)

0. **在电脑上优化**（不在本仓库）：用 [sheng-ren](https://github.com/bbq191/sheng-ren) 的 booklib，按 `xochitl` 阅读模式把书优化成 EPUB（排版、目录、注释、封面、漫画补白都在这一步）。书架 2026-10-07 起**不再改书**：设备上的「优化」、PDF 转 EPUB、抓网文、补封面都删了。
1. **入库**：网页上传，或 scp 进设备的 `inbox/`。只收 EPUB/PDF，字节原样不动；同名同内容的书不重复存；书名整理成 `书名 - 02卷`（数字在前）。书进入**母版库**（设备上的暂存池，永久保留，可反复加入）。只勾选一本时可以"下载原件"或"改名"（只改文件名，不改书里的书名）。书架不管翻页方向（xochitl 里日漫一律从左往右翻）。
2. **加入 xochitl**：在母版库勾选，点「加入 xochitl」，可选放进哪个文件夹（不存在就让 xochitl 自己建）。≤90MB 走 xochitl 自己的网页上传接口；>90MB 走**占位 + 磁盘替换**（见下）。投完自动检查 xochitl 渲染出的页数是否合理，给出徽章。sheng-ren 优化的漫画带页边距标记，加入后首次打开会自动把页边距设到最小（没有开关；不想要的在阅读器里自己调回，每本只设一次）。
3. **批量**：在母版库勾选多本，底部批量栏一键排队；网关在后台顺序逐本执行，关掉浏览器也会接着跑。

## 5. 三个值得知道的机制

**占位 + 替换（绕开 xochitl 约 100MB 上传上限）**——先用网页接口传一个几 KB 的"替身"文档（带真书名和封面）让 xochitl 建好条目，再由设备上的 book-serve 把磁盘上那个文件原子替换成真文件。真机验证过 PDF 154MB、EPUB 153MB。上限 1GB；超过或造不出占位时整本拒绝，不再拆成几卷（按卷拆分 2026-09-30 已移除）。

![占位 + 替换](diagrams/upload-limit-bypass.svg)

**批量队列**——批量由**网关**执行而不是浏览器：一次一本，等这本在 book-serve 那边真加完才取下一本，网页的「加入 xochitl」只走这条队列；状态落盘 `state/batch.json`，网关重启后从被打断的那本续跑；"全部中止"清掉还没开始的；正在"出队、交给 book-serve"那一刻的这本也拦得住（2026-09-30 补上），已经在上传的那本会传完。现在只有「加入 xochitl」一种任务（"加入 KOReader" 09-30 撤、"优化" 10-07 撤）。

![批量队列状态机](../gateway/docs/diagrams/batch-queue.svg)

**并发闸门（2026-10-07 删除，未部署）**——当年设备上优化大书时几本一起跑会内存叠加，网关加过一道按体积限并发的闸门（>90MB 同时 1 本、其余 3 本）。书架不再优化书后，加入 xochitl 是流式上传、只占几 MB，批量队列本来就一本一本来，闸门已经拦不到东西，删掉；"等这本书真加完"的轮询（连续失败 6 次才放弃）挪进了批量队列。经过见网关白皮书 §04。

## 6. 笔记线在做什么（一句话）

![笔记线一图读懂](../notes/docs/diagrams/overview.svg)

你在书上用荧光笔划出内容、并在旁边手写批注；合上书后，`ink-serve` 把"勾画 + 手写"按距离配成条目存进条目库（手写离勾画 160 以内算这条勾画的批注，更远算本页批注）；你在手机网页里校对转写（`transcribe-serve` 调视觉模型）、选去向、需要时单条问 AI（`mind-serve`）；`note-serve` 再把条目投影回设备笔记本或导出 Obsidian markdown。

生成的设备笔记本沿用设备自带的文字样式：行首 `##` 是大标题、`###` 是小标题、`1.` 编号、`-` 圆点、`- [ ]` 复选框，还会按书的小节插入小节名；导出 Obsidian 时是对应的 Markdown。

「笔记」页顶部有跨书全文搜索（勾画原文、转写、提问、AI 回答、书名）。（「导入 KOReader 批注」按钮 2026-09-30 随 KOReader 卸载一起删了。）条目库文件损坏时不会被当成空书覆盖，会另存一份 `.corrupt` 副本并拒绝写入，等人处理。详见 [`notes/README.md`](../notes/README.md)。

## 7. 术语表

| 术语 | 一句话解释 |
|---|---|
| xochitl | reMarkable 官方主程序（阅读器 + 笔记 + UI）；本项目不改它 |
| xovi / 扩展 | 第三方的扩展加载框架；我们的 `hl-snap` 是它加载的 `.so` 插件（`handwriting-stroke` 2026-09-30 已移除） |
| qmd / qmldiff | 对 xochitl 界面 QML 的补丁语言/文件；用来往界面里加字体菜单、回收站/建文件夹代理、单击翻页等 |
| vellum | 设备上的包管理器，用来装 xovi、qt-resource-rebuilder 等生态组件 |
| 母版库 | shelf 里的暂存池：入库的书原样保存在这里，加入 xochitl 都从它出发 |
| 边车（sidecar） | 母版库里每本书旁边的 `.<书名>.delivered` 小文件，记录"什么时候加入过 xochitl、加入结果、渲染自检结果" |
| 占位文档 | 为绕开上传上限先传的几 KB 替身，之后被替换成真文件 |
| hook | 在 xochitl 某个函数入口"插一脚"：先跑我们的代码，再决定是否调用原函数；`hl-snap` 就是这样改行为的（已移除的 `hw-stroke` 也是） |
| 私有 CA | 设备自己生成的证书颁发机构，给网关签 HTTPS 证书；手机/电脑装一次它的证书，浏览器就不再报"不安全" |
| OTA | 固件在线升级；会整体替换 `/usr`、`/etc`，不动 `/home` |
| 注册表 | 服务启动时写的一份 JSON，网关据此出 tab 和转发 |
| SSE | 服务器推送事件；网页靠它即时刷新，不用一直轮询。网关重启后网页会自己重连（登录失效就跳到登录页） |

## 8. 接下来读什么

| 想了解 | 去读 |
|---|---|
| 怎么装 / 卸 / 升级后恢复 | [`INSTALL.md`](INSTALL.md)（脚本内部结构见 [`../packaging/README.md`](../packaging/README.md)） |
| 书架细节 | [`../shelf/README.md`](../shelf/README.md) → [`传书线架构`](../shelf/docs/传书EPUB线架构.md)（现状）→ [书架白皮书](../shelf/docs/reMarkable书架白皮书.md)（决策与真机记录） |
| 书怎么被优化 | 2026-10-07 起不归本仓库：用 [sheng-ren](https://github.com/bbq191/sheng-ren)（booklib，`xochitl` 阅读模式）在电脑上优化；排版规则看它的 `docs/typesetting.md`，xochitl 阅读器的实测踩坑看 `docs/xochitl.md`。本仓库以前的设备端优化（v16 及以前）与 PDF 转换的文档已删除，见 git 历史 |
| 网关、批量队列 | [`../gateway/README.md`](../gateway/README.md) · [网关白皮书](../gateway/docs/reMarkable网关白皮书.md) |
| 笔记线 | [`../notes/README.md`](../notes/README.md) |
| 系统增强 | [`../enhance/README.md`](../enhance/README.md) |
| 最近改了什么 | [`CHANGELOG.md`](CHANGELOG.md) |
