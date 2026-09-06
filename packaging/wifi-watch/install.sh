#!/bin/sh
# 装 wifi-watch（设备上 root 跑；host 侧：scp 本目录到设备后 `sh install.sh`，或 `sh install.sh --uninstall`）。
# 脚本落 ~/.local/bin（/home，OTA 不丢），单元落 /usr/lib/systemd/system（OTA 冲掉后重跑本脚本；dm-verity 激活则跳过写 /usr）。
set -eu
HERE="$(cd "$(dirname "$0")" && pwd)"
BIN=/home/root/.local/bin/wifi-watch.sh
SYSD=/usr/lib/systemd/system
if [ "${1:-}" = "--uninstall" ]; then
    systemctl disable --now wifi-watch.service 2>/dev/null || true
    mount -o remount,rw / 2>/dev/null || true
    rm -f "$SYSD/wifi-watch.service" "$SYSD/multi-user.target.wants/wifi-watch.service" "$BIN"
    sync; mount -o remount,ro / 2>/dev/null || true; systemctl daemon-reload
    echo "-- wifi-watch 已卸载"; exit 0
fi
mkdir -p "$(dirname "$BIN")"
cp "$HERE/wifi-watch.sh" "$BIN" && chmod 755 "$BIN"
if dmsetup ls --target verity 2>/dev/null | grep -q .; then
    echo "✋ dm-verity 激活，不写 /usr：手动 nohup sh $BIN & 或走 xovi post-start"; exit 0
fi
mount -o remount,rw / || { echo "!! remount rw / 失败"; exit 1; }
cp "$HERE/wifi-watch.service" "$SYSD/wifi-watch.service" && chmod 644 "$SYSD/wifi-watch.service"
mkdir -p "$SYSD/multi-user.target.wants" && ln -sf ../wifi-watch.service "$SYSD/multi-user.target.wants/wifi-watch.service"
sync
i=0; while ! mount -o remount,ro / 2>/dev/null; do i=$((i + 1)); [ "$i" -ge 5 ] && break; sleep 2; done
systemctl daemon-reload && systemctl restart wifi-watch.service
sleep 1; echo "-- wifi-watch: $(systemctl is-active wifi-watch.service)（journalctl -u wifi-watch）"
