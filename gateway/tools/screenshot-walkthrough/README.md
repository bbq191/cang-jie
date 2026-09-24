# 前端可视渲染走查工具

**它解决什么**：网页前端（母版库上传、笔记「浏览/整理/回收站」等）反复迭代，但以前只验证“数据链路对不对”（curl API、单测），没人眼看过浏览器里实际长什么样。i18n 漏翻译、排版错位、字段显示错这类问题，数据测试测不出来。这个工具把“起服务 + 灌数据 + 过一遍浏览器截图”变成一条命令。

**它不是断言测试**：没有通过/失败判定，截图要人去看。唯一能自动判断的是 `console-errors.json`：页面 JS 错误和 4xx/5xx 响应（已过滤掉“服务没起”这类预期内的 404），出现新条目就值得看一眼。

## 用法

```sh
npm install                       # 装 Playwright（一次性）
npx playwright install chromium   # 下载浏览器（一次性，不进 git，约 300MB）
./run.sh                          # 编译 + 起服务 + 灌数据 + 走查 + 清理，截图落 ./shots/
./run.sh /tmp/my-shots            # 换个输出目录
```

`run.sh` 做的事：

1. **端口守卫**：本机 8790 / 8795 / 8798 / 8778 任一已被占用（开发机上可能有真服务在跑）就拒绝运行，免得 fixture 打进真实数据。
2. 在 `mktemp -d` 出来的临时 XDG 目录里编译并起 `book-serve`、`ink-serve`、`note-serve`（host debug 构建），等 book-serve 真的就绪。
3. 灌 fixture（见下）。
4. 用 `gateway passwd` 设一个非默认密码（跳过首登必改），在 `127.0.0.1:8778` 起网关。
5. 跑 `walk.mjs`：登录，中文、英文各一遍（改 `shelf.lang` localStorage 再刷新），点遍顶层标签和一层子标签，逐张全页截图。

全程不碰真实 `~/.config` / `~/.local`；跑完（含失败、Ctrl-C）用 `trap` 清掉起的进程和临时目录。

## fixture 数据

- `fixtures/make_fixture_epub.py`：生成两本最小合法 EPUB（占位正文），走 book-serve 真实的 `POST /staging` 上传接口进母版库——练的是真实上传路径。
- `fixtures/make_fixture_book.py`：生成一份笔记条目库 JSON（9 条，覆盖 Mined/Pending/Draft/Reviewed/Skipped/Revoked/Archived 全部 7 种状态 + 一条带“问 AI”的），直接写进 ink-serve 的状态目录 `$XDG_STATE_HOME/notes/books/<uuid>.json`。这是**绕开真实 `.rm` 摄取管线的测试夹具**，字段形状照抄 `notes/crates/notecore/src/model.rs` 手写；选它是因为条目库落盘就是普通 JSON，比等真机摄取好控制。

## 覆盖范围与局限

- **能走到的**：传书（入库 / 母版库）、笔记（浏览 / 整理 / 回收站）、管理（基石与模块 / 模型管理 / 系统增强 / 实验室），以及「整理」里未导出/已导出切换（`[data-etab]`）的默认档。
- **走不到的**：只起了 book-serve / ink-serve / note-serve 三个后端，font-serve、koreader-serve、wallpaper-serve、transcribe-serve、mind-serve 没起——“其他”标签（xochitl 字体 / KOReader / 壁纸）不会出现，转写和问 AI 面板也看不到。它们要么需要真实外部依赖（AI API key），要么 fixture 成本更高，属于有意的范围裁剪。
- **不递归进第三层**：「整理」的章节切换、「浏览」按书切换的下拉只截默认状态。要扩大覆盖，往 `walk.mjs` 里加一层循环即可。
- **不是像素级回归**：没有和上一次截图比对的机制，适合“改完前端跑一遍、自己瞄一眼”，不能进 CI 自动判定。

## 实战记录

2026-09-16 第一次真跑就抓到一个前端 bug：`cropHtml()` 把“有手写但裁图渲染失败”误判成“纯勾画没有手写”（见 `gateway/ui/app.js` 里 `notes.cropMissing` 相关改动的提交信息）。
