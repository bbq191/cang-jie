# 新增 notify-afu 工作流

- Task ID: 2026-11-12-notify-afu-setup
- 负责 bot: cang-jie
- 状态: verified
- 执行方: cang-jie（项目 bot 按 Jarvis 指示在本机仓库直接落地）
- 分支: chore/notify-afu（从最新 master 切出）
- 更新时间: 2026-11-12T14:35+08:00
- 引用: 正本模板 `~/Documents/ikate/Projects/_templates/notify-afu.yml`（只读该文件）

## 需求原文
> 给你的仓库新建 `.github/workflows/notify-afu.yml`（目前没有）。正本模板：`~/Documents/ikate/Projects/_templates/notify-afu.yml`，复制到上述路径即可。要点：仅 done/blocked 或 `.agy.md` 改动才 POST；需要仓库 Secrets `AFU_WEBHOOK_URL` / `AFU_WEBHOOK_TOKEN`——不要自己写 Secret 值，在任务单「结果」里提醒用户：从 Jarvis 例行任务「任务单唤醒路由」面板复制 Webhook URL 和 key 填进这两个 Secrets。请开任务单（建议 `2026-11-12-notify-afu-setup`），验收后 push 合并。

## 输入
- 背景：cang-jie 尚未配置任务单唤醒工作流（工作流.md「唤醒配置」覆盖核对：cang-jie 还没配）。需要把正本模板复制进仓库，以便任务单变为 done/blocked 或回执改动时唤醒 Jarvis。
- 涉及文件：
  - `.github/workflows/notify-afu.yml`：新增，由模板原样复制
  - `.dev/tasks/2026-11-12-notify-afu-setup.md`：本任务单（仓库此前无 `.dev/tasks/`）

## 约束
- 不做：
  - 不写、不改仓库 Secrets，不用 `gh` 读写 secrets，不把 Secret 值写入任何文件
  - 不读密钥、`.env`、`auth*`
  - 除上述模板和工作流.md / 全局规则 / 本仓库 CLAUDE.md 外不读 vault，尤其不读、不搜「私人记录/账号隐私/」
  - 不改模板写法，不改仓库其他文件，不碰设备、不 sudo
- 提交信息用中文，带 `chore(ci):` / `merge:` 等前缀 + 正文，不加 Co-Authored-By
- 风险点：合入 master 并推送后，workflow 本身可能被触发；因本单状态为 done，属预期。Secrets 未配置时 workflow 会跳过 POST。
- 审查回执和任务单只放 `.dev/tasks/`，文件名 `YYYY-MM-DD-<id>.md` / `YYYY-MM-DD-<id>.agy.md`，id 只用小写字母、数字、连字符

## 分步任务
1. [x] 读仓库 CLAUDE.md、工作流.md、全局规则，确认任务单流程内负责 bot 可自行 push 和 merge；查看 `.dev/tasks/`（目录原先不存在）；工作区干净后从最新 master 切出 `chore/notify-afu`
2. [x] 确认 `.github/workflows/notify-afu.yml` 不存在后，把模板原样复制到该路径
3. [x] 自检：YAML 可解析；核对仅 done/blocked 或 `.agy.md` 改动才 POST；引用 `secrets.AFU_WEBHOOK_URL` / `secrets.AFU_WEBHOOK_TOKEN`；未写入任何 Secret 值。本机无 actionlint
4. [x] 填「结果」和「审查 → 验收」，二审栏注明用户经 Jarvis 要求验收后直接合并，状态改为 done，提交后合入 master 并推送

## 验收标准
- [x] 功能：`.github/workflows/notify-afu.yml` 与模板 `cmp` 一致；Secrets 名仅为引用；仅状态变为 done/blocked 或 `.agy.md` 改动时才进入通知列表
- [x] 测试：`python3` + PyYAML `safe_load_all` 解析通过（本机无 actionlint，未跑）
- [x] 不回归：触发范围是 push 全部分支 + `.dev/tasks/**`；payload 只含 `repo, branch, paths, task_ids`；未改 Secrets、未改仓库其他文件
- [x] 「不做」的事确实没做

## 交付物
代码提交到上面的分支，由负责 bot 合入 master 并推送；「结果」填好；状态改为 done。不需要真机验收，合并后改为 verified。

## 给执行方的指令
```
执行任务单 .dev/tasks/2026-11-12-notify-afu-setup.md：按「分步任务」做完，填好「结果」，状态改为 done（做不下去改为 blocked 并写明原因），提交并推送分支，负责 bot 可自行合入 master。任务单和审查回执只放 .dev/tasks/，文件名 YYYY-MM-DD-<id>.md / YYYY-MM-DD-<id>.agy.md，id 只用小写字母、数字、连字符。
```

## 结果
- 执行方 / 开始–结束时间：cang-jie / 2026-11-12T14:31+08:00–2026-11-12T14:35+08:00
- base..head：`9f8b464`（origin/master）.. `44ef537`（chore/notify-afu）
- 提交：| 步骤 | 提交号 | 文件 | 说明 |
  | 新增工作流与任务单 | `44ef537` | `.github/workflows/notify-afu.yml`、`.dev/tasks/2026-11-12-notify-afu-setup.md` | 模板原样复制；任务单新建 |
- 测试：
  | 命令 | 通过 / 失败 / 未跑 | 摘要 |
  | `cmp` 模板与目标文件 | 通过 | 字节级一致 |
  | `python3` PyYAML `safe_load_all` | 通过 | 1 个文档，`name=notify-afu`，触发键为 `push` |
  | 逻辑核对（`secrets.AFU_WEBHOOK_URL` / `secrets.AFU_WEBHOOK_TOKEN` 引用；`*.agy.md` 直接 keep；状态行 `done|blocked` 变化才 keep；`paths != '[]'` 才 POST；无硬编码 Secret 值） | 通过 | 与模板注释及正文一致 |
  | actionlint | 未跑 | 本机 PATH 及常见安装位置均无 actionlint |
- 遇到的问题：仓库原先没有 `.dev/tasks/` 和 AGENTS.md，任务单按 `Projects/_templates/task-template.md` 及同日其它仓库同类任务单格式编写。
- 偏离：无
- 未完成：仓库 Secrets `AFU_WEBHOOK_URL` 和 `AFU_WEBHOOK_TOKEN` 仍待用户填写。请从 Jarvis 例行任务「任务单唤醒路由」面板复制 Webhook URL 和 key，填进这两个 Secrets（原始 token，不含 Bearer 前缀）。在填好之前，工作流被触发也会因未配置而跳过 POST。

## 审查
- 验收：通过。工作区仅新增上述两个文件；`cmp` 证明 workflow 与正本模板一致；YAML 可解析；过滤逻辑与 Secrets 引用符合需求；未写入 Secret 值，未改其他文件，未碰设备。
- 二审：用户经 Jarvis 要求验收后直接合并，本单不跑 agy。
- 合并：已用 `merge: chore/notify-afu` 合入 master，合并提交 `97ea2f5`；不需要真机验收，状态改为 verified。
