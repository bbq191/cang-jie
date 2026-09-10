#!/bin/sh
# hw-stroke 安装脚本（vellum-xovi 结构，只碰 /home，不碰 /usr）——独立于
# chinese-ime/langhook 之外的最小 xovi 扩展，第一轮真机实验：验证"改变宽笔画
# 几何生成函数的入口宽度，能不能真的影响渲染出来的笔迹粗细"。
#
# 装的是什么：`extensions.d/` 下的一个 xovi 扩展（hw-stroke.so），跟
# hl-snap/qt-resource-rebuilder/appload/cangjie-langhook 并列，由 xovi 自己
# 扫描加载。hook 目标（FUN_00f47530）跟 hl-snap（FUN_00f05ad0）不是同一个
# 函数，两者不冲突，可以同时装。
#
# ⚠️ 这一版默认 hwStrokeWidthFactor=1.0（不改变笔画粗细，纯诊断/零写入），
# 先按这个默认值真机验证 journal 日志正常、数值合理，再手动改配置文件测试
# 实际改变宽度——见 ../README.md「实现」一节的两步验证流程。
#
# 前置（本脚本不装，跟 chinese-ime/langhook 共用同一份基石）：
#   vellum add xovi
#
# 【重启后持久化】/etc tmpfs 重启即清、无开机自动服务 → 重启后需手动恢复：
#   /home/root/xovi/start        （或 vellum reenable）
set -eu

HERE="$(cd "$(dirname "$0")" && pwd)"
PAYLOAD="$(dirname "$HERE")"   # deploy/ 的上一级，hw-stroke.so 编译产物在这
ROOT=/home/root
XOVI="$ROOT/xovi"
EXTDIR="$XOVI/extensions.d"
DATADIR="$ROOT/.local/share/cangjie-ime"   # 复用同一个 reading-qol.json（hwStrokeWidthFactor 键）

echo "== hw-stroke 安装（第一轮真机实验：变宽笔画几何 hook）=="

[ "$(id -u)" = "0" ] || { echo "!! 需要 root 运行"; exit 1; }
[ -f "$XOVI/xovi.so" ] || { echo "!! 没找到 $XOVI/xovi.so —— 先跑：vellum add xovi"; exit 1; }
[ -f "$PAYLOAD/hw-stroke.so" ] || { echo "!! 没找到 $PAYLOAD/hw-stroke.so，先在这边跑 make aarch64"; exit 1; }

echo "-- 拷 hw-stroke.so -> $EXTDIR/"
mkdir -p "$EXTDIR"
cp "$PAYLOAD/hw-stroke.so" "$EXTDIR/hw-stroke.so"
rm -f "$EXTDIR/hw-stroke.so.crashed"   # 清旧崩溃标记（xovi 把 extensions.d 里任意文件当扩展加载，重复注册是致命错）

# reading-qol.json 首次装才建（不覆盖已有设置）；只关心 hwStrokeWidthFactor
# 这一个键，其它键留给别的功能（hlSnapCjk/笔记增强等）各自维护，这里不动。
RQOL="$DATADIR/reading-qol.json"
if [ ! -s "$RQOL" ]; then
    echo "-- 建最小配置（hwStrokeWidthFactor 默认 1.0，不改变笔画粗细）-> $RQOL"
    mkdir -p "$DATADIR"
    printf '%s' '{"hwStrokeWidthFactor":1.0}' > "$RQOL"
fi

echo "-- 应用 xovi/start（/etc tmpfs 引导，不碰 /usr）"
OLD_PID="$(systemctl show xochitl -p MainPID --value 2>/dev/null || echo 0)"
"$XOVI/start"
sleep 5

STATE="$(systemctl is-active xochitl 2>/dev/null || true)"
NEW_PID="$(systemctl show xochitl -p MainPID --value 2>/dev/null || echo 0)"
NREST="$(systemctl show xochitl -p NRestarts --value 2>/dev/null || echo '?')"
HW="$(grep -c hw-stroke /proc/"$NEW_PID"/maps 2>/dev/null || echo 0)"
echo "=================================================="
echo "  is-active : $STATE   (期望 active)"
echo "  MainPID   : $OLD_PID -> $NEW_PID   (期望有变化)"
echo "  NRestarts : $NREST   (期望 0/不增)"
echo "  hw-stroke 加载 : $HW 段   (期望 >0)"
echo "=================================================="
if [ "$STATE" = "active" ] && [ "$NEW_PID" != "0" ] && [ "${HW:-0}" -gt 0 ]; then
    echo "✅ 安装完成。写字/画图后跑：journalctl -u xochitl | grep hw-stroke"
    echo "   确认有日志、w 数值合理，再考虑改 $RQOL 里的 hwStrokeWidthFactor 测试实际效果。"
    echo "⚠️  重启后需手动恢复：/home/root/xovi/start（或 vellum reenable）"
else
    echo "⚠️  健康检查未达预期。查 journalctl -u xochitl | grep hw-stroke"
    exit 1
fi
