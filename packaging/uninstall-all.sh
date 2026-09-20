#!/bin/sh
# ═══════════════════════════════════════════════════════════════════════════
# cang-jie 全新设备统一卸载器（host 侧编排，2026-09-16 新写）—— install-all.sh 的对称卸载。
# 2026-09-20：步骤表与 install-all 共用 lib.sh（STEP_ORDER / STEP_CONFIG_ONLY），设备端动作走 devlib.sh；
# 每个非"配置覆写/纯动作"的安装步骤在这里都必须有 uninstall_<步骤名> 函数——tests/run_sim_tests.sh 会核对。
#
# 只编排、不重新实现——依次在设备端停用/删除 install-all.sh 各步骤留下的东西：
#   chrony-boot-wakelock / xovi-persist / wifi-watch   停用 + 删 /usr 单元（dm-verity 门，跟安装时同一套 devlib 写法）；
#                                                       wifi-watch 另删 ~/.local/bin/wifi-watch.sh
#   hl-snap / handwriting-stroke   从 extensions.d 摘除 .so（不碰 reading-qol.json 配置、不碰 cangjie-backups/）
#   sidebar-entry                  从 qt-resource-rebuilder exthome 摘除 qmd/rcc
#   battop                         停用 + 删 /usr 单元（--purge 才连 /home/root/battop 数据删）
#   shelf                          调设备上的 shelf-uninstall（~/.local/bin，优先）或 shelf-pkg 里的 uninstall.sh，
#                                  默认保留用户数据，清单与 install 共用 manifest.sh
#
# 明确不做的事（范围外，跟 install-all.sh「明确不做的事」对称）：
#   · chrony-cn.sh / timezone-cn.sh 不卸——它们是配置覆写（改 /etc/chrony.conf、/etc/localtime 指向），
#     不是"装了一个独立的东西"，没有卸载语义；xovi-apply 是纯动作。
#   · 不卸 vellum/xovi/qt-resource-rebuilder/appload 本体、不卸载 KOReader 侧载——这些从来不是本项目装的。
#   · 不碰中文化（不在本仓库）。
#
# ⚠️ 摘掉 extensions.d/qt-resource-rebuilder 里的文件后，当前正在跑的 xochitl 进程内存里还留着旧的映射——
# 真正"生效"要等下一次 xochitl 重启。本脚本不主动重启 xochitl（卸载没有"装完立刻验证"的必要，强制重启
# 只会多一次触发 watchdog/StartLimit 的机会）；要重启用 systemctl restart xochitl（xovi 已生效时**不要** xovi/start）。
#
# 用法：./uninstall-all.sh [host] [--purge] [--skip a,b,...]
#   host    默认 10.11.99.1（USB）
#   --purge 额外清用户数据——目前只影响 battop（连 /home/root/battop 的二进制+历史采样数据一起删）；
#           shelf 不受影响，那是几条独立业务线的用户数据，误删风险太高，要删请 --skip shelf 后自己跑
#           设备上的 shelf-uninstall --purge。
#   --skip  逗号分隔，跳过指定的卸载步骤（名字同 install-all）
# ═══════════════════════════════════════════════════════════════════════════
set -eu
cd "$(dirname "$0")"
# shellcheck disable=SC1091
. ./lib.sh

parse_step_args "$@"
[ "$FORCE" = "0" ] || { echo "!! 未知参数：--force（那是 install-all.sh 的）"; exit 2; }

uninstall_chrony_boot_wakelock() { dev_script chrony-boot-wakelock.service <<'DEVICE_SCRIPT'
set -eu
cj_require_root || exit 1
rc=0; cj_remove_usr_unit "$1" multi-user.target.wants || rc=$?
[ "$rc" = 0 ] || [ "$rc" = 3 ]
DEVICE_SCRIPT
}

uninstall_xovi_persist() { dev_script xovi-reenable.service <<'DEVICE_SCRIPT'
set -eu
cj_require_root || exit 1
rc=0; cj_remove_usr_unit "$1" multi-user.target.wants || rc=$?
[ "$rc" = 0 ] || [ "$rc" = 3 ]
DEVICE_SCRIPT
}

uninstall_wifi_watch() { dev_script <<'DEVICE_SCRIPT'
set -eu
cj_require_root || exit 1
rc=0; cj_remove_usr_unit wifi-watch.service multi-user.target.wants || rc=$?
[ "$rc" = 0 ] || [ "$rc" = 3 ] || exit 1
rm -f "$CJ_HOME/.local/bin/wifi-watch.sh"
echo "-- 已删 ~/.local/bin/wifi-watch.sh（cangjie-backups/ 下的备份不动）"
DEVICE_SCRIPT
}

