# lo-alias —— 让 10.11.99.1 常驻可达

`lo-alias.sh` 解决一个网络问题：**不插 USB 时，xochitl 的 :80 网页上传口不可达**。xochitl 的网页接口只绑 USB 网段的 `10.11.99.1`；不插 USB 时 USB 网卡 down、这个地址从所有接口消失，网关等服务往 xochitl `/upload` 传书/回写就报 `Network unreachable`。脚本做两件事（幂等、失败不阻塞启动）：

1. 给回环接口 `lo` 挂 `10.11.99.1/32` 别名——保住"xochitl 已绑定之后"的本机可达；
2. 给 `usb1` 接口也挂同一个地址——治"无 USB 冷启动时 :80 根本没绑起来"：xochitl 启动选接口时若 `usb0` 无 carrier 就落到 `usb1` 且不再查 carrier，绑该接口的地址（反编译 `0x71e070` 坐实，2026-08-31 真机验证过）。

网关的 systemd 单元用它做 `ExecStartPre`，安装时随 `shelf` 步装到 `~/.local/bin/lo-alias.sh`。真插 USB 时电脑经 USB 网段访问设备不受影响。注意 xochitl 冷启动很慢（重启到 :80 就绪约 3 分钟），开机头几分钟 :80 缺失属正常。

## 来源

2026-09-11 **剥离移植**（copy，不是路径依赖）自旧中文化线的 `chinese-ime/langhook/deploy/cangjie-lo-alias.sh`——那次全仓库归档整理，`chinese-ime/` 挪出了仓库，原先路径引用它的部署脚本断了。脚本内容跟中文输入法无关，落到这里独立成副本，并改名 `lo-alias.sh`（往后新命名一律不带 `cangjie-` 前缀）。`packaging/deploy.sh` 打包时从这里取；设备端落点和网关单元的 `ExecStartPre` 都用新名字（安装/卸载会顺带清理旧名遗留，见 `shelf/manifest.sh`）。

后续这份副本独立维护，不再"改一处两边同步"。
