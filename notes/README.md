# notes —— reMarkable 笔记线

**一句话**：合上书后，把你在 xochitl（reMarkable 自带的阅读程序）里用荧光笔勾的原文和旁边的手写批注自动收进一个“条目库”；你在手机网页上挑选、改字、问 AI，再推送成设备上的笔记本或 Obsidian 用的 md。

- **给谁用**：用 reMarkable 读 EPUB、习惯边勾边写的人；以及要改这四个服务的开发者。
- **怎么开始**：整包安装 `sh packaging/install-all.sh <设备IP>` 后，打开网关网页的「笔记」标签；要转写手写、问 AI，先在「管理 → 模型管理」填一把模型厂商的 key。
- **想弄懂原理**：[`docs/reMarkable笔记白皮书.md`](docs/reMarkable笔记白皮书.md)——第 1–3 节讲是什么、怎么用，第 4 节讲怎么工作，第 8 节集中列出真机验证现状；整个系统的位置见 [`../docs/OVERVIEW.md`](../docs/OVERVIEW.md)。

## 能做什么

![一条笔记的旅程](docs/diagrams/overview.svg)

原则：**设备只负责写，不负责改；改在手机上做；条目库是唯一事实源，设备笔记本和 md 都是从它重新生成的投影。**

- **摄取**：合上书后，勾画与旁边手写自动配成一条“条目”；没写字的纯勾画也算一条；手写画成灰度裁图；按书的目录归章。
- **浏览**：逐条决定「转入笔记」还是「不需要」（已有定稿 / 草稿的条目转入时保留原来的进度）。
- **转写**：转入笔记的手写自动交给视觉模型识别，结果作为草稿，你在手机上改定。
- **整理**：改字、选去处（设备笔记本 / Obsidian / 两处）、问 AI、「不要了」进回收站（可恢复）。行首写 `##` / `###` / `1.` / `-` / `- [ ]`（或手写 `口`），推送后分别变成设备笔记本内置的大标题 / 加粗小标题 / 编号列表 / 圆点列表 / 复选框，导出 md 时是对应的 markdown（白皮书 3.3）。
- **推送**：每章一个「推送本章」，生成设备笔记本（一章一本，放进书本自己所在的文件夹）和 md（提示里给下载链接）。内容没变就跳过；旧版笔记本几秒内自动进 xochitl 回收站。
- **全文搜索**：跨所有书搜原文、转写、AI 回答。
- **导入 md 文档**（默认隐藏，「管理 → 实验室」打开）：选一个 `.md` 文件直接生成一份设备笔记本，不经条目库。

**模型**：网关「管理 → 模型管理」里视觉（转写）和文字（问 AI）各一张卡片。预置了 DashScope / OpenAI / Gemini / DeepSeek 四家（也可自定义 OpenAI 兼容地址），key 按厂商分开存，用量按模型分账。只有 DashScope 在真机上真调过，其余三家只验证了配置层（白皮书 3.4、5.6）。

## 组成

![笔记线的组成](docs/diagrams/architecture.svg)

四个服务都挂在网关（[`../gateway`](../gateway/README.md)）下、只监听 `127.0.0.1`，网关 `manage::MODULES` 里登记了四行。服务之间只经 HTTP 通信，**条目库只有 ink-serve 能写**。

| 服务 | 网关前缀 · 端口 | 职责 | 出网 |
|---|---|---|---|
| `ink-serve`（矿） | `/api/ink` · 8795 | 监听书库 → 解析变更页 → 配对 → 裁图 → 写条目库；全文搜索 | 否 |
| `transcribe-serve`（转写） | `/api/transcribe` · 8796 | 订阅 ink 事件，转写待转写的条目，草稿写回 | 是 |
| `mind-serve`（脑） | `/api/mind` · 8797 | 点「提问」时调文字模型，回答写回；没有后台任务 | 是 |
| `note-serve`（本） | `/api/notes` · 8798 | 注册「笔记」tab；生成设备笔记本、导出 md、单篇 md 导入 | 否 |

服务之间怎么调用见白皮书 4.1；完整接口表见白皮书 5.1。几条要点：

- 网页改字 `POST /api/ink/books/{uuid}/entries/{id}`；转写写草稿、问 AI 写回答走各自的命令端点 `…/draft`、`…/answer`。请求体类型在 `notecore::api`，收发两端共用，未知字段或形状不对回 400。
- `GET /api/ink/books/{uuid}` 每条条目带 `live`（是否进投影），只在应答里、不落盘。
- 其他服务读条目库都经 `notesvc::InkClient`：ink-serve 回 404 原样透传，ink-serve 没运行回 503。
- 「推送本章」回 `{chapters:[{chapter,title,status,…}]}`，`status` 为 `generated` / `unchanged` / `empty` / `failed`，失败原因在 `message`。

## 目录

