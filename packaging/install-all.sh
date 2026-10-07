#!/bin/sh
# ═══════════════════════════════════════════════════════════════════════════
# cang-jie 全新设备统一安装器（host 侧编排，2026-09-11 新写；2026-09-20 起步骤表与 uninstall-all 共用 lib.sh）。
#
# 只编排、不重新实现任何构建/传输逻辑——依次调用各自独立可用的部署脚本（步骤表见 lib.sh 的 STEP_ORDER）：
#   chrony-cn            国内 NTP（跟 xovi/vellum 无关）
#   chrony-boot-wakelock 开机头几十秒防自动休眠打断 chronyd 首次校时（根因见该 .service 头注）
#   timezone-cn          默认时区 Asia/Shanghai（跟 xovi/vellum 无关）
#   wifi-watch           WiFi 载波假死看护（2026-09-20 接入；跟 xovi/vellum 无关）
#   xovi-persist         xovi 开机持久化恢复链（需要 vellum add xovi）
#   hl-snap              荧光笔 CJK 精确吸附（需要 vellum add xovi；只落盘）
#   ui-font              界面字体（xovi 扩展，换 xochitl 应用默认字体；需要 vellum add xovi；只落盘）
#   （sidebar-entry 已退役，2026-09-29：设备不再用 KOReader/WeRead/appload；安装件 09-30 已删，uninstall-all 仍会清，见 lib.sh STEP_RETIRED）
#   （battop 电池刺客、handwriting-stroke 手写优化 2026-09-30 已移除：不再安装；旧设备上的残留在 xovi-apply 之前自动清，
#     见下面「已移除功能的残留清理」与 removal.sh）
#   shelf                shelf 本体+网关+笔记线+两个领域服务（不需要 xovi；qmd 只落盘）
#   xovi-apply           统一让上面落盘的 xovi 内容生效：有待生效改动（或 xovi 还没生效）才整机重启，且只一次
# 装前先过固件安全门（sha256(/usr/bin/xochitl) 比对 firmware-allowlist.txt），避免在没验证过注入定位的固件上装错。
#
# ⚠️ 生效只做一次，放在最后（xovi-apply）：没有"只重载一个扩展"的机制。hl-snap
# 用 DEFER_XOVI_START=1 只落盘。
# 已移除功能的残留清理（2026-09-30）：xovi-apply 之前对 lib.sh 的 STEP_RETIRED_AUTOCLEAN（battop、handwriting-stroke）
# 各跑一次 removal.sh 里的 uninstall_<步骤>——没有残留就什么都不动；摘了 xochitl 正加载着的 hw-stroke.so 会记待生效
# 标记，随后的 xovi-apply 因此整机重启一次（不停/不重启 xochitl，xovi 已生效时不跑 xovi/start）。`--skip battop`、
# `--skip handwriting-stroke` 可跳过对应清理（例如这次不想被整机重启打断）。
# ⚠️ 怎么生效由设备端 devlib.sh 的 cj_xochitl_apply 判定：2026-09-25 起一律**主动整机重启**（xovi 已生效或装了
# xovi-reenable 时）——单独 restart xochitl 有概率在它退出时崩溃、再由系统整机重启（memfault 栈 5 份，见 devlib.sh
# 头注 H3）；只有既没生效也没装 xovi-reenable 才走 xovi/start。最后一步会先打印"将打断阅读"并留 5 秒宽限；
# 不想被打断就 `--skip xovi-apply`，稍后自己在合适时机跑 deploy-xovi-apply.sh 或在设备上 reboot。
#
# 明确不做的事（范围外，见 packaging/README.md「前置条件」「已知缺口」）：
#   · 不装 vellum/xovi/qt-resource-rebuilder 本体——这些是全新设备共同的手动前置条件，本脚本只在缺失时把报错原样透出，
#     不代为安装。KOReader/WeRead/appload 2026-09-29 起不再是前置条件（不用了）。
#   · 不装中文化（输入法/候选栏/UI 汉化）——那条链路已不在本仓库（见顶层 README「历史与范围」）。
# 对称卸载见 packaging/uninstall-all.sh。
#
# 用法：./install-all.sh [host] [--force] [--force-apply] [--dry-run] [--skip a,b,...]
#   host          默认 10.11.99.1（USB）
#   --force       固件不在白名单也强装（哈希追加进本机 firmware-allowlist.local.txt，不再改动被 git 跟踪的白名单）
#   --force-apply 最后一步（xovi-apply）无论有没有"待生效"的落盘改动都整机重启一次。缺省只在这轮真的改了 xovi 相关
#                 文件（或 xovi 还没在 xochitl 里生效）时才重启——重复跑 install-all 不再每次闪屏
#   --dry-run     只在本机打印将执行的步骤，不连设备、不执行任何东西
#   --skip        逗号分隔，跳过指定步骤（可选值见 lib.sh STEP_ORDER）
# 装前依次：固件安全门（sha256 白名单）→ 设备预检（root/磁盘空间/xovi·qrr·dm-verity 现状，只读）。
# ═══════════════════════════════════════════════════════════════════════════
set -eu
cd "$(dirname "$0")"
# shellcheck disable=SC1091
. ./lib.sh
# shellcheck disable=SC1091
. ./removal.sh   # 已移除功能（battop/handwriting-stroke）的残留清理，与 uninstall-all 同一份

