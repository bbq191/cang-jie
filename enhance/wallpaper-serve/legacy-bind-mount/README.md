# legacy-bind-mount —— 已退役的休眠壁纸方案（参考代码，不要运行）

> ⚠ **已退役，只作历史参考。不要在设备上运行本目录的任何脚本。**
>
> - **被什么取代**：2026-09-06 起用 xochitl 3.28 的隐藏配置键 `SleepScreenPath`（原生满屏、自动隐藏插画卡、每次休眠重读文件），实现是上级目录的 [`wallpaper-serve`](../README.md)，经网关网页「其他 → 壁纸」使用。
> - **为什么不能再用**：`install.sh` 会 `mount -o remount,rw /` 往 rootfs 的 `/usr/lib/systemd/` 写服务单元和 sleep 钩子，而且不经过 `packaging/devlib.sh` 的 dm-verity 门与带 trap 的读写窗口——这违反现行的"不写 `/usr`，非写不可时走 devlib 窗口"做法；sleep 钩子在充电时（按电源键内核不挂起）也不可靠。
> - **设备上万一还有残留**（从更老的备份恢复出来的 bind-mount、`cangjie-wallpaper.service` / `shelf-wallpaper-bind.service`、sleep 钩子）：按书架白皮书 §03x 手工清理。
> - **来历**：2026-08-27 作为个人小工具真机验证；09-03 并入书架（单元改名 `shelf-wallpaper-bind.service`）；09-05 发现 `SleepScreenPath` 后整套删掉；09-11 从顶层 `misc/wallpaper/` 归档到这里，随 `wallpaper-serve` 挪进 `enhance/`。来龙去脉见书架白皮书 §03w / §03x / §03ab。
>
> 下面是该方案 2026-08-27～09-05 的**原始记录**，原样保留，不代表当前状态（文中"不进仓库主线"等说法都已过时）。

---

## 原始记录：休眠壁纸(reMarkable Paper Pro Move)

设备休眠屏用**自备彩色图**替换,两张**每次唤醒自动交替**,并**消掉中央的原生"休眠插画卡"**。
不改 xochitl、不写系统真身文件(全 bind-mount)、不碰核心服务启动依赖。

### 原理(真机 2026-08-27 端到端验证)

- 休眠屏 = `/usr/share/remarkable/suspended.png`(满幅背景,**xochitl 每次休眠重读磁盘**,非开机缓存)
  \+ `/usr/share/remarkable/carousel/sleep_Illustration_0{1,2,3}.png`(776×776 中央插画卡,白底烘焙在 png 内)。
- **bind-mount 覆盖**这些文件(只读 rootfs 上也能挂,真身从不被改,`umount` 即完全还原):
  - `current.png` → `suspended.png`(满幅壁纸)
  - 全透明 `blank776.png` → 三张插画(插画卡整块消失)
- **交替**:reMarkable 定制 systemd-sleep 每次休眠跑 `/lib/systemd/system-sleep/` 钩子。
  钩子在**唤醒(after/post)**时 `roll.sh` 把 `current.png` 原地覆盖成另一张(保持 inode,bind 自动跟随);
  放到唤醒而非休眠,彻底避开跟 xochitl 休眠画图抢时序。参数约定 `before/after`(主)与 `pre/post` 都认,每周期防重滚。

### 文件

| 路径 | 作用 | OTA |
|---|---|---|
| `/home/root/wallpaper/1.png` `2.png` | 两张壁纸(**954×1696** 竖版) | 不丢 |
| `/home/root/wallpaper/current.png` `state` | bind 源 + 当前序号 | 不丢 |
| `/home/root/wallpaper/blank776.png` | 776×776 全透明(消插画卡) | 不丢 |
| `/home/root/wallpaper/{bind,unbind,roll,install}.sh` | 逻辑 | 不丢 |
| `/usr/lib/systemd/system/cangjie-wallpaper.service` | 开机建立全部 bind | **OTA 冲** |
| `/usr/lib/systemd/system-sleep/cangjie-wallpaper.sh` | 唤醒滚图 + before 补 bind | **OTA 冲** |

rootfs 两个文件被 OTA 冲掉后,重跑 `install.sh` 一键恢复(图片/逻辑在 /home 不受影响)。

### 常用操作

```sh
# 换图:覆盖 1.png / 2.png(必须 954×1696;host 端预处理见下),然后
/home/root/wallpaper/bind.sh          # 幂等,确保绑定在位

# 安装 / OTA 后恢复
sh /home/root/wallpaper/install.sh

# 彻底卸载(真身本就没改,撤挂即还原)
systemctl disable --now cangjie-wallpaper.service
/home/root/wallpaper/unbind.sh
# 再 remount,rw 删除那两个 rootfs 文件即可
```

### host 端图片预处理(Gallery 3)

```sh
# 缩到设备竖版尺寸 + 满幅裁切(cover)
convert in.jpg -resize 954x1696^ -gravity center -extent 954x1696 out.png
```
Gallery 3 CMYW 对高对比度/复杂渐变会洗屏久 + 噪点;选**低饱和、大色块、水彩/复古**类最佳。
需要主动控成像可加 Floyd-Steinberg 抖色到受限色板;低饱和照片一般直接让面板处理即可。

### 加更多张(>2)

`roll.sh` 目前两张交替。多张改成随机:
```sh
# roll.sh 里换成
next=$(ls "$DIR"/pool/*.png | shuf -n1)
cat "$next" > "$DIR/current.png"
```
（设备 busybox 自带 `shuf`。）

### 注意

- 这是**个人自用小工具**,与 cang-jie 主线(中文化/阅读/PKM/系统增强)无关,独立于 xovi/daemon,不进仓库主线。
- 未来固件可能改休眠屏机制(carousel 路径/合成方式);升级后若失效,按「原理」重新核对路径即可。