```
notes/
├── crates/rmv6/          .rm v6 解析 + 写入（解析部分剥离移植自 remarkable_lines 0.1.3，MIT，见 PROVENANCE.md）
├── crates/epubmap/       .epubindex + EPUB 目录 → 页号对应的章 / 小节（EPUB 容器解析用 ../rmsvc-core/epubpkg）
├── crates/notecore/      领域核心（纯函数）：条目模型与状态转移、聚簇配对、增量合并、行首标记、投影、md 导出 / 导入、HTTP 契约类型 api
├── crates/vendorcfg/     两个 AI 服务共用：预置表、key 分厂商、配置迁移、用量账本、OpenAI 兼容调用端
├── crates/notesvc/       InkClient：transcribe / mind / note 访问 ink-serve 的唯一客户端
├── services/             ink-serve · transcribe-serve · mind-serve · note-serve
├── systemd/              四个 .service（PartOf=shelf.target）
├── testdata/             真机样本（renggu 墓碑页 / renggu_marks 勾画+手写 / seven_styles 七种打字样式）
└── docs/                 白皮书 + diagrams/（overview · architecture · data-flow · ingest · entry-status · marker-styles · model-config · organize-page · push-notebook）
```

网页在网关里：[`../gateway/ui/notes.js`](../gateway/ui/notes.js) 的 `renderNotes`（浏览 / 整理 / 回收站 / 导入 md 文档四个子视图，顶部是选书、重扫和搜索框）与 [`../gateway/ui/manage.js`](../gateway/ui/manage.js) 的 `mountModelPanel`（模型管理卡片）。

## 设备上的路径（XDG，HOME=/home/root）

| 用途 | 路径 |
|---|---|
| 二进制 | `~/.local/bin/{ink-serve,transcribe-serve,mind-serve,note-serve}` |
| 配置 | `~/.config/notes/{ink,transcribe,mind,note}.json`（含 key 的两份权限 0600；字段见白皮书 5.3） |
| 数据 | `~/.local/share/notes/crops/`（裁图）· `~/.local/share/notes/vault/`（md 导出） |
| 状态 | `~/.local/state/notes/books/<uuid>.json`（**条目库**）· `notebooks/`、`exports/`（每章推送记录）· 用量账本；任何一份 JSON 解析失败时，旁边另存 `*.json.corrupt`（只留第一份坏内容） |
| 只读 | xochitl 书库 `~/.local/share/remarkable/xochitl/`——**绝不写** |

## 构建、测试与部署

与书架共用交叉编译环境（`rustup target add aarch64-unknown-linux-musl` + aarch64 交叉 gcc），见 [`../shelf/README.md`](../shelf/README.md)。

```sh
cd notes && cargo test --workspace      # host：264 过 + 1 忽略（rmv6 29 · epubmap 10 · notecore 75 · notesvc 3 · vendorcfg 29 · ink 32 · transcribe 28 · mind 25 · note 33 另 1 个 ignored；2026-10-10 实跑）
cd ../shelf && sh build.sh               # 各项目 host 测试 + 交叉编译（notes/ 在就一起编；服务清单取自 shelf/manifest.sh）
cd ../packaging && sh deploy.sh <设备IP> --only ink,transcribe,mind,note   # 只装/更新笔记线（网关总会一起装）；不加 --only 就全装
```

卸载走 `packaging/uninstall-all.sh` 或书架 `uninstall.sh`；条目库不在书架 `--purge` 范围内。

## 验证现状与还没做的

完整表格见白皮书第 8 节，要点：

- **2026-10-10 重构（两个阶段）未部署**：状态转移收进 `Entry`、`notecore::api` 契约、命令端点、`live`、`InkClient`、上传认领迁到基座 `upload_and_claim`、页表改用基座 `PageTable`、错误状态码、失败字段 `message` 等。四个服务和网关须一起部署（白皮书 8.2 #7h、#7i）。
- 10-09 第六轮审计（笔记本落进书所在文件夹、调云重试与闲置连接重建、epubmap 解 XML 实体等）**已部署、功能未手测**；09-30 第五轮、09-25 第四轮的改动同样已部署、没逐项真机核（白皮书 8.2 #7d、#7e、#7g）。⚠ 书在文件夹里时，笔记本从 09-09 起其实一直落在书库根，10-09 才修好，落点还没在真机上手测。
- 网页渲染和交互没有人眼确认过（后端链路都真机走通了）。
- 转写准确率：汉字数字常被认成阿拉伯数字；手写 `##` 标记还没有一次被正确识别的真机例子。
- OpenAI / Gemini / DeepSeek 没用真实 key 调用过；全文搜索没用真实数据核对；书的小节名插行只有单测。

用户已决定不改的：编号列表被〔原文〕/〔AI〕段打断会从 1 重来；没转写的条目写“（待转写）”占位。
