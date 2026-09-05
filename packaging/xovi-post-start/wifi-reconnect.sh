#!/bin/sh
# xovi post-start 钩子（装到 /home/root/xovi/scripts/post-start/wifi-reconnect.sh，xovi/start 重启 xochitl 后自动跑）。
# 现象（3.28.0.172 真机 2026-09-05）：xochitl 停止后十几秒 wlan0 丢载波（Lost carrier + regdom 重置 WORLD），
# NetworkManager 却仍标"已连接"、不自愈，WiFi 地址上的书架/SSH 全断；`nmcli con up <连接>` 一下就回来。
# 做法：脱离 xovi/start 后台轮询 60s，见 NO-CARRIER 就 `nmcli con up` 当前 WiFi 连接（最多 2 次）。
# 日志：journalctl -t xovi-wifi。WiFi 本就关着/没配置时 nmcli 失败只记日志，不影响任何东西。
IFACE=wlan0
TAG=xovi-wifi

poll() {
    i=0
    tries=0
    while [ "$i" -lt 12 ] && [ "$tries" -lt 2 ]; do
        sleep 5
        i=$((i + 1))
        ip link show "$IFACE" 2>/dev/null | grep -q NO-CARRIER || continue
        tries=$((tries + 1))
        con="$(nmcli -t -f DEVICE,NAME con show --active 2>/dev/null | grep "^$IFACE:" | head -n 1 | cut -d: -f2-)"
        if [ -n "$con" ]; then
            out="$(nmcli con up "$con" 2>&1 | tail -n 1)"
        else
            out="$(nmcli dev connect "$IFACE" 2>&1 | tail -n 1)"
        fi
        logger -t "$TAG" "t+$((i * 5))s $IFACE NO-CARRIER → nmcli up '${con:-$IFACE}': $out"
    done
    logger -t "$TAG" "done: $(ip link show "$IFACE" 2>/dev/null | grep -oE 'NO-CARRIER|LOWER_UP' | head -n 1) tries=$tries"
}

if [ "${1:-}" = "--poll" ]; then
    poll
    exit 0
fi
logger -t "$TAG" "xovi/start post-start: 后台看护 $IFACE 载波 60s"
if command -v setsid >/dev/null 2>&1; then
    setsid sh "$0" --poll >/dev/null 2>&1 </dev/null &
else
    nohup sh "$0" --poll >/dev/null 2>&1 </dev/null &
fi
exit 0
