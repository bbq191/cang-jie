#!/bin/sh
# wifi-watch —— 常驻看护 wlan0 载波（装为 systemd 服务 wifi-watch.service，/usr 单元，OTA 后重装）。
# 定位（2026-09-06）：连上 60 s 必掉的真凶是 cfg80211 regdomain 宽限——精简 regulatory.db 的 CN 不含 5150–5350，路由 5G 信道 36
# 被判非法而断开（书架白皮书 §03w），根治是锁 2.4G 或路由改 149+ 信道。本脚本只做兜底：slumber 醒来后 wlan0 偶发假死
# （NetworkManager 仍标 connected、永不自愈）时 `nmcli con up`。⚠ `nmcli con up` 对已激活连接会先断再连，所以判据必须是真 NO-CARRIER。
# 做法：每 INTERVAL 秒看一次；wlan0 存在、rfkill 未软锁、NM 有 wifi 连接、却连续 STRIKES 次 NO-CARRIER → `nmcli con up`。
# 只在"NM 以为连着但链路死了"时动手；用户关 WiFi（rfkill/NM 断开）不干预。日志 journalctl -u wifi-watch。
IFACE=${IFACE:-wlan0}
INTERVAL=${INTERVAL:-15}
STRIKES=${STRIKES:-2}
strikes=0
while :; do
    sleep "$INTERVAL"
    [ -e "/sys/class/net/$IFACE" ] || continue
    if rfkill list wifi 2>/dev/null | grep -q "Soft blocked: yes"; then strikes=0; continue; fi
    con="$(nmcli -t -f DEVICE,NAME con show --active 2>/dev/null | grep "^$IFACE:" | head -n 1 | cut -d: -f2-)"
    [ -n "$con" ] || { strikes=0; continue; }
    if ip link show "$IFACE" 2>/dev/null | grep -q NO-CARRIER; then
        strikes=$((strikes + 1))
        if [ "$strikes" -ge "$STRIKES" ]; then
            out="$(nmcli con up "$con" 2>&1 | tail -n 1)"
            echo "$IFACE NO-CARRIER x$strikes → nmcli up '$con': $out"
            strikes=0
        fi
    else
        strikes=0
    fi
done
