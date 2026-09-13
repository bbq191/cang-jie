# hl-snap —— 荧光笔 CJK 精确吸附

独立最小 xovi 扩展，只做一件事：CJK 划线时命中区间不向两边扩张成整行（划哪吸哪）。决策过程/真机验证见 `../docs/reMarkable系统增强线白皮书.md` §03a；技术细节见 `src/hl_snap.c` 头注。

## 构建

```sh
make aarch64 XOVI_DIR=<asivery/xovi clone 路径>
```

产物 `hl-snap.so`（已提交进仓库，跟 `chinese-ime/langhook/cangjie-langhook.so` 同样的约定——不用每次现建才能部署）。

## 部署

```sh
sudo sh deploy/install.sh
```

设备上需要先有 `vellum add xovi`（基石，跟 `chinese-ime/langhook` 共用同一份）。装的是 `extensions.d/hl-snap.so`，跟 `cangjie-langhook.so`/`appload.so`/`qt-resource-rebuilder.so` 并列。

**host 侧一键构建+推送+安装**：`packaging/deploy-hl-snap.sh <host>`（2026-09-11 新增，做的就是上面构建+部署两步的自动化，不改任何逻辑），也是 `packaging/install-all.sh` 全新设备统一安装器调用的其中一步，见 `../../packaging/README.md`。

⚠️ **不能跟完整版 `chinese-ime/langhook` 的 `cangjie-langhook.so` 同时部署**——两者都会尝试 patch 同一个 `FUN_00f05ad0`（荧光笔扩张函数）目标地址，谁后加载谁的 patch 生效，行为未定义。要装完整中文输入法前，先卸掉这个（删 `extensions.d/hl-snap.so`，`xovi/start` 重启一次）。

## 开关

跟设备原生「设置」App「系统增强→笔记增强」页、shelf 网页「管理→系统增强」页共用同一个开关键：`~/.local/share/cangjie-ime/reading-qol.json` 的 `hlSnapCjk`（布尔，默认开，改哪边都实时生效，不用重启）。
