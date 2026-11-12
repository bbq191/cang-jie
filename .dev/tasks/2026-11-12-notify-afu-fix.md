# 用修复版正本替换 notify-afu 工作流

- Task ID: 2026-11-12-notify-afu-fix
- 负责 bot: cang-jie
- 状态: done
- 执行方: cang-jie（项目 bot 按指示在本机仓库直接落地）
- 分支: fix/notify-afu-merge-base（从最新 master 切出）
- 更新时间: 2026-11-12T14:36+08:00
- 引用: 2026-11-12-notify-afu-setup；正本模板 `~/Documents/ikate/Projects/_templates/notify-afu.yml`（sha256 以 `2c3f9ea8` 开头、以 `d206` 结尾）

## 需求原文
> 暂停解除：notify-afu 正本已修好（before 不可用时改用与 origin/默认分支的 merge-base 做 diff）。请在 alienware 的 ~/Projects/cang-jie 按同样任务单流程做替换：先算正本 sha256，必须以 2c3f9ea8 开头、以 d206 结尾；不符就停下什么都别改。git status 干净、在 master 且与 origin 同步才继续。新任务单 `.dev/tasks/2026-11-12-notify-afu-fix.md`，写明原因：旧模板新分支首推会误发历史回执，整文件替换为修复版正本；关联 2026-11-12-notify-afu-setup。新分支 `fix/notify-afu-merge-base`，整文件覆盖 `.github/workflows/notify-afu.yml`，cmp 与正本一致，PyYAML 解析通过，确认 diff 只涉及该文件和任务单，确认没有 Secret 值。中文提交 `fix(ci):` 前缀，正文写原因，不加 Co-Authored-By；merge: 合并到 master 并 push，回填任务单合并号、状态 verified。任务单「结果」提醒：现在可以从 Jarvis 例行任务「任务单唤醒路由」面板复制 Webhook URL 和 key，填进 `AFU_WEBHOOK_URL` / `AFU_WEBHOOK_TOKEN`。冲突、远端保护或任何异常就停下报告。不碰设备、不 sudo、不改其他文件。

## 输入
- 背景：`2026-11-12-notify-afu-setup` 已把旧正本合入 master 并推送。旧模板在新分支首次 push（`github.event.before` 为全 0 或不可用）时，会把 `.dev/tasks` 下全部现存文件当作变更，从而误发历史回执。修复版正本在 before 不可用时改为与 `origin/<默认分支>` 的 merge-base 做 diff。
- 涉及文件：
  - `.github/workflows/notify-afu.yml`：整文件覆盖为修复版正本
  - `.dev/tasks/2026-11-12-notify-afu-fix.md`：本任务单

## 约束
- 不做：
  - 不写、不改仓库 Secrets，不用 `gh` 读写 secrets，不把 Secret 值写入任何文件
  - 不读密钥、`.env`、`auth*`
  - 除正本模板和工作流相关规则外不读 vault，尤其不读、不搜「私人记录/账号隐私/」
  - 不改模板写法，不改仓库其他文件，不碰设备、不 sudo
- 提交信息用中文，带 `fix(ci):` / `merge:` 等前缀 + 正文，不加 Co-Authored-By
- 风险点：合入 master 并推送后，workflow 可能被触发；因本单状态后续为 verified，过滤逻辑不应因此误报历史回执。Secrets 未配置时 workflow 会跳过 POST。
- 审查回执和任务单只放 `.dev/tasks/`，文件名 `YYYY-MM-DD-<id>.md` / `YYYY-MM-DD-<id>.agy.md`，id 只用小写字母、数字、连字符

