#!/bin/sh
# cangjie-ime 卸载脚本（vellum-xovi 结构）—— reMarkable Paper Pro，root 运行。
# 与新 install.sh 配套：只清 /home 下 cangjie 自己的东西 + 语言回退；不动 xovi/qrr（vellum 装的）。
#
# 借鉴 rmtool：若系统语言仍是 zh_*，先把 xochitl.conf 改回 en 再收尾，避免悬空配置。
set -eu

ROOT=/home/root
XOVI="$ROOT/xovi"
EXTDIR="$XOVI/extensions.d"
SVCEXT="$XOVI/services/xochitl.service/extensions.d"
QRR="$XOVI/exthome/qt-resource-rebuilder"
DATADIR="$ROOT/.local/share/cangjie-ime"
XCONF="$ROOT/.config/remarkable/xochitl.conf"
# 旧架构残留（本项目曾写 /usr，dm-verity 回滚雷）——best-effort 清掉
LEGACY_DROPIN=/usr/lib/systemd/system/xochitl.service.d/zz-cangjie-xovi.conf

echo "== cangjie-ime 卸载（vellum-xovi 结构）=="
[ "$(id -u)" = "0" ] || { echo "!! 需要 root 运行"; exit 1; }

# ---- 1. 系统语言回 en（先于收尾）----
if [ -f "$XCONF" ] && grep -q '^language=zh_' "$XCONF"; then
    sed -i 's/^language=zh_.*/language=en/' "$XCONF"
    echo "-- xochitl.conf language 已从 zh_* 改回 en"
fi

# ---- 2. cangjie 扩展（两处 extensions.d）+ 词典 ----
echo "-- 删 cangjie 扩展（顶层 + services）+ 词典"
rm -f "$EXTDIR/cangjie-langhook.so" "$EXTDIR/cangjie-langhook.so.crashed"
rm -f "$SVCEXT/cangjie-langhook.so" "$SVCEXT/cangjie-langhook.so.crashed"
rm -rf "$DATADIR"

# ---- 2b. UI 汉化 bind-mount：卸载挂载 + 删 pre-start 脚本 + overlay ----
TRANS=/usr/share/remarkable/xochitl/translations
echo "-- 卸载 UI 汉化 bind-mount + 删 pre-start 脚本"
# 可能叠挂多层（reenable 反复跑过），最多卸 3 次；没挂了即完成，卸不动用 lazy 兜底再停
i=0
while [ "$i" -lt 3 ] && grep -q " $TRANS " /proc/mounts 2>/dev/null; do
    umount "$TRANS" 2>/dev/null || umount -l "$TRANS" 2>/dev/null || break
    i=$((i + 1))
done
rm -f "$XOVI/scripts/pre-start/cangjie-xlate-bindmount.sh"
rm -f "$XOVI/scripts/pre-start/cangjie-qrr-failsafe.sh"   # qrr 崩溃自愈 fail-safe（隔离区/标记随 rm -rf $DATADIR 清）
rm -rf "$DATADIR/xlate-overlay" "$DATADIR/translations"

# ---- 3. 本项目装的 qmd（保留 qrr 自己的 hashtab）----
echo "-- 删本项目的 qmd"
for q in candidatebar.qmd reading-qol-config.qmd tap-page-turn.qmd fast-mono-reading.qmd \
         page-refresh.qmd keyboard-mono.qmd add-reading-fonts.qmd add-lxgw-font.qmd \
         settings-keyboard-zh.qmd settings-reading-enhance.qmd; do
    rm -f "$QRR/$q"
done

# ---- 4. best-effort 清旧架构 /usr 残留（若存在。注意：写 /usr 本身有 verity 风险，
#         但删掉一个不该在的文件是为了避免它继续存在；只在真存在时才 remount）----
if [ -f "$LEGACY_DROPIN" ]; then
    echo "-- 检测到旧架构 /usr drop-in，尝试清除（remount rw）"
    mount -o remount,rw / 2>/dev/null || true
    rm -f "$LEGACY_DROPIN"
    rmdir /usr/lib/systemd/system/xochitl.service.d 2>/dev/null || true
    sync; mount -o remount,ro / 2>/dev/null || true
fi

# ---- 5. 重放 xovi/start，让 xochitl 不带 cangjie 重启（qrr 等其它扩展保留）----
echo "-- 重放 xovi/start（xochitl 将不再加载 cangjie）"
if [ -x "$XOVI/start" ]; then
    "$XOVI/start"; sleep 4
    STATE="$(systemctl is-active xochitl 2>/dev/null || true)"
    echo "   xochitl is-active=$STATE"
else
    echo "   未找到 xovi/start，请手动 systemctl restart xochitl"
fi

echo "✅ 卸载完成。字体保留（无害）；xovi/qrr 保留（vellum 管理，非本项目）。"
echo "   如需连 xovi 一起卸：vellum del xovi qt-resource-rebuilder"
