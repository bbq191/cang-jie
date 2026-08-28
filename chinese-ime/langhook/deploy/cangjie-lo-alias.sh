#!/bin/sh
# cangjie: 把 10.11.99.1 别名挂到 loopback，让设备端注入回传（星标卡片/生词本/笔记本
# → xochitl web POST /upload，该 web 只绑 USB gadget IP 10.11.99.1:80）在**不插 USB**
# 时也本机可达。不插 USB 时 usb 网卡 down、10.11.99.1 从所有接口消失 → POST 报
# Network unreachable(os error 101) → 全部回传失败（2026-08-28 真机根因）。
# 给 lo 加 /32 别名后该地址常驻、xochitl :80 socket 仍可本机 accept
# （真机验证：断 USB 画星→卡片笔记本更新成功）。幂等；不影响真插 USB 时
# host(10.11.99.2) 经 usb 网段访问设备 web。
if ip addr add 10.11.99.1/32 dev lo 2>/dev/null; then
    echo "[cangjie-lo-alias] 已加 10.11.99.1/32 到 lo"
else
    echo "[cangjie-lo-alias] 已存在或跳过"
fi
