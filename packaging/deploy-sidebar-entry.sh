#!/bin/sh
# host 侧一键构建+推送 Sidebar 一级直达入口（KOReader，装了第三方 WeRead app 时自动带上它）。
#
# 2026-09-13 从「手动 SSH 上机部署」捞回 packaging/ 变成可重复脚本：源 QML 补丁本来在
# 2026-09-11 大整理时搬出 git 仓库的 reading-qol/ 里，2026-09-13
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
# 探测不到 ~/xovi/exthome/qt-resource-rebuilder/ 就跳过，不算失败）+ 已 vellum add appload
# （本 qmd 的 onClicked 靠 appload 暴露的 CJAppLoad.AppLoadLauncher 单例发起启动，appload 没装
# 这个调用打不到目标，缺失时探测不到 ~/xovi/exthome/appload/ 也跳过，不算失败）。KOReader 本身
# 是否已经通过 appload 侧载不在本脚本探测范围——沿用 2026-09-02 首版设计（未装 KOReader 时这个
# 按钮点了也没反应，属已知取舍，见 sidebar-entry-koreader-only.qmd 头注）。
#
# 【appload 在 3.28 上要打过 PR #59 兼容补丁】appload v0.5.3 自带的内嵌 qmd 钩的是 3.27 的旧
# Sidebar/MainView 锚点，3.28 已经改了名字——没打这个补丁时 qmldiff 会报 "Couldn't resolve
# the hashed identifier"，appload 自己往 MainView 注入的常驻 Loader（CJAppLoad.AppLoadLauncher
# 单例就活在这个 Loader 里）根本建不起来，这样即使本脚本把 Sidebar 按钮插上去了，点了也没反应
# ——不是本脚本的 bug，是 appload 那份 .so 本身在这个固件版本上不兼容。检测靠读当前这次开机
# 的 journalctl：appload 自己的 qmd 处理成功会打一行 "Loaded external AppLoad hooks in main
# UI"；没这行说明大概率没打过这个补丁（或者压根还没重启过 xochitl 应用刚装好的 appload），本
# 脚本探测不到就跳过、不硬装一个不会响应的按钮。真要修：`packaging/appload_patch_328.py`
# 2026-09-16 已回收进版本控制（见同目录
# appload-qmd-PROVENANCE.md），独立跑 `packaging/deploy-appload-patch.sh <host>`——**没有
# 接入 install-all.sh 的自动编排**，字节替换逻辑还没有对真实 appload.so 做过真机验证，见该
# 脚本头注。PR #59 本身已在 2026-09-07 合并进上游 master，但上游至今没有发布带这个修复的新
# tag，`vellum add appload` 装的官方发行版（仍是 v0.5.3）因此依然没有这个修复。
#
# 用法：./deploy-sidebar-entry.sh [host]      host 默认 10.11.99.1
#   环境 DEFER_XOVI_START=1：只把 qmd/rcc 落盘，不在这一步重启 xochitl——install-all.sh 编排
#   多个 xovi 扩展时用这个避免短时间内反复重启 xochitl（撞 watchdog+StartLimit 的风险，
#   2026-09-11 真机踩过），改成全部落盘完最后统一重启一次（deploy-xovi-apply.sh）。单独跑本脚本
#   不用管这个变量：装完立即重启 xochitl 生效 + 健康检查——怎么重启由设备端 devlib.sh 的
#   cj_xochitl_apply 判定（xovi 已生效 → systemctl restart；没生效才 xovi/start，2026-09-20 修，
#   见 deploy-xovi-apply.sh 头注），重启前会提示"打断阅读"并留 5 秒宽限。
#
# 2026-09-20 改动（脚本审计）：qmd/rcc 先推到暂存目录并 md5 校验，通过后设备端才原子 rename 进 qrr 目录
# （旧版直接 scp 覆盖，md5 不符时坏文件已在 qrr 里）；旧文件备份进 cangjie-backups（保留最近几份），
# 不再在 qrr 目录里放 .bak.pre-*。
set -eu
cd "$(dirname "$0")"
# shellcheck disable=SC1091
. ./lib.sh
# shellcheck disable=SC2034  # HOST 由 lib.sh 的 rssh/rscp/dev_script 使用
HOST="${1:-10.11.99.1}"
QRR_DIR=/home/root/xovi/exthome/qt-resource-rebuilder
STAGE="$CJ_STAGE_REMOTE"
RCC_LOCAL="$(mktemp -t sidebar-icons.XXXXXX.rcc)"
trap 'rm -f "$RCC_LOCAL"' EXIT

echo "== 探测设备端 qt-resource-rebuilder =="
if ! rssh "[ -d $QRR_DIR ]"; then
    echo "-- 设备没装 qt-resource-rebuilder（vellum add qt-resource-rebuilder）——跳过，非失败"
    exit 0
fi

echo "== 探测设备端 appload =="
if ! rssh "[ -d /home/root/xovi/exthome/appload ]"; then
    echo "-- 设备没装 appload（vellum add appload）——跳过，非失败"
    exit 0
fi

