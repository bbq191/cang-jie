# hl-snap —— 荧光笔 CJK 精确吸附

**一句话**：用荧光笔划中文时，划哪就高亮哪，不再"划一小段吸整行"。

它是一个独立的 xovi 扩展（`hl-snap.so`）。xovi 是第三方扩展加载框架，在 xochitl 启动时把 `~/xovi/extensions.d/` 下的 `.so` 加载进进程；扩展再用 hook（把某个函数的入口改成先跳到自己的代码）改变行为，全程不改 xochitl 文件本身。

- **现状**：真机通（3.28.0.172），设备日常在用。
- **原理**：xochitl 划线后会调一个"扩张选区"函数（`FUN_00f05ad0`，3.28.0.172 上在 `0xf03670`），按空格分词把选区扩到整个"词"；中文没有空格，于是整行都被当成一个词。本扩展 hook 这个函数：选区第一个字是 CJK（统一表意、扩展 A、兼容表意、CJK 标点）时直接不调原函数，其它文字照旧。
- 来龙去脉、真机验证记录：[白皮书 §03a](../docs/reMarkable系统增强线白皮书.md)；xovi 扩展通用的加载/hook 流程见白皮书 §02 的流程图；代码细节见 `src/hl_snap.c` 头注。

## 开关

| 在哪 | 写什么 | 何时生效 |
|---|---|---|
| 网页「管理 → 系统增强」→「CJK 荧光笔精确吸附」 | `~/.local/share/cangjie-ime/reading-qol.json` 的 `hlSnapCjk`（布尔） | 扩展每次划线都现读这个文件，下一次划线就生效，不用重启 |

- **默认开**：文件缺失、字段缺失、读失败都按"开"处理。`cangjie-ime` 是历史目录名，沿用未改。
- 开关旁的「已加载 / 未加载」徽章表示 `hl-snap.so` 是否真的在 xochitl 进程里（网关查 `/proc/<pid>/maps`）。开关只是配置，扩展没加载时开了也没用。
- 旧版原生「设置」页若还有"笔记增强"面板，它写的也是这个键（该面板源码不在本仓库，未核对）。

## 构建

```sh
make aarch64                               # 产物 hl-snap.so（已提交进仓库）
make glue XOVI_DIR=<asivery/xovi clone 路径>   # 只有改了 hl-snap.xovi 才需要：重新生成 xovi 胶水
```

构建只需要 aarch64 交叉编译器（2026-09-25 起加 `-ffile-prefix-map=$(CURDIR)=.`，调试信息里不带开发机路径；反汇编不变；09-25 已部署真机，hook 装上）：xovi 胶水 `xovi_glue.{c,h}` 已提交进仓库，缺失时才会调 asivery/xovi 的 `xovigen.py` 生成（2026-09-24 前规则依赖 `.xovi` 的修改时间，checkout 后可能无端去跑 xovigen、没有 clone 就失败，部署脚本随即悄悄退回仓库里已提交的旧 `.so`）。部署脚本在构建失败时仍会退回已提交的 `.so`，并打出警告。`make clean` 只删 `hl-snap.so`，不删已提交的胶水（2026-09-30 前会一起删，删完没有 clone 就编不出来）。扫描/trampoline 公共代码在 [`../shared/`](../shared/PROVENANCE.md)，`cd ../shared && make test` 跑 host 单测。

## 部署

**推荐**（host 侧一键：构建 → 推送并逐个 md5 校验 → 设备端安装）：

```sh
cd packaging && sh deploy-hl-snap.sh <host>
```

它也是 `packaging/install-all.sh` 的一步（那时带 `DEFER_XOVI_START=1`，只落盘，最后由 `xovi-apply` 统一整机重启一次），见 [`../../packaging/README.md`](../../packaging/README.md)。

**手动在设备上跑**：`deploy/install.sh` 只放这个扩展的数据（名字、配置键），流程在 `packaging/xovi-ext-install.sh`，所以**同目录必须有 `xovi-ext-install.sh` 与 `devlib.sh`**，只拷一个 `install.sh` 会报错并提示改用 `deploy-hl-snap.sh`。

