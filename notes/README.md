# notes —— reMarkable 笔记线

> 设计决策、真机验证记录和踩坑都在 [`docs/reMarkable笔记白皮书.md`](docs/reMarkable笔记白皮书.md)（开头有“5 分钟读懂”和“现状总览”）；整个系统的位置见 [`../docs/OVERVIEW.md`](../docs/OVERVIEW.md)。

## 能做什么

reMarkable 的强项是**荧光笔勾书，再在勾出来的地方旁边手写**。笔记线在你合上书后，把这些勾画和手写自动收进一个**条目库**；你在手机网页上挑选、改字、按条目问 AI，然后按每条的“去处”推送成设备上的笔记本、Obsidian 用的 md，或两处都要。

原则：**设备只负责写，不负责改；改在手机上做；条目库是唯一事实源，设备笔记本和 md 都是从它重新生成的投影。**

![笔记线一图读懂](docs/diagrams/overview.svg)

- **摄取**：勾画与旁边手写自动配对；没写字的纯勾画也算一条；手写画成裁图。
- **浏览**：逐条决定「转入笔记」还是「不需要」。
- **转写**：转入笔记的手写自动交给视觉模型识别，结果作为草稿，你在手机上改定。
- **整理**：改字（行首写 `-`、`1.`、`口` 自动变成列表或待办）、选去处、问 AI、「不要了」进回收站（可恢复）。
- **推送**：每章一个「推送本章」，生成设备笔记本（放进书本自己所在的文件夹，一章一本）和 md（同时让浏览器下载）。内容没变就跳过。
- **KOReader 回流**：把 KOReader 里的高亮和生词并入条目库，走同样的流程。
- **全文搜索**：跨所有书搜原文、转写、AI 回答。
- **导入 md 文档**（默认隐藏）：选一个 `.md` 文件直接生成一份设备笔记本，不经条目库。

## 四个服务

都是挂在网关（[`../gateway`](../gateway/README.md)）下、只监听 `127.0.0.1` 的服务；网关 `manage::MODULES` 里登记了四行。服务之间只经 HTTP 通信，**条目库只有 ink-serve 能写**。

| 服务 | 网关前缀 / 端口 | 职责 | 出网 |
|---|---|---|---|
| `ink-serve`（矿） | `/api/ink` · 8795 | 监听书库 → 解析变更页 → 配对 → 裁图 → 写条目库；KOReader 回流；全文搜索 | 否 |
| `transcribe-serve`（转写） | `/api/transcribe` · 8796 | 订阅 ink 事件，只转写 `Pending` 条目，草稿写回 | 是 |
| `mind-serve`（脑） | `/api/mind` · 8797 | 点「提问」时调文字模型，回答写回；没有后台任务 | 是 |
| `note-serve`（本） | `/api/notes` · 8798 | 注册「笔记」tab；生成设备笔记本、导出 md、单篇 md 导入 | 否 |

**模型**：在网关「管理」tab 的“模型管理”卡片里选，视觉和文字各一张。预置了 DashScope / OpenAI / Gemini / DeepSeek 四家，key 按厂商分开存，用量按模型分账。只有 DashScope 真机调用过，其余三家只验证了配置层。详见白皮书第 6 章。

![notes 数据流](docs/diagrams/data-flow.svg)

## 主要 API（经网关前缀 `/api/<seg>`）

| 服务 | 路由 |
|---|---|
| ink | `GET /books`（只列还有活条目的书）· `GET /books/{uuid}` · `GET /books/{uuid}/crops/{file}` · `POST /books/{uuid}/entries/{id}`（改 `text` / `style` / `destination` / `draft` / `answer` / `askAi` / `question` / `subheadHint`；终态条目拒改）· `POST …/entries/{id}/request`（转入笔记）· `…/skip`（不需要）· `…/archive`（不要了）· `…/restore`（恢复）· `POST /books/{uuid}/purge`（清空回收站，不可恢复）· `POST /books/{uuid}/rescan` · `POST /koreader/import` · `GET /search?q=&limit=` · `GET /events` |
| transcribe | `GET /status` · `GET /config` · `PUT /config`（`preset` / `apiKey`（只写）/ `clearKey` / `price` / 自定义 `model`+`baseUrl` / `auto` / `maxPerRun` / `pauseMs` / `timeoutSecs` / `maxAttempts` / `prompt`）· `POST /run` · `POST /books/{uuid}/entries/{id}`（强制转写一条，返回 token 用量）· `POST /retry` · `GET /events` |
| mind | `GET /status` · `GET /config` · `PUT /config`（同上，没有节流字段）· `POST /books/{uuid}/entries/{id}/ask`（要求已勾「问 AI」且问题非空）|
| notes | `GET /status` · `GET /books` · `GET /books/{uuid}/notebooks` · `GET /books/{uuid}/exports` · `GET /books/{uuid}/sync`（每章两个去处的同步状态）· `POST /books/{uuid}/generate` · `POST /books/{uuid}/chapters/{idx}/generate` · `POST /books/{uuid}/export` · `POST /books/{uuid}/chapters/{idx}/export` · `GET /books/{uuid}/chapters/{idx}/export.md`（浏览器下载）· `GET /books/{uuid}/vault.json`（读回已导出的 md）· `POST /books/{uuid}/import-md {title, markdown}` · `GET /events` |

