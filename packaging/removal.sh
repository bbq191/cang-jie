#!/bin/sh
# shellcheck shell=sh
# ═══════════════════════════════════════════════════════════════════════════
# removal.sh —— 摘除 xovi 扩展、清理已移除功能的设备端动作（host 侧函数库，2026-09-30 从 uninstall-all.sh 抽出）。
#
# 用法：先 `. ./lib.sh`（要它的 dev_script / step_payload），再 `. ./removal.sh`。两个调用方共用同一份：
#   · uninstall-all.sh：uninstall_hl_snap 用 remove_xovi_extension；退役步骤 battop / handwriting-stroke 照样卸
#   · install-all.sh：重新部署时对 lib.sh 的 STEP_RETIRED_AUTOCLEAN 逐个调用 uninstall_<步骤>，自动清旧设备残留
#
# 2026-09-30 移除的两样（用户要求"移除电池刺客/手写优化及其开关相关功能和模块"）：
#   battop              电池刺客：/usr 的 battop.service（+ 旧版 battop.timer）、/home/root/battop（二进制 + 采样数据 +
#                       deploy-battop.sh 推来的 install.sh/devlib.sh）。服务不是 xochitl，停它不碰 xochitl。
#   handwriting-stroke  手写优化：extensions.d/hw-stroke.so（+ .crashed 标记）、待换入区里的副本、载荷目录 /home/root/hw-stroke/。
#
# 红线（与 devlib.sh 头注 H1/H3 一致）：
#   · 摘 .so 只 rm 文件（运行中的 xochitl 继续用已映射的旧 inode，不受影响），**绝不** stop/restart xochitl，
#     也不在 xovi 已生效时跑 xovi/start；xochitl 正加载着它时记待生效标记，让随后的 xovi-apply（install-all 的最后
#     一步）整机重启生效；单独跑 uninstall-all 则提示用户自己整机重启。
#   · 不往 extensions.d 放任何备份；不写 /usr——删单元只走 devlib 的 cj_remove_usr_unit（dm-verity 门 + 带 trap 的 rw 窗口）。
# ═══════════════════════════════════════════════════════════════════════════

# 从 extensions.d 摘除一个 xovi 扩展本体（+ 清同名的 .crashed 崩溃标记 + 待换入区副本 + 推送载荷目录）。不碰 reading-qol.json
# （多个扩展共用同一份配置，卸一个不该动别人的开关）、不碰 cangjie-backups/（那是回滚安全网）。$1=.so 文件名 $2=步骤名
remove_xovi_extension() {
    # shellcheck disable=SC2046  # step_payload 有意按词展开成 "目录 文件…"
    dev_script "$1" $(step_payload "$2") <<'DEVICE_SCRIPT'
set -eu
SO="$1"; PKG="$2"; shift 2
cj_require_root || exit 1
EXT="$CJ_XOVI/extensions.d"
# 待换入区里的新版也要撤掉：否则下一次 cj_xochitl_apply（xovi-apply / 任何单独部署）会把刚卸掉的扩展又换进 extensions.d
if [ -f "$CJ_SO_PENDING_DIR/$SO" ]; then echo "-- 撤掉待换入区里的 $SO"; fi
cj_so_unstage "$SO"
MAPPED=0
if cj_xochitl_has_xovi && [ "$(cj_count_maps "$SO" "$(cj_xochitl_pid)")" -gt 0 ]; then MAPPED=1; fi
REMOVED=0
if [ -f "$EXT/$SO" ] || [ -e "$EXT/$SO.crashed" ]; then
    [ -f "$EXT/$SO" ] && REMOVED=1
    rm -f "$EXT/$SO" "$EXT/$SO.crashed"
    echo "-- 已从 extensions.d 摘除 $SO（reading-qol.json 配置、cangjie-backups/ 下的历史备份不动）"
else
    echo "-- $EXT/$SO 本来就不存在"
fi
if [ "$MAPPED" = "1" ] && [ "$REMOVED" = "1" ]; then
    # 与 devlib.sh 头注 H3 同类：运行中的 xochitl 还映射着刚删掉的 .so，此时让它退出（restart/stop）有崩溃→整机重启的风险。
    # 记待生效标记：install-all 随后的 xovi-apply 据此整机重启（不停 xochitl）；单独卸载时 verify 也会提示"待生效"。
    cj_pending_mark "removed-$SO" || true
    echo "   ⚠ 运行中的 xochitl 仍加载着 $SO（已删的旧文件）。要立刻停用请**整机重启**（reboot），别 systemctl restart xochitl。"
fi
cj_rm_payload "$CJ_HOME/$PKG" "$@"
DEVICE_SCRIPT
}