usage() {
    cat <<'EOF'
用法：./install-all.sh [host] [--force] [--force-apply] [--dry-run] [--skip a,b,...]
  host          默认 10.11.99.1（USB）
  --force       固件不在白名单也强装（哈希追加进本机 firmware-allowlist.local.txt）
  --force-apply 无论有无待生效改动，最后都让它生效一次（整机重启；缺省：没改动且 xovi 已生效就不重启）
  --dry-run     只在本机打印计划，不连设备
  --skip        逗号分隔，跳过指定步骤（步骤名见 lib.sh 的 STEP_ORDER；battop/handwriting-stroke = 跳过已移除功能的残留清理）
对称卸载：./uninstall-all.sh
EOF
}

parse_step_args "$@"
[ "$PURGE" = "0" ] || { echo "!! 未知参数：--purge（那是 uninstall-all.sh 的）"; exit 2; }

if [ "$DRY" = "1" ]; then
    echo "═══ dry-run：只打印计划，不连设备（目标 root@$HOST）═══"
else
    require_device
    export CJ_DEVICE_OK="$HOST"   # 各步骤脚本不再各自重复做连通检查（见 lib.sh 的 require_device）
    fw_gate "$FORCE" || exit 1
    preflight_device || exit 1
fi

for step in $STEP_ORDER; do
    script="$(step_script "$step")"
    [ -f "$script" ] || { echo "!! 步骤表里的 $step 没有对应脚本 $script"; exit 1; }
    if word_in "$step" "$STEP_DEFER"; then
        # hl-snap / ui-font 只落盘，不各自触发 xochitl 重启
        run_step "$step" env DEFER_XOVI_START=1 sh "$script" "$HOST"
    elif [ "$step" = "xovi-apply" ]; then
        # 先清已移除功能在旧设备上的残留（摘 .so 记的待生效标记由紧接着的 xovi-apply 统一生效）
        for rstep in $STEP_RETIRED_AUTOCLEAN; do
            if skip_has "$rstep"; then
                echo; echo "-- 跳过 $rstep 的残留清理（--skip）"
                SKIPPED="$SKIPPED $rstep"
                continue
            fi
            run_step "清理已移除:$rstep" "uninstall_$(echo "$rstep" | tr '-' '_')"
        done
        if ! skip_has xovi-apply && [ "$DRY" = "0" ]; then
            echo
            echo "⚠ 下一步会检查是否需要让改动生效（有待生效改动才整机重启，约 1 分钟：打断阅读/书写）。完全不想重启：Ctrl-C，或重跑时加 --skip xovi-apply。"
        fi
        if [ "$FORCE_APPLY" = "1" ]; then
            run_step "$step" sh "$script" "$HOST" --force
        else
            run_step "$step" sh "$script" "$HOST"
        fi
    else
        run_step "$step" sh "$script" "$HOST"
    fi
done

echo
echo "═══════════════════════════════════════════════════════════"
if [ "$DRY" = "1" ]; then
    echo "dry-run 计划（未连接设备、未执行）：${DONE:-（无）}"
else
    echo "已安装：${DONE:-（无）}"
fi
[ -z "$SKIPPED" ] || echo "已跳过（--skip）：$SKIPPED"
[ -z "$NOTAPPL" ] || echo "已跳过（前置条件不满足，非失败）：$NOTAPPL"
if [ -n "$FAILED" ]; then
    echo "❌ 失败：$FAILED —— 看对应步骤上面的原始报错，不会自动重试"
fi
echo "─── 不在本脚本范围内，需要手动处理 ───"
echo "· vellum/xovi/qt-resource-rebuilder 引导（若 xovi-persist/hl-snap/"
echo "    xovi-apply 因缺 xovi.so 失败）：设备上先跑 vellum add xovi qt-resource-rebuilder"
echo "· 中文化（输入法/候选栏/UI 汉化）：不在本仓库，本脚本不装（见顶层 README「历史与范围」）"
echo "═══════════════════════════════════════════════════════════"
[ -z "$FAILED" ]
