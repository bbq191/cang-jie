# packaging —— 全新设备统一安装器

host 侧编排层，2026-09-11 新写。全新（或愿意重装的现有）reMarkable Paper Pro Move，固件跟
`firmware-allowlist.txt` 对得上时，一条命令装完当前仓库能装的一切：

```sh
cd packaging
sh install-all.sh <host>                                        # 装全部
sh install-all.sh <host> --force                                # 固件不在白名单也强装
sh install-all.sh <host> --skip battop,hl-snap,handwriting-stroke,shelf   # 跳过指定步骤
```

`<host>` 默认 `10.11.99.1`（USB 网段）。

## 装什么、按什么顺序

`install-all.sh` 只编排，不重新实现任何构建/传输逻辑——先过固件安全门，再依次调用四个
各自独立可用的部署脚本：

| 顺序 | 脚本 | 装什么 | 前置 |
|---|---|---|---|
| 1 | `deploy-battop.sh` | 电池刺客（纯 Rust systemd 常驻采样服务） | 无，跟 xovi/vellum 完全无关 |
| 2 | `deploy-hl-snap.sh` | 荧光笔 CJK 精确吸附（独立最小 xovi 扩展） | 设备已 `vellum add xovi` |
| 3 | `deploy-handwriting-stroke.sh` | CJK 手写笔迹渲染优化（独立最小 xovi 扩展） | 同上 |
| 4 | `deploy.sh` | 网关 + book/koreader/font/wallpaper 四个领域服务 + 笔记线（ink/transcribe/mind/note） | 无（`font`/`book` 的回收站/建夹代理 qmd 这两个可选特性依赖 `qt-resource-rebuilder` 已存在，缺了自动跳过不阻塞） |

四个脚本都可以单独跑（`sh deploy-battop.sh <host>` 等），不依赖 `install-all.sh`——它只是把
四步串起来 + 加一层固件门 + 汇总结果。任何一步失败：打印清楚是哪一步、原始错误，**不自动
重试、不静默跳过**，退出非零。

## 固件安全门

`firmware-allowlist.txt`：每行 `sha256(/usr/bin/xochitl)  <人读标签>`，装前 ssh 拉设备上
`/usr/bin/xochitl` 的哈希跟这张表比对。不命中默认拒装（避免在没验证过注入定位的固件上装错，
qmd/hook 偏移错了轻则功能不生效重则设备行为异常）；确认要装用 `--force`（自动把当前哈希追加
进表里）。文件本身详细讲了为什么用 sha256 而不是版本号，见文件内注释。

## 明确不做的事（已知缺口，别当成已经解决）

- **不装 vellum/xovi 本体**——这是所有脚本（新旧）共同的手动前置条件，本脚本不代为安装，
  缺失时子脚本会清楚报错（`没找到 .../xovi.so —— 先跑：vellum add xovi`），`install-all.sh`
  收尾摘要会再提醒一次。
- **不装中文化**（输入法/候选栏/词典/UI 汉化）——那条链路（`chinese-ime/langhook/`）随
  2026-09-11 全仓库大归档挪出了 git 仓库，目前只在本机 `/home/afu/Projects/oldbak/chinese-ime/`，
  没有回到版本控制。要装：去那边手动编译 + 跑 `deploy/install.sh`（前置同样是
  `vellum add xovi qt-resource-rebuilder`）。
- **不重建 xovi 开机持久化恢复链**（`xovi-reenable.service` 那套——`/etc` 是 tmpfs，重启即清，
  没有这个单元的话每次重启都要手动 `~/xovi/start` 才能让 xovi 重新加载扩展）。这个单元的源
  现在只在 `oldbak/reading/device-rs/systemd/`，要不要把它捞回来划进本脚本的职责，是另一个
  更大的决定，这次没有一并做。
- **不装 chrony 国内 NTP / wifi-watch 常驻看护**——这两个脚本目前只在
  `oldbak/packaging/{chrony-cn.sh,wifi-watch/}`，没有随这次恢复。
- **没有对称的 `uninstall-all.sh`**——三个 enhance 工具目前只能各自手动清理（`shelf/uninstall.sh`
  能卸 shelf 那部分）。

旧的 `packaging/package.sh`（打 `cangjie-full-*.tar.gz` 单体安装包那套）**没有**在这次一并
恢复/重写——经核实那份现在实际上是断的（`oldbak/packaging/package.sh` 按旧路径找 `shelf/` 载荷，
但 `shelf/` 早就独立成仓库顶层项目了，`oldbak/` 下没有这个子目录），也不是这次 `install-all.sh`
的设计参照；这次是纯编排现有独立脚本，不是复刻旧的单体打包架构。

## 验证现状（如实说明，不夸大）

离线验证过：`shellcheck --severity=warning` 全部新脚本零告警；`install-all.sh` 的参数解析
（未知参数拒绝、`--skip` 解析）用假 host 验证过报错路径正确；四个子脚本各自用假 host 跑过，
确认本地构建/打包这半段（不需要网络）能正确走完，卡在 ssh 连不上这步符合预期。**编排脚本
本身没有真机端到端验证过**——四个被调用的子脚本各自的设备端逻辑都在真实设备上验证过能跑
（`shelf/deploy.sh`、三个 enhance 工具各自的 `install.sh`），但"一条命令从固件门到四步全部
跑完、装到真机上"这个完整流程还没有在真实设备上走过一遍。真要下"已验证"的结论，等有真机
跑通一次整个流程再回来补这条记录。

编排这几个脚本的过程中踩到两个真坑，已修：
- `deploy-battop.sh` 最初用 `cargo build --manifest-path ../enhance/battop/Cargo.toml` 在
  `packaging/` 目录下直接调用——**Cargo 搜 `.cargo/config.toml`（CC/AR 覆盖）是按当前工作
  目录往上找，不看 `--manifest-path`**，导致 battop 的 CC 覆盖没生效，本机实测直接在链接
  `crt1.o` 这步报 `Relocations in generic ELF` 炸掉。改成先 `cd` 进 `enhance/battop/` 再跑
  `cargo build` 后立刻恢复正常。
- `deploy-hl-snap.sh`/`deploy-handwriting-stroke.sh` 最初在构建前跑了 `make clean`——两个
  `.so` 都已提交进仓库（不用每次现建才能部署），但本机没有 `asivery/xovi` 的 clone、编不出
  新的，`make clean` 先把已提交的 `.so`/`xovi_glue.{c,h}` 删了，重编又失败，**结果是仓库里
  能用的产物被脚本自己删掉却没能力补回来**（当场发现、`git checkout HEAD --` 救回，没有提交
  这个状态）。改成不清、构建失败时退回用仓库里已提交的版本（缺 xovi clone 时会打印清楚的
  提示，而不是留一个空洞）。
