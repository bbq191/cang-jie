# ui-font —— xochitl 界面字体

**一句话**：把 xochitl 界面（菜单、设置、文件名、对话框、标题）的字体换成你上传的字体，阅读器的字体不变。

它由三块组成，都读同一个选择文件 `~/.local/share/shelf/ui-font.json`（`{"version":1,"sans":"…","serif":"…"}`，空串 = 原生字体）：

| 部件 | 在哪 | 管哪部分界面 |
|---|---|---|
| xovi 扩展 `ui-font.so` | 本目录 | xochitl 启动时设的**应用默认字体**（老界面里没写字体名的文字）：`reMarkable Sans` → `sans` |
| qmd 补丁 `ui-font-tokens.qmd` | [`../../shelf/xovi/`](../../shelf/xovi/ui-font-tokens.qmd)（随 font 服务安装） | **Ark 设计令牌**（新版设置页、对话框、按钮等）：正文 `reMarkable Sans` → `sans`，标题 `reMarkable Serif Small` → `serif` |
| font-serve 的界面字体仓库 | [`../font-serve/`](../font-serve/src/ui.rs) | 上传 / 删除界面字体、写选择文件；网页「其他 → xochitl → 界面字体」 |

- **现状**：已合 master（6e915bc），10-07 17:14 已部署并整机重启，部署自检 38✓ 1⚠（刚开机）0✗；xochitl 日志确认扩展与 qmd 都生效（正文、标题都选了更纱黑体 UI SC）；用户在设备上确认书库、设置、对话框、标题都已是更纱，阅读时书里的中文仍是原来的字体。设备上的 `.so` 是 6e915bc 版（md5 `7199073b…`），之后只改了子进程里拒绝加载那句日志的措辞，下次部署带上。
- 原理、为什么这样做、怎么保证不影响阅读：[白皮书 §03o](../docs/reMarkable系统增强线白皮书.md)。

## 不影响阅读

- 界面字体放在 fontconfig 用户字体目录的子目录 `fonts/shelf-ui/`：xochitl 能按名字用它，但 font-serve 的阅读字体扫描只看顶层，所以它**不进阅读器的字体菜单**（`fonts.json`），也**不进中文缺字回退链**。
- 只装了界面字体时，fontconfig 排阅读的中文缺字回退会把它排到第一（设备上 `fc-match -s` 实测）。所以 font-serve 在有界面字体时往 `fonts.conf` 末尾加一条规则，给每个字体请求追加系统自带的 `Noto Sans SC`（weak），中文回退仍落在它上面；删光界面字体后这条规则自动撤掉。
- 扩展只改 xochitl 自己对 `QGuiApplication::setFont` 的那一次调用；阅读器菜单里的「reMarkable Sans」、笔记本里打字用的字体都是按名字直接取内嵌字体，不经过它。

## 生效

两处都只在 xochitl 启动时读一次选择：改完要**整机重启**（与其它 qmd / 扩展一样，不单独 restart xochitl）。网页改完会显示"整机重启后生效"。

## 构建与测试

```sh
make aarch64   # 产物 ui-font.so（已提交进仓库）
make test      # host（x86_64）：配置解析 + 导入表改写（懒绑定 / BIND_NOW × PIE / 非 PIE）+ 真实 Qt 6 上走一遍 setFont 替换
```

不用 `../shared/` 的特征码扫描和跳板：`setFont` 开头第 3 条是 PC 相对的 `adrp`，搬进跳板会算错地址，所以改的是 xochitl 导入表（`.got.plt`）里的那一格，运行时按符号名从 `.rela.plt` 找，不写死地址。xovi 胶水 `xovi_glue.{c,h}` 与 hl-snap 的逐字节相同（`ui-font.xovi` 只有版本号）。

## 部署

```sh
cd packaging && sh deploy-ui-font.sh <host>   # 构建 → 推送并 md5 校验 → 设备端安装（xovi-ext-install.sh）
sh deploy.sh <host> --only font               # font-serve + 网页 + ui-font-tokens.qmd
```

`install-all.sh` 里它是 `ui-font` 一步（只落盘，最后 `xovi-apply` 统一整机重启）。卸载：`uninstall-all.sh` 的 `ui-font` 步只摘 `.so`，不碰界面字体文件和选择文件（那是 shelf 的数据）。

**验证装上了**：`journalctl -u xochitl | grep -E 'ui-font|SHELF-UI-FONT'`：

```
[ui-font] 安装完成（setFont 导入槽 0x…）
[ui-font] setFont(reMarkable Sans) → Sarasa UI SC        # 没选界面字体时是"没选界面字体，原样放行"
SHELF-UI-FONT: sans=Sarasa UI SC serif=…
```