echo "== 探测 appload 自己的 qmd 在这台固件上是否兼容 =="
# 正面信号："Loaded external AppLoad hooks in main UI" 是 appload 自己那份内嵌 qmd 成功处理后
# 打的日志——没这行不代表一定没打过 PR #59 补丁（也可能是装完 appload 后还没重启过 xochitl），
# 但按钮多半点了没反应，所以一律当作"暂不满足"处理，不硬装。
# ⚠ 这条只是"本次开机内某个时刻出现过"的一次性判据，不代表现在正在跑的 xochitl 就是那次成功
# 挂载的同一个实例——如果这行日志之后设备上跑过 `vellum upgrade`（会用未打补丁的官方版本盖掉
# appload，见 工程纪律 记录）却还没重启过 xochitl，这里还是会读到旧的成功信号（2026-09-15
# 全量代码审查审出）。真正当次生效与否，靠下面本脚本自己触发的这次重启之后重新核对同一行信号
# （`DEFER_XOVI_START=1` 模式不在这一步重启，没法当场复核，见该分支注释）。
if ! rssh "journalctl -b 0 -u xochitl --no-pager 2>/dev/null | grep -q 'Loaded external AppLoad hooks in main UI'"; then
    echo "-- 没在这次开机日志里看到 appload 成功挂载的信号（可能是 appload 在这个固件版本上没打"
    echo "   过 PR #59 兼容补丁，也可能是刚装完 appload 还没重启过 xochitl）——跳过，非失败。"
    echo "   见本脚本头注「appload 在 3.28 上要打过 PR #59 兼容补丁」一节。"
    exit 0
fi

echo "== 探测设备是否已装第三方 WeRead app =="
if rssh "[ -x /home/root/.local/opt/remarkable-weread/bin/start-remarkable-weread.sh ]"; then
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

echo "== 推送到设备暂存目录（md5 校验；通过前不碰 qrr 目录）=="
push_verified "$QMD_SRC" "$STAGE/koreader-sidebar-entry.qmd"
push_verified "$RCC_LOCAL" "$STAGE/cangjie-icons.rcc"

echo "== 设备端落位（备份进 cangjie-backups + 原子 rename）=="
dev_script "$QRR_DIR" "$STAGE" <<'DEVICE_SCRIPT'
set -eu
QRR="$1"; STG="$2"
[ -f "$STG/koreader-sidebar-entry.qmd" ] && [ -f "$STG/cangjie-icons.rcc" ] || { echo "!! 暂存文件缺失"; exit 1; }
for f in koreader-sidebar-entry.qmd cangjie-icons.rcc; do
    if [ -f "$QRR/$f" ] && ! cmp -s "$STG/$f" "$QRR/$f"; then cj_backup_file "$QRR/$f"; fi   # 内容没变就不堆重复备份
done
cj_safe_replace "$STG/koreader-sidebar-entry.qmd" "$QRR/koreader-sidebar-entry.qmd" "$STG" 644
cj_safe_replace "$STG/cangjie-icons.rcc" "$QRR/cangjie-icons.rcc" "$STG" 644
rm -f "$STG/koreader-sidebar-entry.qmd" "$STG/cangjie-icons.rcc"
rmdir "$STG" 2>/dev/null || true
echo "-- 已落位 $QRR/{koreader-sidebar-entry.qmd,cangjie-icons.rcc}"
DEVICE_SCRIPT

if [ "${DEFER_XOVI_START:-0}" = "1" ]; then
    echo "-- DEFER_XOVI_START=1：只落盘，不在这一步重启 xochitl（由后续统一步骤处理）"
    echo "   ⚠ appload 兼容信号只在上面探测的那一刻核对过，这一步不重启就没法当场复核；"
    echo "     后续统一步骤（deploy-xovi-apply.sh）真正重启后如果按钮点了没反应，先查"
    echo "     一遍 appload 是不是重启前又被 vellum upgrade 覆盖过。"
    exit 0
fi

echo "== 设备端重启 xochitl 让新 qmd/rcc 生效 + 健康检查（会打断设备上的阅读/书写）=="
# 重启前打个时间戳，重启后拿它重新核对 appload 兼容信号——只信"本次重启之后新出现的"这一条，
# 不再相信上面探测阶段那次可能已经过期的"本次开机内某个时刻出现过"（见上面探测那步的头注）。
SINCE="$(rssh "date '+%Y-%m-%d %H:%M:%S'")"
dev_script "$SINCE" <<'DEVICE_SCRIPT'
set -eu
SINCE="$1"
cj_require_root || exit 1
OLD_PID="$(cj_xochitl_pid)"
cj_xochitl_apply || exit 1
cj_xochitl_health "$OLD_PID" || { echo "⚠️  健康检查未达预期。查 journalctl -u xochitl"; exit 1; }
if journalctl -u xochitl --since "$SINCE" --no-pager 2>/dev/null | grep -q 'Loaded external AppLoad hooks in main UI'; then
    echo "✅ 部署完成（appload 兼容信号在这次重启之后重新出现，不是复用重启前的旧信号）"
else
    echo "⚠️  部署已落盘、xochitl 重启健康，但这次重启之后没有重新看到 appload 兼容信号——"
    echo "   探测阶段那次可能已经过期（比如中间跑过 vellum upgrade 把 appload 换回未打补丁的"
    echo "   官方版本）。Sidebar 按钮大概率点了没反应，去 journalctl -u xochitl --since \"$SINCE\""
    echo "   核实，必要时重新走一遍「appload 3.28 免SDK补丁法」。"
    exit 1
fi
DEVICE_SCRIPT