# 手写优化（hw-stroke.so，2026-09-30 移除）
uninstall_handwriting_stroke() { remove_xovi_extension hw-stroke.so handwriting-stroke; }

# 电池刺客（battop，2026-09-30 移除）：停用 + 删 /usr 单元（含 2026-09-20 前旧版遗留的 battop.timer）+ 删 /home/root/battop
# 整个目录（二进制、采样数据、推送来的 install.sh/devlib.sh——功能移除后这些数据没有读者了，不再区分 --purge）。
# 什么都没有时不碰 systemd、不 remount（install-all 每次都会调它）。
uninstall_battop() {
    dev_script <<'DEVICE_SCRIPT'
set -eu
cj_require_root || exit 1
BD="$CJ_HOME/battop"
W="$CJ_SYSD/multi-user.target.wants"
TW="$CJ_SYSD/timers.target.wants"
if [ ! -e "$CJ_SYSD/battop.service" ] && [ ! -L "$W/battop.service" ] && [ ! -e "$CJ_SYSD/battop.timer" ] \
    && [ ! -L "$TW/battop.timer" ] && [ ! -L "$W/battop.timer" ] && [ ! -e "$BD" ] && [ ! -L "$BD" ]; then
    echo "-- 设备上没有电池刺客（battop）的残留"
    exit 0
fi
rc=0; cj_remove_usr_unit battop.service multi-user.target.wants || rc=$?
[ "$rc" = 0 ] || [ "$rc" = 3 ] || exit 1
rt=0
if [ -e "$CJ_SYSD/battop.timer" ] || [ -L "$TW/battop.timer" ] || [ -L "$W/battop.timer" ]; then
    cj_remove_usr_unit battop.timer timers.target.wants multi-user.target.wants || rt=$?
    [ "$rt" = 0 ] || [ "$rt" = 3 ] || exit 1
fi
rm -f "$CJ_STAGE_DIR/battop.service.src"   # 旧安装器中断时可能留下的单元源暂存
if [ "$rc" = 3 ] || [ "$rt" = 3 ]; then
    # 单元删不掉：它指向 $BD/battop，留着二进制（已 stop/disable；battop 从不建开机链接，旧版的链接在 /etc tmpfs 重启即清）
    echo "-- 单元还在 /usr（dm-verity），保留 $BD；解除 dm-verity 或 OTA 冲掉单元后重跑会删掉它"
    exit 0
fi
case "$BD" in /?*/battop) ;; *) echo "!! 拒绝清除异常路径 $BD"; exit 1 ;; esac
if [ -L "$BD" ]; then echo "!! $BD 是符号链接，不删（请手动核对它指向哪里）"; exit 1; fi
if [ -d "$BD" ]; then
    echo "-- 删除 $BD（$(du -sk "$BD" 2>/dev/null | awk '{print $1}') KB：二进制 + 历史采样数据；cangjie-backups/ 里的旧二进制备份不动）"
    rm -rf "$BD"
elif [ -e "$BD" ]; then
    echo "!! $BD 不是目录，不删（请手动核对）"; exit 1
fi
echo "-- 电池刺客（battop）已清除"
DEVICE_SCRIPT
}
