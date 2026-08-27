#!/bin/sh
# battop 采集器 安装/OTA 重建(设备端跑,root)。
# 二进制+数据在 /home/root/battop(OTA 不丢);rootfs 只放 service+timer。
# OTA 后重跑:  sh /home/root/battop/install.sh
set -e
DIR=/home/root/battop
SVC=/usr/lib/systemd/system/battop.service
TMR=/usr/lib/systemd/system/battop.timer

[ -x "$DIR/battop" ] || { echo "缺 $DIR/battop(先 scp 二进制)"; exit 1; }
mkdir -p "$DIR/data"

echo "[*] remount rootfs rw"
mount -o remount,rw /

cat > "$SVC" <<'UNIT'
[Unit]
Description=battop battery/usage sampler (oneshot)

[Service]
Type=oneshot
Nice=19
IOSchedulingClass=idle
ExecStart=/home/root/battop/battop
UNIT

cat > "$TMR" <<'UNIT'
[Unit]
Description=battop sampler timer (~10min, awake-only)

[Timer]
OnBootSec=2min
OnUnitActiveSec=10min
AccuracySec=2min
# 不设 WakeSystem:休眠时不唤醒设备,只在醒着时机会性采样。
Persistent=false

[Install]
WantedBy=timers.target
UNIT

echo "[*] 首次采样(建 baseline)"
"$DIR/battop" || true

echo "[*] enable + start timer"
systemctl daemon-reload
systemctl enable --now battop.timer >/dev/null 2>&1 || true

echo "[*] remount rootfs ro"
mount -o remount,ro / || echo "  (remount ro 失败,重启回 ro,无碍)"

echo "[OK] battop 已装。timer 状态:"
systemctl is-active battop.timer
systemctl list-timers battop.timer --no-pager 2>/dev/null | head -n 2
