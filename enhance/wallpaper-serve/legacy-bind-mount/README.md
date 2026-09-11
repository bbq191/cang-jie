> ⚠ **已退役，纯历史存档**（2026-09-11 从顶层 `misc/wallpaper/` 归档到这里）。2026-09-06 起 xochitl 3.28 隐藏键 `SleepScreenPath` 原生方案取代了本文档描述的 bind-mount 整套机制，现行实现在 `../`（`shelf/wallpaper/README.md` + `services/wallpaper-serve`）。下文是该方案 2026-08-27～09-05 的原始记录，不代表当前状态，保留供追溯参考。
>
> ⚠ 2026-09-03：本工具计划并入书架 `shelf/`（`wallpaper-serve` 上传即用 + Rust 子命令承接 bind/roll/unbind，XDG 路径），Phase 2 落地后本目录只留指针。当前仍可独立使用。

# 休眠壁纸(reMarkable Paper Pro Move)

设备休眠屏用**自备彩色图**替换,两张**每次唤醒自动交替**,并**消掉中央的原生"休眠插画卡"**。
不改 xochitl、不写系统真身文件(全 bind-mount)、不碰核心服务启动依赖。

## 原理(真机 2026-08-27 端到端验证)

- 休眠屏 = `/usr/share/remarkable/suspended.png`(满幅背景,**xochitl 每次休眠重读磁盘**,非开机缓存)
  \+ `/usr/share/remarkable/carousel/sleep_Illustration_0{1,2,3}.png`(776×776 中央插画卡,白底烘焙在 png 内)。
- **bind-mount 覆盖**这些文件(只读 rootfs 上也能挂,真身从不被改,`umount` 即完全还原):
  - `current.png` → `suspended.png`(满幅壁纸)
  - 全透明 `blank776.png` → 三张插画(插画卡整块消失)
- **交替**:reMarkable 定制 systemd-sleep 每次休眠跑 `/lib/systemd/system-sleep/` 钩子。
  钩子在**唤醒(after/post)**时 `roll.sh` 把 `current.png` 原地覆盖成另一张(保持 inode,bind 自动跟随);
  放到唤醒而非休眠,彻底避开跟 xochitl 休眠画图抢时序。参数约定 `before/after`(主)与 `pre/post` 都认,每周期防重滚。

## 文件

| 路径 | 作用 | OTA |
|---|---|---|
| `/home/root/wallpaper/1.png` `2.png` | 两张壁纸(**954×1696** 竖版) | 不丢 |
| `/home/root/wallpaper/current.png` `state` | bind 源 + 当前序号 | 不丢 |
| `/home/root/wallpaper/blank776.png` | 776×776 全透明(消插画卡) | 不丢 |
| `/home/root/wallpaper/{bind,unbind,roll,install}.sh` | 逻辑 | 不丢 |
| `/usr/lib/systemd/system/cangjie-wallpaper.service` | 开机建立全部 bind | **OTA 冲** |
| `/usr/lib/systemd/system-sleep/cangjie-wallpaper.sh` | 唤醒滚图 + before 补 bind | **OTA 冲** |

rootfs 两个文件被 OTA 冲掉后,重跑 `install.sh` 一键恢复(图片/逻辑在 /home 不受影响)。

## 常用操作

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

## host 端图片预处理(Gallery 3)

```sh
# 缩到设备竖版尺寸 + 满幅裁切(cover)
convert in.jpg -resize 954x1696^ -gravity center -extent 954x1696 out.png
```
Gallery 3 CMYW 对高对比度/复杂渐变会洗屏久 + 噪点;选**低饱和、大色块、水彩/复古**类最佳。
需要主动控成像可加 Floyd-Steinberg 抖色到受限色板;低饱和照片一般直接让面板处理即可。

## 加更多张(>2)

`roll.sh` 目前两张交替。多张改成随机:
```sh
# roll.sh 里换成
next=$(ls "$DIR"/pool/*.png | shuf -n1)
cat "$next" > "$DIR/current.png"
```
（设备 busybox 自带 `shuf`。）

## 注意

- 这是**个人自用小工具**,与 cang-jie 主线(中文化/阅读/PKM/系统增强)无关,独立于 xovi/daemon,不进仓库主线。
- 未来固件可能改休眠屏机制(carousel 路径/合成方式);升级后若失效,按「原理」重新核对路径即可。