# 从 extensions.d 摘除一个 xovi 扩展本体（+ 清同名的 .crashed 崩溃标记）。不碰 reading-qol.json（多个扩展
# 共用同一份配置，卸一个不该动别人的开关）、不碰 cangjie-backups/（那是回滚安全网）。$1=文件名（不含路径）
remove_xovi_extension() {
    dev_script "$1" <<'DEVICE_SCRIPT'
set -eu
SO="$1"
EXT="$CJ_XOVI/extensions.d"
if [ ! -f "$EXT/$SO" ]; then
    echo "-- $EXT/$SO 本来就不存在，跳过"
    exit 0
fi
rm -f "$EXT/$SO" "$EXT/$SO.crashed"
echo "-- 已从 extensions.d 摘除 $SO（reading-qol.json 配置、cangjie-backups/ 下的历史备份不动）"
DEVICE_SCRIPT
}
uninstall_hl_snap() { remove_xovi_extension hl-snap.so; }
uninstall_handwriting_stroke() { remove_xovi_extension hw-stroke.so; }

uninstall_sidebar_entry() {
    dev_script <<'DEVICE_SCRIPT'
set -eu
QRR_DIR="$CJ_XOVI/exthome/qt-resource-rebuilder"
if [ ! -d "$QRR_DIR" ]; then
    echo "-- 设备没装 qt-resource-rebuilder，本来就没有这两个文件，跳过"
    exit 0
fi
rm -f "$QRR_DIR/koreader-sidebar-entry.qmd" "$QRR_DIR/cangjie-icons.rcc"
echo "-- 已从 qt-resource-rebuilder exthome 摘除 Sidebar 入口 qmd/rcc（备份在 cangjie-backups/，不动）"
DEVICE_SCRIPT
}

uninstall_battop() {
    dev_script "$PURGE" <<'DEVICE_SCRIPT'
set -eu
PURGE="$1"
cj_require_root || exit 1
rc=0; cj_remove_usr_unit battop.service multi-user.target.wants || rc=$?   # battop 有意不建开机链接（见 enhance/battop/install.sh），这里顺手清可能的旧链接
[ "$rc" = 0 ] || [ "$rc" = 3 ] || exit 1
if [ "$PURGE" = "1" ]; then
    BD="$CJ_HOME/battop"
    case "$BD" in /?*/battop) ;; *) echo "!! 拒绝清除异常路径 $BD"; exit 1 ;; esac
    [ -L "$BD" ] && { echo "!! $BD 是符号链接，拒绝清除"; exit 1; }
    rm -rf "$BD"
    echo "-- --purge：已删 $BD（二进制 + 历史采样数据）"
else
    echo "-- 保留 $CJ_HOME/battop（二进制 + 历史采样数据）；要连数据一起删加 --purge"
fi
DEVICE_SCRIPT
}

uninstall_shelf() {
    # 优先用已装的 ~/.local/bin/shelf-uninstall（install.sh 每次更新它，是单一事实源）；没有再退回 shelf-pkg 里的副本
    # （旧设备上 shelf-pkg 可能是很久以前的载荷）
    dev_script <<'DEVICE_SCRIPT'
set -eu
if [ -f "$CJ_HOME/.local/bin/shelf-uninstall" ]; then
    exec sh "$CJ_HOME/.local/bin/shelf-uninstall"
elif [ -f "$CJ_HOME/shelf-pkg/shelf/uninstall.sh" ]; then
    echo "-- 没有 ~/.local/bin/shelf-uninstall，退回 shelf-pkg 里的 uninstall.sh"
    exec sh "$CJ_HOME/shelf-pkg/shelf/uninstall.sh"
else
    echo "-- 设备上没找到 shelf-uninstall / shelf-pkg（shelf 从没部署过，或被手动清过），跳过"
fi
DEVICE_SCRIPT
}

# 顺序：与 install-all 的步骤表一一对应；配置覆写/纯动作步骤没有卸载语义，跳过
for step in $STEP_ORDER; do
    if word_in "$step" "$STEP_CONFIG_ONLY"; then continue; fi
    fn="uninstall_$(echo "$step" | tr '-' '_')"
    run_step "$step" "$fn"
done

echo
echo "═══════════════════════════════════════════════════════════"
echo "已卸载：${DONE:-（无）}"
if [ -n "$FAILED" ]; then
    echo "❌ 失败：$FAILED —— 看对应步骤上面的原始报错，不会自动重试"
fi
echo "─── 不在本脚本范围内 ───"
echo "· chrony-cn.sh / timezone-cn.sh：配置覆写，没有卸载语义，不动"
echo "· vellum/xovi/qt-resource-rebuilder/appload 本体、KOReader 侧载：不代卸"
echo "· 中文化（输入法/候选栏/UI 汉化）：不在本仓库，本脚本管不到"
echo "· 以上改动多数要等下次 xochitl 重启才会在当前运行中的进程里真正停止生效——本脚本不主动触发重启；"
echo "    要重启：设备上 systemctl restart xochitl（xovi 已生效时别用 xovi/start，会让 xochitl SEGV 整机重启）"
echo "═══════════════════════════════════════════════════════════"
[ -z "$FAILED" ]
