#!/bin/sh
# ─────────────────────────────────────────────────────────────
# cj-stars（★ 全局待办 daemon）持久开机自启 —— 把 unit + enable 符号链接都放进
# rootfs（/usr），不放易失的 /etc（overlay tmpfs、重启即清）。
#
# ⚠ 本脚本写 /usr（rootfs）。安全前提（2026-08-24 真机核查）：
#   · 本设备 rootfs = /dev/mmcblk0p3 ext4 **ro**（remount rw 可写），
#     `dmsetup ls` 无 verity target → **dm-verity 未激活**；
#   · cj-stars.service 自 2026-08-22 就在 /usr、历经重启未变砖；
#   · 工程纪律 确认本项目当前就用 /usr rootfs 做开机持久（xovi 同法）。
#   （2026-08-16 那次"写 /usr + 重启 → A/B 回滚变砖"是 dm-verity 激活的旧配置，
#    不适用当前固件。若换回 verity 固件，本脚本不可用——先复核 dmsetup。）
#
# 红线：cj-stars.service `After=home.mount`、**绝不依赖 xochitl**（不牵连本体）。
# OTA 冲掉 rootfs 后重跑本脚本即恢复。
#
# 用法：./enable-cj-stars-boot.sh [host]   （host 默认 10.11.99.1）
#   反做（禁用持久自启）：./enable-cj-stars-boot.sh [host] disable
# ─────────────────────────────────────────────────────────────
# shellcheck disable=SC2029  # $UNIT/$WANTS 等是固定字面量路径，有意客户端展开后传给远端（远端再用单引号包住）
set -e
cd "$(dirname "$0")"

HOST="${1:-10.11.99.1}"
ACTION="${2:-enable}"
SVC=cj-stars.service
SRC="../../reading/device-rs/systemd/$SVC"
UNIT=/usr/lib/systemd/system/$SVC
WANTS=/usr/lib/systemd/system/multi-user.target.wants/$SVC

[ -f "$SRC" ] || { echo "缺 $SRC"; exit 1; }

if [ "$ACTION" = "disable" ]; then
    ssh "root@$HOST" "
        set -e
        mount -o remount,rw /
        rm -f '$WANTS'
        sync
        mount -o remount,ro / || true
        systemctl daemon-reload
        echo '已移除 rootfs 持久自启符号链接（unit 仍在，可手动 start）；is-enabled:'
        systemctl is-enabled cj-stars || true
    "
    exit 0
fi

# enable：装 unit（以仓库为准）+ 建 rootfs .wants 符号链接
ssh "root@$HOST" "
    set -e
    mount -o remount,rw /
    mkdir -p '$(dirname "$UNIT")' '$(dirname "$WANTS")'
    cat > '$UNIT'          # busybox 无 install，用 cat 写 unit（以仓库为准）
    chmod 644 '$UNIT'
    ln -sf ../$SVC '$WANTS'
    sync
    mount -o remount,ro / || true
    systemctl daemon-reload
    echo '── enable 结果 ──'
    echo -n 'is-enabled: '; systemctl is-enabled cj-stars || true
    echo -n '符号链接（应在 /usr rootfs）: '; ls -l '$WANTS'
" < "$SRC"
