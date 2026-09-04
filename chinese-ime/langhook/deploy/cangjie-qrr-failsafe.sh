#!/bin/sh
# cangjie-qrr-failsafe.sh —— qrr 阅读增强 qmd 崩溃自愈（xovi pre-start 钩子）
#
# 装在 $XOVI/scripts/pre-start/：每次 xovi/start（开机由 xovi-reenable.service 触发、
# 或手动 xovi/start）在**重启 xochitl 之前**跑一次。
#
# 【机制】设备 journald 持久（/var/log/journal）→ 检查【上一个 boot】xochitl 有没有崩溃循环：
#   数 code=dumped / Failed with result / "start request repeated too quickly" / watchdog 等崩溃签名。
#   干净的 systemctl restart（部署/手动）不产生这些签名、不计数——只数真崩。
#   ≥THRESH 次 → 判为崩溃循环 → 把我们的 6 个阅读增强 qmd 移出 qrr exthome 隔离，
#   本次 xochitl 就不再加载它们、恢复到"输入法可用"的健康态。**只动我们自己的 qmd，
#   绝不碰 candidatebar / moxiang-sidebar / settings-keyboard-zh / qrr 本体。**
#
# 【为什么这样够】xochitl.service StartLimitBurst=4/10min；崩溃循环触发 StartLimitAction→设备重启，
#   下一个 boot 本脚本用 `journalctl -b -1` 读到上个 boot 的崩溃签名即隔离——**一个重启周期内自愈，
#   不是永久砖**。这是 .so 的 `_xovi_shouldLoad`/`.crashed` load-time fail-safe 在 qmd 粒度的对应物。
#
# 【局限（诚实）】① 只针对崩溃循环（≥3 次），单次崩溃不隔离（单崩 xochitl 重启即恢复、非砖）；
#   ② 无法归因到具体哪个 qmd——崩溃循环时把 6 个一起隔离（保守：先把我们自己摘干净排除嫌疑）；
#   若崩溃其实与我们无关，会连带停掉阅读增强（无害、留 TRIGGERED 说明、可移回）；
#   ③ 只在 xovi/start 跑（开机/手动），systemd 自动重启不跑本脚本——靠"下个 boot 自愈"兜底。
#
# 【恢复】修好后重跑 deploy/install.sh（会清 TRIGGERED + 重铺 qmd），或手动把
#   $STATE/qrr-failsafe.quarantine/*.qmd 移回 qrr exthome。
set -u

STATE=/home/root/.local/share/cangjie-ime
QRR=/home/root/xovi/exthome/qt-resource-rebuilder
QUAR=$STATE/qrr-failsafe.quarantine
TRIG=$STATE/qrr-failsafe.TRIGGERED
THRESH=${CJ_QRR_FAILSAFE_THRESH:-3}   # 可用环境变量覆盖（测试用）
LIST="reading-qol-config.qmd tap-page-turn.qmd fast-mono-reading.qmd page-refresh.qmd add-reading-fonts.qmd font-menu-dynamic.qmd settings-reading-enhance.qmd"

command -v journalctl >/dev/null 2>&1 || exit 0
mkdir -p "$STATE"

# 上一个 boot 的 xochitl 崩溃签名计数（只数真崩、不数干净重启）
crashes=$(journalctl -b -1 -u xochitl 2>/dev/null \
    | grep -c -iE "code=dumped|code=killed|Failed with result|start request repeated too quickly|watchdog timeout" \
    2>/dev/null || echo 0)
case "$crashes" in ''|*[!0-9]*) crashes=0 ;; esac

[ "$crashes" -ge "$THRESH" ] || exit 0

# 仅当我们的 qmd 还在（没被隔离过）才动手——避免反复搬、也避免对已隔离态再触发
present=0
for q in $LIST; do
    [ -f "$QRR/$q" ] && present=1
done
[ "$present" = "1" ] || exit 0

mkdir -p "$QUAR"
moved=""
for q in $LIST; do
    [ -f "$QRR/$q" ] && mv "$QRR/$q" "$QUAR/$q" 2>/dev/null && moved="$moved $q"
done
{
    echo "$(date): 上一个 boot 检测到 xochitl 崩溃签名 ${crashes} 次 (>=${THRESH})"
    echo "已隔离阅读增强 qmd →${moved}"
    echo "恢复：修好后重跑 deploy/install.sh，或把 ${QUAR}/*.qmd 移回 ${QRR}/"
} > "$TRIG"
echo "[qrr-failsafe] 崩溃循环自愈：隔离阅读增强 qmd →${moved}" >&2
exit 0
