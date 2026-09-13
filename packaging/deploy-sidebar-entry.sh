#!/bin/sh
# host 侧一键构建+推送 Sidebar 一级直达入口（KOReader，装了第三方 WeRead app 时自动带上它）。
#
# 2026-09-13 从「手动 SSH 上机部署」捞回 packaging/ 变成可重复脚本：源 QML 补丁本来在
# oldbak/xovi-extensions/reading-qol/（2026-09-11 大整理搬出 git 仓库时带走的），2026-09-13
# 给这台设备装第三方 WeRead app 时又手动改过一次、手动重新部署过——这次把最终版本捞回
# packaging/，往后走这个脚本，不用再记住那一串手动步骤。
#
# 两份 qmd 二选一（同一个仓库都带，脚本按设备实际装了什么选）：
#   sidebar-entry-koreader-only.qmd      只有 KOReader 一项（WeRead 没装时用这份，避免装出一个
#                                         点了没反应的按钮）
#   sidebar-entry-koreader-weread.qmd    KOReader + WeRead 两项（探测到设备装了 WeRead 时用这份）
# 图标资源 sidebar-icons.qrc + 两张 png 本地用 rcc 编译成 cangjie-icons.rcc 再推上去（跟
# qt-resource-rebuilder 已经在用的资源文件同名，直接覆盖）。
#
# 前置：设备已 vellum add qt-resource-rebuilder（qmd/rcc 靠它的 .rcc 通道加载，缺失时本脚本
# 探测不到 ~/xovi/exthome/qt-resource-rebuilder/ 就跳过，不算失败）。KOReader 本身是否已经
# 通过 appload 侧载不在本脚本探测范围——沿用 2026-09-02 首版设计（未装 KOReader 时这个按钮
# 点了也没反应，属已知取舍，见 sidebar-entry-koreader-only.qmd 头注）。
#
# 用法：./deploy-sidebar-entry.sh [host]      host 默认 10.11.99.1
#   环境 DEFER_XOVI_START=1：只把 qmd/rcc 落盘，不在这一步跑 xovi/start——install-all.sh 编排
#   多个 xovi 扩展时用这个避免短时间内反复重启 xochitl（撞 watchdog+StartLimit 的风险，
#   2026-09-11 真机踩过），改成全部落盘完最后统一跑一次（deploy-xovi-apply.sh）。单独跑本脚本
#   不用管这个变量，默认行为不变（装完立即 xovi/start 生效 + 健康检查）。
set -eu
cd "$(dirname "$0")"
HOST="${1:-10.11.99.1}"
QRR_DIR=/home/root/xovi/exthome/qt-resource-rebuilder
RCC_LOCAL="$(mktemp -t sidebar-icons.XXXXXX.rcc)"
trap 'rm -f "$RCC_LOCAL"' EXIT

echo "== 探测设备端 qt-resource-rebuilder =="
if ! ssh "root@$HOST" "[ -d $QRR_DIR ]"; then
    echo "-- 设备没装 qt-resource-rebuilder（vellum add qt-resource-rebuilder）——跳过，非失败"
    exit 0
fi

echo "== 探测设备是否已装第三方 WeRead app =="
if ssh "root@$HOST" "[ -x /home/root/.local/opt/remarkable-weread/bin/start-remarkable-weread.sh ]"; then
    QMD_SRC=sidebar-entry-koreader-weread.qmd
    echo "-- 装了 WeRead，用 $QMD_SRC（KOReader + WeRead 两项）"
else
    QMD_SRC=sidebar-entry-koreader-only.qmd
    echo "-- 没装 WeRead，用 $QMD_SRC（只有 KOReader 一项）"
fi

echo "== 本地编译图标资源 =="
if ! command -v rcc >/dev/null 2>&1; then
    echo "!! 本机没有 rcc（Qt Resource Compiler，随 Qt 开发包/qt6-base-devel 一类包提供）"
    exit 1
fi
rcc --binary -o "$RCC_LOCAL" sidebar-icons.qrc

echo "== 备份设备上现有的 qmd/rcc（若存在）=="
# shellcheck disable=SC2029  # 远端路径固定字面量，无用户输入拼接风险
ssh "root@$HOST" "
    [ -f $QRR_DIR/koreader-sidebar-entry.qmd ] && cp $QRR_DIR/koreader-sidebar-entry.qmd $QRR_DIR/koreader-sidebar-entry.qmd.bak.pre-sidebar-entry-deploy
    [ -f $QRR_DIR/cangjie-icons.rcc ] && cp $QRR_DIR/cangjie-icons.rcc $QRR_DIR/cangjie-icons.rcc.bak.pre-sidebar-entry-deploy
    true
"

echo "== 推送到 root@$HOST =="
scp "$QMD_SRC" "root@$HOST:$QRR_DIR/koreader-sidebar-entry.qmd"
scp "$RCC_LOCAL" "root@$HOST:$QRR_DIR/cangjie-icons.rcc"

echo "== md5 校验 =="
LOCAL_QMD_MD5="$(md5sum "$QMD_SRC" | awk '{print $1}')"
LOCAL_RCC_MD5="$(md5sum "$RCC_LOCAL" | awk '{print $1}')"
REMOTE_MD5S="$(ssh "root@$HOST" "md5sum $QRR_DIR/koreader-sidebar-entry.qmd $QRR_DIR/cangjie-icons.rcc" | awk '{print $1}')"
REMOTE_QMD_MD5="$(echo "$REMOTE_MD5S" | sed -n 1p)"
REMOTE_RCC_MD5="$(echo "$REMOTE_MD5S" | sed -n 2p)"
if [ "$LOCAL_QMD_MD5" != "$REMOTE_QMD_MD5" ] || [ "$LOCAL_RCC_MD5" != "$REMOTE_RCC_MD5" ]; then
    echo "!! md5 对不上（qmd: $LOCAL_QMD_MD5 vs $REMOTE_QMD_MD5；rcc: $LOCAL_RCC_MD5 vs $REMOTE_RCC_MD5）"
    exit 1
fi
echo "-- md5 一致"

if [ "${DEFER_XOVI_START:-0}" = "1" ]; then
    echo "-- DEFER_XOVI_START=1：只落盘，不在这一步跑 xovi/start（由后续统一步骤处理）"
    exit 0
fi

echo "== 设备端跑一次 xovi/start 让新 qmd/rcc 生效 + 健康检查 =="
# shellcheck disable=SC2087  # heredoc 内变量就是要在本地展开，全部是固定字面量，无远端注入风险
ssh "root@$HOST" "sh -s" <<'DEVICE_SCRIPT'
set -eu
OLD_PID="$(systemctl show xochitl -p MainPID --value 2>/dev/null || echo 0)"
/home/root/xovi/start
sleep 5
STATE="$(systemctl is-active xochitl 2>/dev/null || true)"
NEW_PID="$(systemctl show xochitl -p MainPID --value 2>/dev/null || echo 0)"
NREST="$(systemctl show xochitl -p NRestarts --value 2>/dev/null || echo '?')"
echo "  is-active : $STATE   (期望 active)"
echo "  MainPID   : $OLD_PID -> $NEW_PID   (期望有变化)"
echo "  NRestarts : $NREST   (期望 0/不增)"
if [ "$STATE" = "active" ] && [ "$NEW_PID" != "0" ]; then
    echo "✅ 部署完成"
else
    echo "⚠️  健康检查未达预期。查 journalctl -u xochitl"
    exit 1
fi
DEVICE_SCRIPT
