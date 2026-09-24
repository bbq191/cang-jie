# lo-alias —— 让 10.11.99.1 常驻可达

**一句话**：不插 USB 时，xochitl 的网页上传口（`10.11.99.1:80`，`POST /upload`）会变得不可达；`lo-alias.sh` 给网卡补挂这个地址，让本机服务照样能往 xochitl 传书。

## 为什么需要它

xochitl 自带的网页上传接口只绑 USB 网卡的 `10.11.99.1`。不插 USB 时 USB 网卡 down，这个地址从所有接口上消失，网关传书、笔记推送这类"往 xochitl `/upload` 发请求"的操作就报 `Network unreachable`。

脚本做两件事（幂等，失败不阻塞启动）：

| 步骤 | 做什么 | 解决什么 |
|---|---|---|
| 1 | 给回环接口 `lo` 挂 `10.11.99.1/32` | xochitl **已经绑好** :80 之后，拔掉 USB 本机仍能访问 |
| 2 | 给 `usb1` 接口也挂同一个地址 | **不插 USB 冷启动**时 :80 根本没绑起来：xochitl 启动选接口时 `usb0` 无 carrier 就落到 `usb1`，且不再查 carrier，直接绑该接口的地址（反编译 `0x71e070` 坐实，2026-08-31 真机验证） |

真插 USB 时电脑经 USB 网段访问设备不受影响。

## 谁调用、装在哪

- 网关单元 `gateway.service` 的 `ExecStartPre=-/bin/sh /home/root/.local/bin/lo-alias.sh`（前面的 `-` 表示失败也不拦网关启动）。
- 安装随 `packaging/deploy.sh` 的 shelf 载荷一起走，落到 `~/.local/bin/lo-alias.sh`；旧名 `cangjie-lo-alias.sh` 在安装/卸载时自动清理（见 `shelf/manifest.sh`）。
- 本目录不单独部署。

## 已知限制

- **时序没有保证**：脚本挂在网关启动前，不在 xochitl 启动前。2026-09-24 那次开机日志里它比 xochitl 晚 3 秒运行；xochitl 冷启动到绑 :80 约要 3 分钟，所以地址多半赶得上，但"不插 USB 冷启动"这一场景在现行接法下**没有重新真机验证**。
- 开机头几分钟 :80 缺失属正常，依赖它的服务应失败即重试。

## 来源

2026-09-11 从旧中文化线的 `chinese-ime/langhook/deploy/cangjie-lo-alias.sh` **拷贝**过来（`chinese-ime/` 已移出仓库），改名去掉 `cangjie-` 前缀。之后这份副本独立维护，不再和原件同步。
