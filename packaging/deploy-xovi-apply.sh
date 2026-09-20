#!/bin/sh
# host 侧对设备统一"让已落盘的 xovi 扩展/qmd 生效"一次（重启 xochitl）+ 健康检查。
#
# 用在所有"只落盘、不自己重启 xochitl"的步骤跑完之后，最后调用一次——hl-snap/handwriting-stroke/
# sidebar-entry 用 DEFER_XOVI_START=1 时只落盘不重启；shelf 的字体菜单/回收站/建夹 qmd 本来就只落盘。
# 没有"只重载一个扩展"的机制，多个步骤各自重启等于短时间内重启 xochitl 多次，会撞 xochitl 的
# watchdog+StartLimit（2026-09-11 install-all 连续装 hl-snap+handwriting-stroke，真机触发过意外整机重启）。
#
# ⚠ 怎么"重启"由设备端 devlib.sh 的 cj_xochitl_apply 判定（2026-09-20 修）：
#   xovi 已在运行的 xochitl 里生效 → systemctl restart xochitl；没生效才 xovi/start。
#   旧版无条件 xovi/start：在已生效的 xochitl 上它会 umount 重挂 drop-in 目录，xochitl SEGV → 整机自动重启
#   （2026-09-20 真机事故；重跑 install-all 必踩）。重启前会打印"打断阅读"提示并留 5 秒宽限。
#
# 用法：./deploy-xovi-apply.sh [host]      host 默认 10.11.99.1
set -eu
cd "$(dirname "$0")"
# shellcheck disable=SC1091
. ./lib.sh
# shellcheck disable=SC2034  # HOST 由 lib.sh 的 rssh/rscp/dev_script 使用
HOST="${1:-10.11.99.1}"

echo "== 设备端让 xovi 扩展 + qmd 生效（重启 xochitl 一次，会打断设备上正在做的事）+ 健康检查 =="
dev_script <<'DEVICE_SCRIPT'
set -eu
cj_require_root || exit 1
OLD_PID="$(cj_xochitl_pid)"
cj_xochitl_apply || exit 1
TAGS=""
[ -f "$CJ_XOVI/extensions.d/hl-snap.so" ] && TAGS="$TAGS hl-snap"
[ -f "$CJ_XOVI/extensions.d/hw-stroke.so" ] && TAGS="$TAGS hw-stroke"
# shellcheck disable=SC2086  # TAGS 有意按词展开
if cj_xochitl_health "$OLD_PID" $TAGS; then
    echo "✅ xochitl 重启完成，xovi 扩展/qmd 已重新注入"
else
    echo "⚠️  健康检查未达预期。查 journalctl -u xochitl"
    exit 1
fi
DEVICE_SCRIPT
