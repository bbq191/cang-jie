#!/bin/sh
# ═══════════════════════════════════════════════════════════════════════════
# chrony-cn.sh —— reMarkable 设备时钟同步修复：把 chrony 的 time{1-4}.google.com（国内不通、设备从未同步过）
# 换成国内可达的 NTP，写进 **rootfs 底层 /etc/chrony.conf**（普通重启不丢；OTA 冲 rootfs 后重跑本脚本）。
#
# 用法（root，设备上跑；host 侧一行）：  ssh root@10.11.99.1 sh -s < packaging/chrony-cn.sh
# 幂等：底层已是国内配置就不再写 rootfs；overlay 视图与底层不一致才拷贝；chronyd 只在有改动或未同步时重启。
#
# 【为什么这么绕】/etc 是 overlay（lower=rootfs /etc，upper=/var/volatile tmpfs）：直接写 /etc 重启即丢。改底层要
#   ① 先 `mount -o remount,rw /` 再 `mount --bind /`（bind 继承绑定时刻的 ro 标志，顺序反了就 Read-only）；
#   ② overlay 缓存 lower，改完当场 /etc 看到的仍是旧内容——要立即生效再拷一份进 overlay（落 upper，重启自然消失、底层接管）；
#   ③ `remount,ro /` 偶发 busy，重试几次即可。设备没有 chronyc、busybox 没有 ntpd，验收看 timedatectl + journal。
#   书架白皮书 §03w / §05 第 10 条。本脚本不属于 shelf 安装器（书架不碰领域外系统配置）。
# ═══════════════════════════════════════════════════════════════════════════
set -u

SERVERS="ntp.aliyun.com ntp.tencent.com cn.pool.ntp.org time.cloudflare.com"
CONF=/etc/chrony.conf
BK_DIR=/home/root/cangjie-backups
BIND=/tmp/chrony-cn.rootbind

[ "$(id -u)" = "0" ] || { echo "!! 需 root"; exit 1; }
[ -f "$CONF" ] || { echo "!! 没有 $CONF"; exit 1; }

is_cn() { # $1=文件：不含 google 且含全部 4 个国内服务器
    ! grep -q "^server .*google" "$1" && for s in $SERVERS; do grep -q "^server $s " "$1" || return 1; done
}

rewrite() { # $1=源 $2=目标：第一条 server 行处换成 4 条国内，其余 server 行删掉
    awk -v servers="$SERVERS" '
        BEGIN { n = split(servers, S, " ") }
        /^server / { if (!done) { for (i = 1; i <= n; i++) print "server " S[i] " iburst minpoll 7"; done = 1 }; next }
        { print }' "$1" > "$2"
}

changed=0
if grep -q " /etc overlay " /proc/mounts; then
    # ── overlay：改 rootfs 底层 ──
    if dmsetup ls --target verity 2>/dev/null | grep -q .; then
        echo "✋ dm-verity 激活，rootfs 不可写：只改本次开机的 overlay 视图（重启会丢）"
    else
        mount -o remount,rw / || { echo "!! remount rw / 失败"; exit 1; }
        mkdir -p "$BIND" && mount --bind / "$BIND" || { mount -o remount,ro / 2>/dev/null; echo "!! bind / 失败"; exit 1; }
        LOWER="$BIND/etc/chrony.conf"
        if is_cn "$LOWER"; then
            echo "-- rootfs 底层已是国内 NTP，跳过"
        else
            mkdir -p "$BK_DIR"
            cp "$LOWER" "$BK_DIR/chrony.conf.bak.$(date +%Y%m%d-%H%M%S)"
            rewrite "$LOWER" "$LOWER.new" && mv "$LOWER.new" "$LOWER" && sync
            echo "-- rootfs 底层已改（备份在 $BK_DIR）"
            changed=1
        fi
        cp "$LOWER" /tmp/chrony-cn.lower
        umount "$BIND"; rmdir "$BIND" 2>/dev/null
        i=0
        while ! mount -o remount,ro / 2>/dev/null; do
            i=$((i + 1)); [ "$i" -ge 5 ] && { echo "⚠ remount ro / 一直 busy，rootfs 暂留 rw（重启恢复 ro）"; break; }
            sleep 2
        done
        # overlay 视图与底层不一致（overlay 缓存）→ 拷进 upper 让本次开机立即生效
        if ! cmp -s /tmp/chrony-cn.lower "$CONF"; then
            cp /tmp/chrony-cn.lower "$CONF" && echo "-- 已同步进 overlay（本次开机立即生效）" && changed=1
        fi
        rm -f /tmp/chrony-cn.lower
    fi
fi
# 非 overlay 或 verity：直接改 /etc 视图（幂等）
if ! is_cn "$CONF"; then
    rewrite "$CONF" "$CONF.new" && mv "$CONF.new" "$CONF" && changed=1 && echo "-- /etc 视图已改"
fi

synced() { timedatectl 2>/dev/null | grep -q "synchronized: yes"; }
if [ "$changed" = "1" ] || ! synced; then
    systemctl restart chronyd
    i=0
    while [ "$i" -lt 15 ] && ! synced; do sleep 2; i=$((i + 1)); done
fi
echo "-- servers: $(grep "^server " "$CONF" | awk '{print $2}' | tr '\n' ' ')"
if synced; then
    echo "✅ 时钟已同步：$(journalctl -u chronyd --no-pager -b 2>/dev/null | grep 'Selected source' | tail -n 1 | sed 's/.*Selected source //')  $(date '+%F %T %Z')"
else
    echo "⚠ 未同步（网络不通？）：journalctl -u chronyd 看原因"; exit 2
fi
