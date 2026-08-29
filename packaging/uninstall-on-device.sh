#!/bin/sh
# ═══════════════════════════════════════════════════════════════════════════
# cang-jie 全项目卸载器（设备端） —— 干净回滚，root 运行。
# 逆序拆三层：停服务 → 摘 /usr 开机持久（dm-verity 门）→ 删 reading/pkm 二进制
#   → 交给真机验证过的 ime/uninstall.sh 收 xovi/qmd/字体/汉化 + 语言回退。
# 幂等；不动 vellum 装的 xovi/qt-resource-rebuilder 本体。
# ═══════════════════════════════════════════════════════════════════════════
set -u

HERE="$(cd "$(dirname "$0")" && pwd)"
ROOT=/home/root
DEST="$ROOT/weread"
SYSD=/usr/lib/systemd/system
UNITS="cangjie-xovi-reenable.service wr-serve.service wr-renew.service wr-renew.timer cj-stars.service"
BINS="wr-serve wr-download wr-renew wr-fetch cj-stars-daemon cj-stars"

echo "== cang-jie 全项目卸载 =="
[ "$(id -u)" = "0" ] || { echo "!! 需要 root 运行"; exit 1; }

# 1. 停 + 禁用常驻服务（timer 也停）
for u in wr-serve.service cj-stars.service wr-renew.timer wr-renew.service cangjie-xovi-reenable.service; do
    systemctl stop "$u" 2>/dev/null || true
done

# 2. 摘 /usr 开机持久（dm-verity 激活则不碰 /usr）
if dmsetup ls --target verity 2>/dev/null | grep -q .; then
    echo "-- dm-verity 激活，跳过写 /usr（单元若在 rootfs 需换非 verity 时再清）"
else
    mount -o remount,rw / 2>/dev/null || true
    for u in $UNITS; do
        rm -f "$SYSD/$u" "$SYSD/multi-user.target.wants/$u" "$SYSD/timers.target.wants/$u"
    done
    sync
    mount -o remount,ro / 2>/dev/null || true
    systemctl daemon-reload 2>/dev/null || true
    echo "-- 已摘除 systemd 开机持久（/usr）"
fi

# 3. 删 reading/pkm 二进制（保留 credentials.json / 已生成卡片等用户数据）
for b in $BINS; do rm -f "$DEST/$b"; done
echo "-- 已删 reading/pkm 二进制（$DEST 下用户数据保留）"

# 3b. 摘 OTA 登录触发恢复钩子（去 ~/.bashrc 块 + 尝试标记）
BRC="$ROOT/.bashrc"
[ -f "$BRC" ] && sed -i '/# >>> cangjie-ota-recover >>>/,/# <<< cangjie-ota-recover <<</d' "$BRC" 2>/dev/null || true
rm -f "$ROOT/.local/share/cangjie-ime/ota-recover.attempted."* 2>/dev/null || true
echo "-- 已摘 OTA 恢复钩子（~/.bashrc）"

# 4. 交给 IME 卸载器收尾（xovi 扩展/qmd/字体/汉化 + 语言回退）
if [ -f "$HERE/ime/uninstall.sh" ]; then
    echo "-- 调 ime/uninstall.sh 收尾中文化层"
    sh "$HERE/ime/uninstall.sh" || echo "⚠ ime/uninstall.sh 非零退出，手动核查"
else
    echo "⚠ 包内无 ime/uninstall.sh，中文化层未清（手动跑 chinese-ime/langhook/deploy/uninstall.sh）"
fi

echo "✅ 卸载完成。重启后 xochitl 裸启原生（本项目全部移除）。"
