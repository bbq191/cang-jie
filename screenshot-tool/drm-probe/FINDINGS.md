# reMarkable Paper Pro 截屏可行性调研（2026-08-23）

**结论：独立进程无法截屏，必须 hook xochitl 进程内。**。

## 设备显示子系统（Paper Pro Move, imx93 彩色）

- **无 `/dev/fb*`**（无传统 framebuffer）；用 **DRM/KMS**：`/dev/dri/card0` + connector `card0-LVDS-1`。
- xochitl 用 **atomic KMS**：legacy `drmModeGetCrtc().buffer_id = 0`，fb 挂在 plane 上。
- 设备 `/usr/lib/libdrm.so.2.4.0`；无现成截屏工具。

## 为什么独立进程读不到画面（用设备 libdrm 权威测过）

非 DRM-master 进程实测：
```
GETRESOURCES: fbs=0        # 看不到 framebuffer 列表
crtc 34: buffer_id=0       # legacy crtc 无 fb（atomic 驱动）
planes=1, 每个 plane fb_id=0  # 看不到 plane 当前 fb
```
根因：**Linux DRM 安全模型**——framebuffer 与 atomic 显示状态对非 master client 隐藏（防偷窥）。`xochitl` 独占 DRM master。`drmModeGetFB2` 对非 owner 的 fb 会被拒。唯一绕过 `drmSetMaster` 会打断 xochitl（画面冻结），不可接受。

## 唯一可行路径：hook xochitl（进程内，它是 master + fb owner）

用 langhook 特征码 hook 拦 xochitl 的 `drmModeAtomicCommit`，从提交的 plane 拿 `fb_id` → `drmModeGetFB2` → `drmPrimeHandleToFD` → `mmap` 读像素（进程内 owner 可读）。

**待解决**：① 彩色 buffer 像素格式 + tiling（`modifier`）；② PNG 编码；③ 触发机制；④ 录屏=连续抓帧。工作量：中等偏大逆向工程（评估后暂不开发）。

## 文件

- `drmshot2.c` — **libdrm 版**（权威，就是它证明非 master 拿不到 fb）。编译：
  `aarch64-linux-gnu-gcc drmshot2.c -I/usr/include/libdrm -I/usr/include <设备 libdrm.so.2> -o drmshot2`
- `drmshot.c` — 纯 ioctl 版（不依赖 libdrm，静态编译；GETRESOURCES 第二次 EFAULT 未完全定位，可能系统 DRM 头 vs 设备内核版本差异，非阻断——libdrm 版已给出权威结论）。