## 分步任务
1. [x] 计算正本 sha256，确认为 `2c3f9ea8…d206`；确认工作区干净、在 master 且与 origin/master 同步后，切出 `fix/notify-afu-merge-base`
2. [x] 用修复版正本整文件覆盖 `.github/workflows/notify-afu.yml`，`cmp` 一致
3. [x] 自检：PyYAML 解析通过；含 merge-base 回退；仅 done/blocked 或 `.agy.md` 改动才 POST；只引用 Secrets，无 Secret 值；diff 仅涉及工作流与本任务单
4. [x] 填「结果」和「审查 → 验收」，用户要求验收后直接合并，状态改为 done，提交后合入 master 并推送

## 验收标准
- [x] 功能：`.github/workflows/notify-afu.yml` 与修复版正本 `cmp` 一致；before 不可用时改用与 origin/默认分支的 merge-base 做 diff；Secrets 名仅为引用
- [x] 测试：`python3` + PyYAML `safe_load_all` 解析通过
- [x] 不回归：触发范围仍是 push 全部分支 + `.dev/tasks/**`；payload 仍只含 `repo, branch, paths, task_ids`；仅 done/blocked 或 `.agy.md` 改动才 POST；未改 Secrets、未改其他文件
- [x] 「不做」的事确实没做

## 交付物
代码提交到上面的分支，由负责 bot 合入 master 并推送；「结果」填好；状态改为 done。不需要真机验收，合并后改为 verified。

## 给执行方的指令
```
执行任务单 .dev/tasks/2026-11-12-notify-afu-fix.md：按「分步任务」做完，填好「结果」，状态改为 done（做不下去改为 blocked 并写明原因），提交并推送分支，负责 bot 可自行合入 master。
```

## 结果
- 执行方 / 开始–结束时间：cang-jie / 2026-11-12T14:36+08:00–2026-11-12T14:36+08:00
- 正本 sha256：`2c3f9ea80688adfb0c14772cb91fed985f31806e564841c5cbebfac060dbd206`
- base..head：`75f7317`（origin/master）.. `fix/notify-afu-merge-base`（提交号见下，提交后回填）
- 提交：| 步骤 | 提交号 | 文件 | 说明 |
  | 替换工作流并新建任务单 | （提交后回填） | `.github/workflows/notify-afu.yml`、`.dev/tasks/2026-11-12-notify-afu-fix.md` | 整文件覆盖为修复版正本；任务单新建 |
- 测试：
  | 命令 | 通过 / 失败 / 未跑 | 摘要 |
  | 正本 sha256 前缀/后缀核对 | 通过 | `2c3f9ea8…d206`，全值如上 |
  | `cmp` 模板与目标文件 | 通过 | 字节级一致，sha256 相同 |
  | `python3` PyYAML `safe_load_all` | 通过 | 1 个文档，`name=notify-afu` |
  | 逻辑核对（`git merge-base` + `DEFAULT_BRANCH` 回退；`secrets.AFU_WEBHOOK_URL` / `secrets.AFU_WEBHOOK_TOKEN` 仅引用；`*.agy.md` 或状态变为 done/blocked 才 keep；无硬编码 Secret 值） | 通过 | 与修复版正本一致 |
  | `git diff --stat` | 通过 | 覆盖后工作流 `14 insertions, 3 deletions`；另加本任务单，无其他文件 |
- 遇到的问题：无
- 偏离：无
- 未完成：仓库 Secrets `AFU_WEBHOOK_URL` 和 `AFU_WEBHOOK_TOKEN` 仍待用户填写。现在可以从 Jarvis 例行任务「任务单唤醒路由」面板复制 Webhook URL 和 key，填进这两个 Secrets（原始 token，不含 Bearer 前缀）。在填好之前，工作流被触发也会因未配置而跳过 POST。

## 审查
- 验收：通过。仅改上述两个文件；`cmp` 与修复版正本一致；YAML 可解析；merge-base 回退与 Secrets 引用符合需求；未写入 Secret 值，未改其他文件，未碰设备。
- 二审：用户要求验收后直接合并，本单不跑 agy。
- 合并：待合入 master 后回填合并提交号；不需要真机验收。
