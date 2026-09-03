#!/bin/sh
# rmweb × Paper Pro Move 门控 spike（设备端，root）。每步人工确认；绝不写 systemd/开机路径。
# 用法: spike.sh recon | fetch <tar.gz> | run [url] | restore
set -eu
HOME_DIR="${HOME:-/home/root}"
DIR="${XDG_DATA_HOME:-$HOME_DIR/.local/share}/shelf/rmweb"
FINDINGS="$DIR/findings.md"
WD_PID="$DIR/.watchdog.pid"
mkdir -p "$DIR"

case "${1:-}" in
recon)
    {
        echo "# rmweb × Move 侦查 $(date '+%F %T')"
        echo "- version: $(cat /etc/version 2>/dev/null || echo ?)"
        echo "- kernel: $(uname -r)"
        echo "- machine: $(cat /sys/devices/soc0/machine 2>/dev/null || echo ?)"
        echo "- mem: $(grep MemTotal /proc/meminfo)"
        echo "- qpa plugins: $(ls /usr/lib/plugins/platforms/ 2>/dev/null | tr '\n' ' ')"
        echo "- dri: $(ls /dev/dri 2>/dev/null | tr '\n' ' ')"
        echo "- fb: $(ls /dev/fb* 2>/dev/null | tr '\n' ' ' || true)"
        echo "- xochitl: $(systemctl is-active xochitl) NRestarts=$(systemctl show xochitl -p NRestarts --value)"
    } | tee -a "$FINDINGS"
    ;;
fetch)
    tgz="${2:?用法: spike.sh fetch <rmweb-*.tar.gz>}"
    tar -C "$DIR" -xzf "$tgz"
    echo "-- 解到 $DIR"; ls "$DIR"
    bin="$(find "$DIR" -maxdepth 3 -type f -name 'rmweb*' -perm -u+x | head -n1)"
    [ -n "$bin" ] && { echo "-- ldd $bin"; ldd "$bin" 2>&1 | grep -i "not found" | tee -a "$FINDINGS" || echo "   （无缺库）"; }
    ;;
run)
    url="${2:-https://ink.qq.com}"
    launcher="$(find "$DIR" -maxdepth 3 -type f \( -name 'run.sh' -o -name 'rmweb.sh' -o -name 'launch*.sh' \) | head -n1)"
    bin="$(find "$DIR" -maxdepth 3 -type f -name 'rmweb' -perm -u+x | head -n1)"
    [ -n "$launcher$bin" ] || { echo "!! 找不到 rmweb 可执行/启动脚本，先 fetch"; exit 1; }
    echo "-- 看门狗：600s 后自动 systemctl start xochitl（防挂死）"
    ( sleep 600; systemctl start xochitl ) & echo $! > "$WD_PID"
    echo "-- 停 xochitl，前台跑 rmweb（Ctrl+C 退出后请 spike.sh restore）"
    systemctl stop xochitl
    echo "$(date '+%T') run url=$url launcher=${launcher:-$bin}" >> "$FINDINGS"
    if [ -n "$launcher" ]; then sh "$launcher" "$url" || echo "rmweb 退出码 $?" | tee -a "$FINDINGS"
    else "$bin" "$url" || echo "rmweb 退出码 $?" | tee -a "$FINDINGS"; fi
    ;;
restore)
    if [ -f "$WD_PID" ]; then kill "$(cat "$WD_PID")" 2>/dev/null || true; rm -f "$WD_PID"; fi
    systemctl start xochitl
    sleep 3
    echo "- restore $(date '+%T'): xochitl $(systemctl is-active xochitl) NRestarts=$(systemctl show xochitl -p NRestarts --value)" | tee -a "$FINDINGS"
    ;;
*)
    echo "用法: spike.sh recon | fetch <tar.gz> | run [url] | restore"; exit 2 ;;
esac