事件区域都是 `notes`：ink 发 `entries`，transcribe 发 `transcribe`，note-serve 发 `notebooks`；网页据此自动刷新。

## 目录

```
notes/
├── crates/rmv6/          .rm v6 解析 + 写入（解析部分剥离移植自 remarkable_lines 0.1.3，MIT，见 PROVENANCE.md）
├── crates/epubmap/       .epubindex + 目录 → 页号对应的章/小节
├── crates/notecore/      领域核心（纯函数）：条目模型、聚簇配对、增量合并、KOReader 合并、行首标记、投影、md 导出/导入
├── crates/vendorcfg/     两个 AI 服务共用：预置表、key 分厂商、配置迁移、用量账本、OpenAI 兼容传输
├── services/             ink-serve · transcribe-serve · mind-serve · note-serve
├── systemd/              四个 .service（PartOf=shelf.target）
├── testdata/             真机样本（renggu 墓碑页 / renggu_marks 勾画+手写 / seven_styles 七种打字样式）
└── docs/                 白皮书 + diagrams/（overview · architecture · data-flow · entry-status · model-config · organize-page）
```

网页在网关里：[`../gateway/ui/app.js`](../gateway/ui/app.js) 的 `renderNotes`（浏览 / 整理 / 回收站 / 导入 md 文档四个子视图，顶部是选书、重扫、KOReader 回流和搜索框）与 `mountModelPanel`（模型管理卡片）。

## 设备上的路径（XDG，HOME=/home/root）

| 用途 | 路径 |
|---|---|
| 二进制 | `~/.local/bin/{ink-serve,transcribe-serve,mind-serve,note-serve}` |
| 配置 | `~/.config/notes/{ink,transcribe,mind,note}.json`（含 key 的两份权限 0600） |
| 数据 | `~/.local/share/notes/crops/`（裁图）· `~/.local/share/notes/vault/`（md 导出） |
| 状态 | `~/.local/state/notes/books/<uuid>.json`（**条目库**）· `notebooks/`、`exports/`（每章推送记录）· 用量账本 |
| 只读 | xochitl 书库 `~/.local/share/remarkable/xochitl/`——**绝不写** |

## 构建与部署

与书架共用交叉编译环境（`rustup target add aarch64-unknown-linux-musl` + aarch64 交叉 gcc），见 [`../shelf/README.md`](../shelf/README.md)「构建」。改代码前先看工程纪律；在 master 上开 feature 分支开发。

```sh
cd notes && cargo build --workspace && cargo test --workspace     # host：214 个测试（rmv6 27 · epubmap 5 · notecore 62 · vendorcfg 20 · ink 21 · transcribe 23 · mind 22 · note 34，含 1 个 ignored）
cd ../shelf && ./build.sh && ./deploy.sh <设备IP>                  # 随书架一起交叉编译、打包、装机
ssh root@<设备IP> sh /home/root/shelf-pkg/shelf/install.sh --only ink,transcribe,mind,note   # 只装/更新笔记线
```

卸载走书架 `uninstall.sh`；条目库不在 `--purge` 范围内。

## 还没做的

完整清单见白皮书第 13 章，要点：

- 网页渲染和交互没有人眼确认过（后端链路都真机走通了）；
- 转写准确率：汉字数字常被认成阿拉伯数字，手写 `##` 标记还没有一次被正确识别的真机例子；
- OpenAI / Gemini / DeepSeek 没用真实 key 调用过；
- 全文搜索没用真实数据核对，且有两个已知问题（草稿字段取的是最早一份；点结果跳到「浏览」看不到已转入笔记的条目）；
- 「不要了」和「清空回收站」没对真实数据执行过；几处被动触发的修复没在真机复现。
