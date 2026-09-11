# packaging —— 全新设备统一安装器

host 侧编排层，2026-09-11 新写。全新（或愿意重装的现有）reMarkable Paper Pro Move，固件跟
`firmware-allowlist.txt` 对得上时，一条命令装完当前仓库能装的一切：

```sh
cd packaging
sh install-all.sh <host>                                        # 装全部
sh install-all.sh <host> --force                                # 固件不在白名单也强装
sh install-all.sh <host> --skip chrony-cn,timezone-cn,xovi-persist   # 跳过指定步骤
```

`<host>` 默认 `10.11.99.1`（USB 网段）。

## 前置条件（全新设备，需手动，本脚本不代装）

以下几样是 reMarkable 官方/`vellum`/`appload` 生态自己的东西，不属于这个仓库，`install-all.sh`
**不会**帮你装，缺了会在对应步骤报清楚的错误：

1. **`vellum add xovi`**——xovi 本体（`hl-snap`/`handwriting-stroke`/`xovi-persist` 三步的硬前提）。
2. **`vellum add qt-resource-rebuilder`**——`shelf` 里 `font`/`book` 的字体菜单、回收站/建夹代理这
   几个可选特性依赖它；缺了这两个特性自动跳过，不阻塞其它安装。
3. **`vellum add appload`**——第三方 App 加载器，KOReader 要通过它侧载。
4. **KOReader**（经 appload 侧载）——`shelf` 的 `koreader-serve` 只是管理/配置这个已装好的
   KOReader，不负责把 KOReader 本身装上去。

装好以上四样、再跑 `install-all.sh`，才是完整的"全新设备"安装顺序。

## 装什么、按什么顺序

`install-all.sh` 只编排，不重新实现任何构建/传输逻辑——先过固件安全门，再依次调用七个
各自独立可用的部署脚本：

| 顺序 | 脚本 | 装什么 | 前置 |
|---|---|---|---|
| 1 | `deploy-chrony-cn.sh` | 国内 NTP（chrony 服务器换成阿里云/腾讯云等） | 无，跟 xovi/vellum 完全无关 |
| 2 | `deploy-timezone-cn.sh` | 默认时区设为 Asia/Shanghai | 无，跟 xovi/vellum 完全无关；设备镜像缺 `/usr/share/zoneinfo/Asia/Shanghai` 时优雅跳过 |
| 3 | `deploy-battop.sh` | 电池刺客（纯 Rust systemd 常驻采样服务） | 无，跟 xovi/vellum 完全无关 |
| 4 | `deploy-xovi-persist.sh` | xovi 开机持久化恢复链（`xovi-reenable.service`） | 设备已 `vellum add xovi`（`/home/root/xovi/start` 存在） |
| 5 | `deploy-hl-snap.sh` | 荧光笔 CJK 精确吸附（独立最小 xovi 扩展） | 同上 |
| 6 | `deploy-handwriting-stroke.sh` | CJK 手写笔迹渲染优化（独立最小 xovi 扩展） | 同上 |
| 7 | `deploy.sh` | 网关 + book/koreader/font/wallpaper 四个领域服务 + 笔记线（ink/transcribe/mind/note） | 无（`font`/`book` 的回收站/建夹代理 qmd 这两个可选特性依赖 `qt-resource-rebuilder` 已存在，缺了自动跳过不阻塞） |

七个脚本都可以单独跑（`sh deploy-battop.sh <host>` 等），不依赖 `install-all.sh`——它只是把
七步串起来 + 加一层固件门 + 汇总结果。任何一步失败：打印清楚是哪一步、原始错误，**不自动
重试、不静默跳过**，退出非零。

## 固件安全门

`firmware-allowlist.txt`：每行 `sha256(/usr/bin/xochitl)  <人读标签>`，装前 ssh 拉设备上
`/usr/bin/xochitl` 的哈希跟这张表比对。不命中默认拒装（避免在没验证过注入定位的固件上装错，
qmd/hook 偏移错了轻则功能不生效重则设备行为异常）；确认要装用 `--force`（自动把当前哈希追加
进表里）。文件本身详细讲了为什么用 sha256 而不是版本号，见文件内注释。

## xovi-reenable.service 为什么放在 packaging/，不放 shelf/

`shelf/docs/reMarkable书架白皮书.md` §03o 记过一次相关的历史决策（历史文档，不改写，这里只
引用结论）：2026-09-03 曾经有一版把 `--with-xovi-reenable` 装进 `shelf/install.sh`、单元源放
`reading/device-rs/systemd/`，被撤回——理由是 xovi 持久化是**整个 xovi 层**通用的（重跑
`xovi/start` 会重注入全部扩展，不分 shelf/中文化/enhance），`shelf` 耦合它反而破坏"网关+领域
服务独立可插拔"的原则。`shelf/install.sh` 现在的头注也仍然写着这层该由"整包"装。`packaging/`
正是这次大归档后 `install-on-device.sh` 的精神继承者，装它是把当初就规划好、只是归档时连带
消失的一层补回来，不是重新踩同一个耦合坑。

