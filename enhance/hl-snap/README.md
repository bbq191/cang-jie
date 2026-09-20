# hl-snap —— 荧光笔 CJK 精确吸附

独立最小 xovi 扩展，只做一件事：CJK 划线时命中区间不向两边扩张成整行（划哪吸哪）。决策过程/真机验证见 `../docs/reMarkable系统增强线白皮书.md` §03a；技术细节见 `src/hl_snap.c` 头注。

## 构建

```sh
make aarch64 XOVI_DIR=<asivery/xovi clone 路径>
```

产物 `hl-snap.so`（已提交进仓库，跟 `chinese-ime/langhook/cangjie-langhook.so` 同样的约定——不用每次现建才能部署）。

## 部署

**推荐（host 侧一键构建+推送+安装）**：`cd packaging && sh deploy-hl-snap.sh <host>`。它构建 `.so`、把 `.so`、`deploy/install.sh`、`packaging/xovi-ext-install.sh`、`packaging/devlib.sh` 一起推到设备的暂存目录（md5 逐个校验），再跑设备端 `install.sh`；也是 `packaging/install-all.sh` 调用的一步（那时用 `DEFER_XOVI_START=1`，只落盘、最后统一重启），见 `../../packaging/README.md`。

**手动在设备上跑**：`deploy/install.sh` 现在只剩这个扩展的数据（名字、配置键），流程在 `packaging/xovi-ext-install.sh`——所以**同目录必须有 `xovi-ext-install.sh` 与 `devlib.sh`，只拷单个 `install.sh` 不够**（缺了会清楚报错并提示用 `deploy-hl-snap.sh`）：

```sh
sh deploy/install.sh [--no-restart]     # --no-restart：只落盘不重启 xochitl
```

设备上需要先有 `vellum add xovi`。装的是 `extensions.d/hl-snap.so`，跟 `appload.so`/`qt-resource-rebuilder.so` 并列。行为：旧 `.so` 先备份进 `~/cangjie-backups/`（**绝不留在 `extensions.d/`**，xovi 会把该目录下任意文件当扩展加载）；用"先写暂存目录再 rename"原子替换，不在运行中 xochitl 已映射的文件上原地写；`reading-qol.json` 首次才建、不覆盖已有设置。不带 `--no-restart` 时：xovi 已在 xochitl 里生效 → `systemctl restart xochitl`，否则 `xovi/start`（xovi 已生效时跑 `xovi/start` 会让 xochitl SEGV 整机重启，见 `../../packaging/README.md`）。

⚠️ **不能跟完整版 `chinese-ime/langhook` 的 `cangjie-langhook.so` 同时部署**——两者都会尝试 patch 同一个 `FUN_00f05ad0`（荧光笔扩张函数）目标地址，谁后加载谁的 patch 生效，行为未定义。要装完整中文输入法前，先卸掉这个（删 `extensions.d/hl-snap.so`，`xovi/start` 重启一次）。

## 开关

跟设备原生「设置」App「系统增强→笔记增强」页、shelf 网页「管理→系统增强」页共用同一个开关键：`~/.local/share/cangjie-ime/reading-qol.json` 的 `hlSnapCjk`（布尔，默认开，改哪边都实时生效，不用重启）。
