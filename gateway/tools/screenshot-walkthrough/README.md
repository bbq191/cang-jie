# 前端可视渲染走查工具

补的缺口：`shelf`/`notes` 两条线在网页前端上反复迭代（母版库上传、笔记「浏览/整理/回收站」等），
但历次改动只验证过"数据链路对了"（curl API、host 单测），**从没有人眼看过浏览器里实际渲染出来
长什么样**——这类问题（i18n 漏翻译、CSS 排版错位、字段展示逻辑错误）纯数据链路测试测不出来，
之前只能靠"配 Reddit 宣传截图时顺手发现"这种偶然路径（见 `shelf/docs/reMarkable书架白皮书.md`
关于 i18n 审计方法论那段）。这工具把"起服务+灌数据+过一遍浏览器"变成一条可重复命令。

**不是自动化断言测试**——没有"通过/失败"判定，截图仍然要人去看；价值在于把"走查这一遍"的固定
劳动（起四个服务、造两本书+一份条目库、登录、切语言、点遍每个 tab）自动化掉，把人力集中在"看图"
这一步。控制台/HTTP 4xx-5xx 错误会额外收集进 `console-errors.json`，这部分可以自动判断有没有
新噪音（过滤掉"服务没起"这类预期内的 404）。

## 用法

```sh
npm install                       # 装 Playwright（一次性）
npx playwright install chromium   # 下载浏览器二进制（一次性，不进 git，~300MB）
./run.sh                          # 编译+起服务+灌数据+走查+清理，截图落 ./shots/
./run.sh /tmp/my-shots            # 换个输出目录
```

全程隔离在 `mktemp -d` 出来的临时 XDG 目录，不碰真实 `~/.config`/`~/.local`；跑完（含失败/
Ctrl-C）都会 `trap` 清掉起的四个进程和临时目录。只起 `book-serve`/`ink-serve`/`note-serve`/
`gateway` 四个服务（`shelf`/`notes` 两条线里最核心、能造出有意义 fixture 的部分）——`font-serve`/
`koreader-serve`/`wallpaper-serve`/`transcribe-serve`/`mind-serve` 没起，这几个 tab/面板走查
不到，见下面「已知局限」。

## 走查什么

- 中文/英文各一遍（`shelf.lang` localStorage，同 `langsel` 下拉框行为）。
- 顶层 nav：传书（含入库/母版库两个子 tab）、笔记（浏览/整理/回收站）、管理（基石与模块/模型管理/
  系统增强/实验室）。
- 「整理」内嵌的未导出/已导出切换（`[data-etab]`）会截到默认落在哪一档，但不会点进去切换那一层
  ——见「已知局限」。

## fixture 数据

- `fixtures/make_fixture_epub.py`：生成两本最小合法 EPUB（纯占位正文，不是真书），走
  `book-serve` 真实的 `POST /staging` 上传 API 入母版库——练的是真实上传路径，不是绕开它直接
  塞文件。
- `fixtures/make_fixture_book.py`：生成一份 `notecore::model::Book` JSON（9 条条目，覆盖
  Mined/Pending/Draft/Reviewed/Skipped/Revoked/Archived 全部 7 种状态 + 一条带"问 AI"的条目），
  直接落 `ink-serve` 的状态目录（`$XDG_STATE_HOME/notes/books/<uuid>.json`）——**这是绕开真实
  `.rm` 摄取管线的测试夹具**，不是"假装是真机摄取产物"，字段形状照抄 `notes/crates/notecore/
  src/model.rs` 手写，选它是因为条目库落盘就是普通 JSON、没有真实笔迹二进制那道门槛，覆盖度比
  等真机摄取好控制。

## 已知局限

- **不递归进第三层 tab**（比如「整理」的章节切换按钮、「浏览」按书本切换的下拉框）——目前只有
  顶层 nav + 一层子 nav 两层自动点击，第三层只截到默认状态。要扩大覆盖面往 `walk.mjs` 里加
  一层循环即可，结构是现成的。
- **只起三个后端服务**（book-serve/ink-serve/note-serve），font-serve/koreader-serve/
  wallpaper-serve/transcribe-serve/mind-serve 的面板/tab 走查不到——这几个要么需要真实外部
  依赖（AI API key），要么 fixture 构造成本更高，这次没做，是明确的范围裁剪不是遗漏。
- **截图不是像素级回归基线**（没有跟"上一次截图"比对的机制）——纯粹给人看，适合"改完这一块前端
  跑一遍、自己瞄一眼有没有明显错位/漏翻译/字段显示不对"，不是 CI 里能自动判定通过/失败的测试。
- 2026-09-16 第一次真跑就用这个工具抓到一个真实前端 bug（`cropHtml()` 把"有手写但裁图渲染
  失败"误判成"纯勾画没有手写"，见 `gateway/ui/app.js` 里 `notes.cropMissing` 那段改动的提交
  信息）——证明这条走查路径是有效的，不只是造了个没用过的工具。