```sh
sh deploy/install.sh [--no-restart]     # --no-restart：只落盘，不重启
```

前置：设备上已 `vellum add xovi`。装到 `extensions.d/hl-snap.so`，和 `qt-resource-rebuilder.so` 并列（09-29 前还有 `appload.so`，已随 KOReader 一起卸载；手写优化 `hw-stroke.so` 2026-09-30 已移除）。安装器会：

1. 旧 `.so` 先备份进 `~/cangjie-backups/`（**绝不留在 `extensions.d/`**：xovi 会把该目录下任何文件当扩展加载，重名会让 xochitl 起不来）；内容没变就不重复备份，`--no-restart` 模式下也不会标记"待重启"。
2. 换文件：
   - 运行中的 xochitl 没在用旧版 → 先写暂存目录再 rename 进去（原子替换）；
   - 运行中的 xochitl **正映射着旧版** → 不当场换，先放进待换入区 `~/.cangjie-stage/so-pending/`，整机重启前换入（你自己 `reboot`，开机时 `xovi-reenable` 也会换入）。
3. `reading-qol.json` 只在首次建，不覆盖已有设置。
4. 不带 `--no-restart` 时：`.so` 没变、已在 xochitl 里加载、也没有别的待生效改动（`cj_apply_needed` 为假）就**不重启**，直接报"已是最新"；否则先提示并等 5 秒，然后：
   - xovi 已在 xochitl 里生效，或装了 `xovi-reenable.service` → 换入待换入区、**整机重启**（约 20–60 秒回来）。2026-09-25 起不再 restart xochitl：停止 xochitl 本身有概率在退出时崩溃，见 [`../../packaging/README.md`](../../packaging/README.md)「怎么让改动生效」。回来后用 `packaging/verify-on-device.sh` 核对（host 侧一键脚本会自动等设备回来再跑）。
   - 两者都没有 → 跑 `xovi/start`（此时 xochitl 没带 xovi，安全），之后做健康检查（`is-active`、`MainPID` 变化、`NRestarts` 不增、maps 里有 `hl-snap`）。
   - xovi 已生效时**绝不能**跑 `xovi/start`，会让 xochitl SEGV、整机重启。

**验证装上了**：`journalctl -u xochitl | grep hl-snap` 应有两行：

```
[hl-snap] _xovi_shouldLoad: 固件兼容(FUN_00f05ad0@0xf03670) → 加载
[hl-snap] 荧光笔EXPAND hook 安装完成 @ 0xf03670（neuter=1）
```

如果出现 `_xovi_construct: 找不到 xochitl 映射` 或 `特征码不再唯一命中（目标可能已被其它扩展改写），hook 未安装`，说明 `.so` 进了进程但 hook 没装上（2026-09-24 起才打这两行，以前静默），网页徽章仍会显示"已加载"。

每次 xochitl 渲染 PDF 时 fork 出的子进程里也会打一行"找不到 xochitl 映射 → 拒绝加载"，这是正常的，不代表主进程没装上。

## 和旧中文化扩展 `cangjie-langhook.so` 的关系

两者都要 patch 同一个 `FUN_00f05ad0`，**不要同时装**。langhook 自带同样的吸附修复，设备上若装着它，就不需要 hl-snap。

同时装了会怎样（按 hl-snap 代码推断，langhook 一侧源码已不在仓库，未核对、未真机测）：两边都在 `_xovi_shouldLoad` 阶段看到原始机器码、都同意加载；进入 `_xovi_construct` 后，先装 hook 的那个会改写函数开头，后装的那个再扫时特征码已经对不上，**静默放弃**这个 hook。所以不是"两个 patch 叠在一起"，而是"先到先得、后到的悄悄不生效"，网页徽章还会显示两个都"已加载"。要从 hl-snap 换成 langhook：先把 `extensions.d/hl-snap.so` 移出该目录，再整机重启（`reboot`）。
