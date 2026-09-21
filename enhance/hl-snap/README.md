# hl-snap —— 荧光笔 CJK 精确吸附

独立最小 xovi 扩展（xovi 是第三方扩展加载框架，扩展是它在 xochitl 启动时加载的 `.so`），只做一件事：CJK 划线时命中区间不向两边扩张成整行（划哪吸哪）。

**原理一句话**：xochitl 的荧光笔划线后会调一个"扩张选区"函数（`FUN_00f05ad0`，3.28.0.172 上位于 `0xf03670`），按空格分词把选区扩到整个"词"；中文没有空格，于是"划一小段吸整行"。本扩展 hook 这个函数，选区首字是 CJK 时直接不调原函数。

决策过程/真机验证见 `../docs/reMarkable系统增强线白皮书.md` §03a；技术细节见 `src/hl_snap.c` 头注。

## 构建

```sh
make aarch64 XOVI_DIR=<asivery/xovi clone 路径>
```

产物 `hl-snap.so`（已提交进仓库，部署时不用每次现建）。构建需要 aarch64 交叉编译器和 asivery/xovi 的 `xovigen.py`；没有 xovi clone 时，部署脚本会退回用仓库里已提交的版本。

## 部署

**推荐（host 侧一键构建+推送+安装）**：`cd packaging && sh deploy-hl-snap.sh <host>`。它构建 `.so`、把 `.so`、`deploy/install.sh`、`packaging/xovi-ext-install.sh`、`packaging/devlib.sh` 一起推到设备的暂存目录（md5 逐个校验），再跑设备端 `install.sh`；也是 `packaging/install-all.sh` 调用的一步（那时用 `DEFER_XOVI_START=1`，只落盘、最后统一重启），见 `../../packaging/README.md`。

**手动在设备上跑**：`deploy/install.sh` 现在只剩这个扩展的数据（名字、配置键），流程在 `packaging/xovi-ext-install.sh`——所以**同目录必须有 `xovi-ext-install.sh` 与 `devlib.sh`，只拷单个 `install.sh` 不够**（缺了会清楚报错并提示用 `deploy-hl-snap.sh`）：

```sh
sh deploy/install.sh [--no-restart]     # --no-restart：只落盘不重启 xochitl
```

设备上需要先有 `vellum add xovi`。装的是 `extensions.d/hl-snap.so`，跟 `appload.so`/`qt-resource-rebuilder.so` 并列。行为：

- 旧 `.so` 先备份进 `~/cangjie-backups/`（**绝不留在 `extensions.d/`**，xovi 会把该目录下任意文件当扩展加载）；
- 用"先写暂存目录再 rename"原子替换，不在运行中 xochitl 已映射的文件上原地写；
- `reading-qol.json` 首次才建、不覆盖已有设置；
- 不带 `--no-restart` 时：xovi 已在 xochitl 里生效 → `systemctl restart xochitl`，否则 `xovi/start`（xovi 已生效时跑 `xovi/start` 会让 xochitl SEGV 整机重启，见 `../../packaging/README.md`）。

⚠️ **不能跟完整版中文化扩展 `cangjie-langhook.so` 同时部署**（它的源码已搬出本仓库，见根 README「历史与范围」，但设备上可能仍装着）：两者都会 patch 同一个 `FUN_00f05ad0` 目标地址，谁后加载谁的 patch 生效，行为未定义。若设备上已装着它（它自带同样的吸附修复），就别装 `hl-snap`；反过来要装它之前，先手动删掉 `extensions.d/hl-snap.so` 再 `systemctl restart xochitl`。

## 开关

网页「管理 → 系统增强」页的开关写的是 `~/.local/share/cangjie-ime/reading-qol.json` 里的 `hlSnapCjk`（布尔，默认开；`cangjie-ime` 是历史目录名，沿用未改）。扩展**每次划线时现读**这个文件，所以改了立即生效、不用重启；文件缺失/字段缺失时保持默认（开）。若设备上仍装着旧版原生「设置」页里的"笔记增强"面板，它写的是同一个键（该面板源码不在本仓库，未核对）。
