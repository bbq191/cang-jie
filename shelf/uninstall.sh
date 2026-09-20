#!/bin/sh
# 书架卸载（设备端，root）：停服务、删单元/.wants、删 ~/.local/bin 二进制、删随服务装的 qmd 与辅助脚本；
# **保留用户数据**（~/.config/shelf、~/.local/share/shelf、~/.local/state/shelf、用户字体/壁纸）。
# 清单与 install.sh 共用 manifest.sh（2026-09-20：install 装什么这里就删什么，含 shelf-mkdir-agent.qmd、
# lo-alias.sh、shelf-uninstall 本身与 ~/.local/lib/shelf 库，以及旧命名遗留 shelf-gateway 等）。
# 用法：./uninstall.sh [--only font,wallpaper] [--purge]   --purge 连数据一起删
#   --only 只卸列出的服务（共享件 shelf-uninstall/库/shelf.target 保留；被网页「管理台」这样调用）
#   --purge 只删 shelf 自己的三个 XDG 目录（且要求目录名恰为 shelf、不是符号链接）；笔记线等其它线的数据不碰。
# 设备上装成 ~/.local/bin/shelf-uninstall（网关网页卸载与 packaging/uninstall-all.sh 都调它）。
set -eu

HERE="$(cd "$(dirname "$0")" && pwd)"
ONLY=""; PURGE=0; _prev=""
for a in "$@"; do
    case "$_prev" in
        --only) ONLY="$a"; _prev=""; continue ;;
    esac
    case "$a" in
        --only=*) ONLY="${a#--only=}" ;;
        --only) _prev="$a" ;;
        --purge) PURGE=1 ;;
        *) echo "!! 未知参数：$a"; exit 2 ;;
    esac
done
[ -z "$_prev" ] || { echo "!! $_prev 缺参数"; exit 2; }

_lib() {
    for d in "$HERE" "${HOME:-/home/root}/.local/lib/shelf"; do
        [ -f "$d/$1" ] && { echo "$d/$1"; return 0; }
    done
    echo "!! 找不到 $1（重装一次 shelf 会装上库；或从载荷目录跑）" >&2; return 1
}
# shellcheck disable=SC1090
. "$(_lib devlib.sh)"
# shellcheck disable=SC1090
. "$(_lib manifest.sh)"

HOME_DIR="$CJ_HOME"
BIN_DIR="$HOME_DIR/.local/bin"
LIB_DIR="$HOME_DIR/.local/lib/$SHELF_LIB_DIRNAME"
SYSD="$CJ_SYSD"
QRR="$HOME_DIR/xovi/exthome/qt-resource-rebuilder"

if [ -n "$ONLY" ]; then
    SEL=""
    for s in $(echo "$ONLY" | tr ',' ' '); do
        case " $SHELF_ALL " in *" $s "*) SEL="$SEL $s" ;; *) echo "!! 未知服务令牌：$s（可选：$SHELF_ALL）"; exit 2 ;; esac
    done
else
    SEL="$SHELF_ALL"
fi
sel_has() { case " $SEL " in *" $1 "*) return 0 ;; *) return 1 ;; esac; }
cj_require_root || exit 1

for s in $SEL; do systemctl disable --now "$(shelf_svc_of "$s").service" 2>/dev/null || true; done
if [ -z "$ONLY" ]; then
    systemctl disable --now shelf.target 2>/dev/null || true
    for lu in $SHELF_LEGACY_UNITS; do systemctl disable --now "$lu" 2>/dev/null || true; done
fi

# ── /usr 单元（dm-verity 门 + 带 trap 的 rw 窗口）──
rm_units() {
    for s in $SEL; do
        u="$(shelf_svc_of "$s").service"
        rm -f "$SYSD/$u" "$SYSD/shelf.target.wants/$u"
    done
    if [ -z "$ONLY" ]; then
        rm -f "$SYSD/shelf.target" "$SYSD/multi-user.target.wants/shelf.target"
        for lu in $SHELF_LEGACY_UNITS; do rm -f "$SYSD/$lu" "$SYSD/shelf.target.wants/$lu"; done
        rmdir "$SYSD/shelf.target.wants" 2>/dev/null || true
    fi
    return 0
}
if cj_verity_active; then
    echo "✋ dm-verity 激活，rootfs 不可写——服务已 disable，/usr 里的单元文件删不掉（下次 OTA 冲掉后随之消失）。"
else
    cj_with_rootfs_rw rm_units || echo "⚠ 删 /usr 单元失败（rootfs 已恢复 ro）；服务已 disable，不会自动运行"
    systemctl daemon-reload
fi

# ── 随服务装的东西：壁纸还原 / qmd / 辅助脚本 / 二进制 ──
if sel_has wallpaper; then
    # 还原原生休眠屏（删 xochitl.conf SleepScreenPath；xochitl 重启后生效）
    [ -x "$BIN_DIR/wallpaper-serve" ] && "$BIN_DIR/wallpaper-serve" disable 2>/dev/null || true
fi
for s in $SEL; do
    for q in $(shelf_svc_qmds "$s"); do rm -f "$QRR/$q"; done
    for h in $(shelf_svc_helpers "$s"); do rm -f "$BIN_DIR/$h"; done
    rm -f "$BIN_DIR/$(shelf_svc_of "$s")"
done

# ── 整包卸载才做：旧命名遗留、共享件（库、shelf-uninstall 本身，最后删）──
if [ -z "$ONLY" ]; then
    for lb in $SHELF_LEGACY_BINS; do rm -f "$BIN_DIR/$lb"; done
    for lq in $SHELF_LEGACY_QMDS; do rm -f "$QRR/$lq"; done
    for f in $SHELF_LIB_FILES; do
        # 库文件此刻已被 source 进内存，删了不影响后面的执行
        rm -f "$LIB_DIR/$f"
    done
    rmdir "$LIB_DIR" 2>/dev/null || true
    rmdir "$HOME_DIR/.local/lib" 2>/dev/null || true
    rm -f "$BIN_DIR/$SHELF_UNINSTALL_BIN"   # 自己：Linux 上删掉正在跑的脚本文件没问题，放在最后
fi

if [ "$PURGE" = "1" ]; then
    # 只删 shelf 自己的三个 XDG 目录：目录名必须恰为 shelf、不能是符号链接、变量不能为空
    XCH="${XDG_CONFIG_HOME:-$HOME_DIR/.config}"; XDH="${XDG_DATA_HOME:-$HOME_DIR/.local/share}"; XSH="${XDG_STATE_HOME:-$HOME_DIR/.local/state}"
    for d in $(shelf_data_dirs "$XCH" "$XDH" "$XSH"); do
        case "$d" in
            /?*/shelf) ;;
            *) echo "!! 拒绝清除异常路径：$d"; continue ;;
        esac
        [ -L "$d" ] && { echo "!! $d 是符号链接，拒绝清除"; continue; }
        [ -d "$d" ] && rm -rf "$d"
    done
    echo "-- 已连同数据清除（~/.config/shelf ~/.local/share/shelf ~/.local/state/shelf；笔记线数据未动）"
fi
echo "✅ 书架已卸载（$(echo "$SEL" | tr ' ' ',' | sed 's/^,//')）；用户字体/壁纸文件未动。"
[ -z "$ONLY" ] || echo "   （--only：shelf.target、shelf-uninstall 与共享库保留）"