## 明确不做的事（已知缺口，别当成已经解决）

- **不装 vellum/xovi/qt-resource-rebuilder/appload 本体、不侧载 KOReader**——这是所有脚本
  （新旧）共同的手动前置条件，见上面「前置条件」一节，本脚本不代为安装，缺失时子脚本会清楚
  报错，`install-all.sh` 收尾摘要会再提醒一次。
- **不装中文化**（输入法/候选栏/词典/UI 汉化）——那条链路（`chinese-ime/langhook/`）随
  2026-09-11 全仓库大归档挪出了 git 仓库，目前只在本机 `/home/afu/Projects/oldbak/chinese-ime/`，
  没有回到版本控制。要装：去那边手动编译 + 跑 `deploy/install.sh`（前置同样是
  `vellum add xovi qt-resource-rebuilder`）。
- **不装 wifi-watch 常驻看护**——目前只在 `oldbak/packaging/wifi-watch/`，没有随这次恢复。
- **没有对称的 `uninstall-all.sh`**——三个 enhance 工具 + xovi-persist/chrony-cn/timezone-cn
  目前只能各自手动清理（`shelf/uninstall.sh` 能卸 shelf 那部分；`xovi-reenable.service` 卸载
  是 `systemctl disable --now xovi-reenable.service` + 删 `/usr/lib/systemd/system/` 里的单元
  和软链；chrony/timezone 两个是配置覆写，没有"卸载"语义）。

旧的 `packaging/package.sh`（打 `cangjie-full-*.tar.gz` 单体安装包那套）**没有**在这次一并
恢复/重写——经核实那份现在实际上是断的（`oldbak/packaging/package.sh` 按旧路径找 `shelf/` 载荷，
但 `shelf/` 早就独立成仓库顶层项目了，`oldbak/` 下没有这个子目录），也不是这次 `install-all.sh`
的设计参照；这次是纯编排现有独立脚本，不是复刻旧的单体打包架构。

## 验证现状（如实说明，不夸大）

**真机验证过、确认能跑通的部分**：固件安全门（3.28.0.172 真机 sha256 命中白名单）；
`deploy-hl-snap.sh`/`deploy-handwriting-stroke.sh` 两步整个流程（构建→推送→设备端安装→
`journalctl` 确认 hook 已加载、`is-active`=active、`NRestarts`=0）。

**真机跑过、发现问题、已修但改动本身还没有复验的部分**：
- `deploy-battop.sh`：重装（非首次装）时若 `battop.service` 已在跑，`scp` 直接覆盖正在执行的
  二进制被内核拒绝（`ETXTBSY`，报 `scp: dest open ... Failure`）。修法：推送前先
  `systemctl stop battop.service`（`install.sh` 最后会自己重新 `enable --now`）。
- `deploy.sh`（原 `shelf/deploy.sh`）：健康检查原来固定 `sleep 1` 就查 `systemctl is-active`，
  `gateway` 首次启动要签发私有 CA/自签证书，1 秒不够、还在 `activating` 就被判定为"没起来"。
  修法：`shelf/install.sh` 的健康检查改成轮询（最多等 10 秒）。

**离线验证过，真机完全没跑过的部分**（这次新加的三步，没有真机可用）：
- `shellcheck --severity=warning` 全部新脚本零告警；假 host 测试确认参数解析、本地路径拼接
  正确、卡在 ssh 连不上这步符合预期（详见各脚本内注释）。
- `deploy-xovi-persist.sh`：设备端写 `/usr` 那段逻辑是照抄 `shelf/install.sh` 已经真机验证过的
  dm-verity 门+remount 模式，**但"装完重启一次、xovi 真的自动恢复"这个核心承诺没有真机验证
  过**，需要用户自己重启设备确认。
- `deploy-chrony-cn.sh`：`chrony-cn.sh` 本体的 remount+bind 机制此前在旧版本（`oldbak/`）上
  真机验证过，这次是重写，逻辑照抄未改，理论上行为一致，但这次重写后的版本本身没有单独
  重新在真机上跑过。
- `deploy-timezone-cn.sh`：**全新脚本，零真机验证**——`/usr/share/zoneinfo/Asia/Shanghai`
  这台设备镜像是否真的存在都没有确认过；脚本设计了"缺失就优雅跳过"分支，但这个分支本身
  也没有真机触发验证过。

真要下"已验证"的结论，等有真机跑完一整轮再回来补这条记录。

编排这几个脚本的过程中踩到两个真坑（离线阶段发现，已修）：
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
