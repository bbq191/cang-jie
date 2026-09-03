#!/bin/sh
# ═══════════════════════════════════════════════════════════════════════════
# 书架（shelf）设备端安装器 —— reMarkable Paper Pro Move，root 运行。可独立于中文化套件安装。
#
# 装什么（按服务可插拔，--only 挑选）：
#   · 二进制 → ~/.local/bin/（XDG 用户可执行目录）；cangjie-lo-alias.sh 同目录（网关 ExecStartPre 用，
#     让 10.11.99.1 常驻可达以便 /upload 注入；脚本单一事实源在 chinese-ime/langhook/deploy，包内为同一份）
#   · XDG 目录：~/.config/shelf  ~/.local/share/shelf  ~/.local/state/shelf
#   · systemd：shelf.target + 各服务单元 → /usr/lib/systemd/system（rootfs，普通重启不丢；OTA 冲掉后重跑本脚本）
# 写 /usr 前实检 dm-verity，激活即跳过（ 红线）；绝不给 xochitl 加依赖。
#
# 用法：./install.sh [--only gateway,book,koreader,font,wallpaper] [--no-systemd] [--src DIR]
#   --only        只装/更新列出的服务（网关总会装）；缺省全装
#   --no-systemd  只落二进制与目录，不碰 /usr（重启后需手动 systemctl start）
#   --src DIR     载荷目录（含 bin/ systemd/ lo-alias/），缺省=本脚本所在目录
# 幂等，可反复跑；每次先把现有二进制备份到 /home/root/cangjie-backups/shelf-<时间>/。
# ═══════════════════════════════════════════════════════════════════════════
set -eu

HERE="$(cd "$(dirname "$0")" && pwd)"
SRC="$HERE"
ONLY=""
DO_SYSTEMD=1
for a in "$@"; do
    case "$a" in
        --only=*) ONLY="${a#--only=}" ;;
        --only) ;;                                   # 下一个参数是列表
        --no-systemd) DO_SYSTEMD=0 ;;
        --src=*) SRC="${a#--src=}" ;;
        --src) ;;
        *) if [ -n "${_prev:-}" ]; then
               case "$_prev" in --only) ONLY="$a" ;; --src) SRC="$a" ;; esac
           else
               echo "!! 未知参数：$a"; exit 2
           fi ;;
    esac
    case "$a" in --only|--src) _prev="$a" ;; *) _prev="" ;; esac
done

HOME_DIR="${HOME:-/home/root}"
BIN_DIR="$HOME_DIR/.local/bin"
XDG_CONFIG_HOME="${XDG_CONFIG_HOME:-$HOME_DIR/.config}"
XDG_DATA_HOME="${XDG_DATA_HOME:-$HOME_DIR/.local/share}"
XDG_STATE_HOME="${XDG_STATE_HOME:-$HOME_DIR/.local/state}"
SYSD=/usr/lib/systemd/system
BK="$HOME_DIR/cangjie-backups/shelf-$(date +%Y%m%d-%H%M%S)"

ALL="gateway book koreader font wallpaper"
[ -n "$ONLY" ] && SEL="gateway $(echo "$ONLY" | tr ',' ' ')" || SEL="$ALL"
svc_of() { case "$1" in gateway) echo shelf-gateway ;; *) echo "$1-serve" ;; esac; }

echo "═══ 书架 shelf 安装（$(echo "$SEL" | tr ' ' ',')）═══"
[ "$(id -u)" = "0" ] || { echo "!! 需 root"; exit 1; }
[ -d "$SRC/bin" ] || { echo "!! 载荷缺 $SRC/bin/"; exit 1; }

# ── 1. 备份现有二进制 ──
mkdir -p "$BK" "$BIN_DIR" "$XDG_CONFIG_HOME/shelf" "$XDG_DATA_HOME/shelf" "$XDG_STATE_HOME/shelf"
for s in $SEL; do
    b="$(svc_of "$s")"
    [ -f "$BIN_DIR/$b" ] && cp "$BIN_DIR/$b" "$BK/"
done
echo "-- 备份：$BK"

# ── 2. 二进制 + lo 别名脚本 → ~/.local/bin ──
for s in $SEL; do
    b="$(svc_of "$s")"
    [ -f "$SRC/bin/$b" ] || { echo "!! 载荷缺 bin/$b"; exit 1; }
    cp "$SRC/bin/$b" "$BIN_DIR/$b" && chmod 755 "$BIN_DIR/$b"
done
if [ -f "$SRC/lo-alias/cangjie-lo-alias.sh" ]; then
    cp "$SRC/lo-alias/cangjie-lo-alias.sh" "$BIN_DIR/cangjie-lo-alias.sh" && chmod 755 "$BIN_DIR/cangjie-lo-alias.sh"
fi
echo "-- 二进制已落 $BIN_DIR"

# ── 3. systemd（写 /usr rootfs；dm-verity 门）──
if [ "$DO_SYSTEMD" = "0" ]; then
    echo "-- --no-systemd：跳过单元。手动：$BIN_DIR/shelf-gateway serve"
elif dmsetup ls --target verity 2>/dev/null | grep -q .; then
    echo "✋ dm-verity 激活 —— 跳过写 /usr（不装开机持久，避免变砖）。"
elif [ ! -d "$SRC/systemd" ]; then
    echo "-- 载荷无 systemd/，跳过"
else
    mount -o remount,rw / || { echo "!! remount rw / 失败"; exit 1; }
    (
        set -e
        cd "$SYSD"
        cp "$SRC/systemd/shelf.target" shelf.target && chmod 644 shelf.target
        mkdir -p multi-user.target.wants shelf.target.wants
        ln -sf ../shelf.target multi-user.target.wants/shelf.target
        for s in $SEL; do
            u="$(svc_of "$s").service"
            cp "$SRC/systemd/$u" "$u" && chmod 644 "$u"
            ln -sf "../$u" "shelf.target.wants/$u"
        done
        sync
    )
    mount -o remount,ro / || true
    systemctl daemon-reload
    for s in $SEL; do systemctl restart "$(svc_of "$s").service" 2>/dev/null || true; done
    systemctl start shelf.target 2>/dev/null || true
    echo "-- 单元已写入 /usr 并启动（shelf.target；单个服务可 systemctl disable --now <svc>）"
fi

# ── 4. 健康检查 ──
sleep 1
GET() { if command -v curl >/dev/null 2>&1; then curl -s --max-time 3 "$1"; else wget -qO- -T 3 "$1"; fi; }
echo "═══════════════════════════════════════════════════"
for s in $SEL; do
    printf '  %-16s %s\n' "$(svc_of "$s")" "$(systemctl is-active "$(svc_of "$s")" 2>/dev/null || echo '?')"
done
if GET http://127.0.0.1:8778/api/services | grep -q '"services"'; then
    echo "✅ 书架在线：http://<设备IP>:8778/  （$(GET http://127.0.0.1:8778/api/services | grep -o '"name":"[^"]*"' | tr '\n' ' ')）"
else
    echo "⚠️  网关未响应（journalctl -u shelf-gateway）。备份在 $BK。"
    [ "$DO_SYSTEMD" = "0" ] || exit 1
fi
echo "═══════════════════════════════════════════════════"
