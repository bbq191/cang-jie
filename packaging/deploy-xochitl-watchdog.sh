#!/bin/sh
# host 侧一键放宽 xochitl 的 systemd 看门狗（官方单元 WatchdogSec=60 → 300 秒）。
#
# 为什么（2026-10-15 真机）：几万页的合集（梁羽生武侠全集 40492 页、古龙作品全集 28128 页）打开或重排时，xochitl
# 主线程要连续满负荷算约 66 秒（采样 /proc 实测：纯用户态 CPU、几乎无系统调用、内存平稳），超过 60 秒就被 systemd
# 以 SIGABRT 杀掉重拉——用户看到的是"打开大书就重启"。临时关掉看门狗（systemctl service-watchdogs no）实测算完即
# 恢复、能正常翻页，不是死循环，所以放宽时限就够。代价：xochitl 真卡死时要 5 分钟才被自动拉起。
#
# 怎么做：只往 xovi 的持久源目录 /home/root/xovi/services/xochitl.service/ 放一份 drop-in（xochitl-watchdog.conf）。
# xovi/start（开机由 xovi-reenable 跑）把这个目录整份拷进 /etc/systemd/system/xochitl.service.d/（tmpfs）再
# daemon-reload，所以**整机重启后才生效**——本步骤只落盘 + 记待生效标记，由 install-all 最后的 xovi-apply 统一整机
# 重启；单独跑完后请 `sh deploy-xovi-apply.sh`。不碰 /usr、不碰 /etc 底层、不 restart xochitl。
# 内容没变就什么都不做（不记待生效，不会白白重启）。
#
# 用法：./deploy-xochitl-watchdog.sh [host]      host 默认 10.11.99.1
set -eu
cd "$(dirname "$0")"
# shellcheck disable=SC1091
. ./lib.sh
host_arg "用法：./deploy-xochitl-watchdog.sh [host]      host 默认 10.11.99.1" "$@"
require_device

dev_script "$XOCHITL_WATCHDOG_SEC" <<'DEVICE_SCRIPT'
set -eu
SEC="$1"
D="$CJ_XOVI/services/xochitl.service"
F="$D/xochitl-watchdog.conf"
if [ ! -d "$D" ]; then
    echo "-- 设备没有 $D（没装 xovi/vellum），跳过：看门狗保持官方 60 秒"
    exit 0
fi
WANT="# cang-jie：几万页的书打开/重排时 xochitl 主线程要算 60 秒以上，官方 60 秒看门狗会把它杀掉重启
[Service]
WatchdogSec=$SEC"
if [ -f "$F" ] && [ "$(cat "$F")" = "$WANT" ]; then
    echo "-- $F 已是 WatchdogSec=$SEC，不用改"
    exit 0
fi
T="$D/.xochitl-watchdog.conf.new.$$"
printf '%s\n' "$WANT" > "$T"
chmod 644 "$T"
mv -f "$T" "$F"
echo "-- 已写 $F（WatchdogSec=$SEC），整机重启后生效"
cj_pending_mark xochitl-watchdog || true
DEVICE_SCRIPT
