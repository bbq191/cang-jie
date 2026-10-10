#!/bin/sh
# shellcheck shell=sh
# ═══════════════════════════════════════════════════════════════════════════
# lib.sh —— packaging/ 下所有 host 侧脚本共用的函数库（2026-09-20 脚本审计后抽出）。
#
# 用法：调用方先 `cd "$(dirname "$0")"`（进 packaging/），再 `. ./lib.sh`，设好全局 HOST。
# 设备侧对应的库是 devlib.sh（本库的 dev_script 会把它拼在 heredoc 脚本前面经 ssh 送上设备）。
#
# 提供：
#   ssh 封装（M11）  rssh（stdin=/dev/null）· rssh_in（透传 stdin）· rscp —— 统一
#                    BatchMode + ConnectTimeout，休眠/断线时快速失败而不是卡死
#   shquote          把任意字符串安全地拼进远端命令行（M9/M10）
#   dev_pipe         完整脚本（stdin）整组包成 `{ …; } </dev/null` 经 `ssh sh -s` 在设备上执行（断在半截不会执行半截）
#   dev_script       `dev_script ARG… <<'EOF' … EOF`：devlib.sh + 脚本体经 dev_pipe 在设备上执行
#   device_awake_hold / device_awake_release   顶层编排期间持一把带超时的设备唤醒锁，别让设备在两次 ssh 之间睡过去
#   push_verified    一批文件 scp 到"暂存路径"→ 逐个 md5 对拍；不对就删暂存并失败，绝不落到最终位置（H3）；
#                    一次 ssh 建目录并取现有 md5 + 只 scp 有变化的文件 + 有上传才再一次 ssh 复核 md5
#   push_tar_verified 整包 tar 经 ssh stdin 送上设备 → md5 对拍 + 解包核对 → 才换位（PK-4，deploy.sh 的 shelf 载荷）
#   步骤表           STEP_ORDER / step_script / STEP_DEFER / STEP_CONFIG_ONLY（install-all 与 uninstall-all 共用，
#                    保证两边清单对称）
#   parse_step_args  install-all / uninstall-all 共用的 [host] --force --purge --force-apply --dry-run --skip -h 解析
#                    （调用方先定义 usage()）
#   run_step / skip_has   （DRY=1 时 run_step 只打印将执行的命令，不连设备；SKIPPED/NOTAPPL/DONE/FAILED 记账）
#   print_step_summary    install-all / uninstall-all 收尾汇总
#   step_skipped     步骤因前置条件不满足而跳过（非失败）：打印原因；在 run_step 编排下另记入汇总的"前置条件不满足"栏
#   host_arg         薄 deploy-*.sh 共用的 [host] 参数解析（-h、多余/未知参数 exit 2）
#   require_device   动手前确认 ssh 通；不通给下一步排查提示并 exit 1
#   fw_gate          固件 sha256 白名单门（install-all）
#   preflight_device 设备只读预检：root/ /home 可写与剩余空间/xovi·qrr·verity 现状（install-all）
# ═══════════════════════════════════════════════════════════════════════════

CJ_PKG_DIR="$(pwd)"
CJ_SSH_TIMEOUT="${CJ_SSH_TIMEOUT:-8}"

# shellcheck disable=SC2086  # CJ_SSH_OPTS 有意按词展开成多个选项
rssh()    { ssh -n $CJ_SSH_OPTS "root@$HOST" "$@"; }
# shellcheck disable=SC2086
rssh_in() { ssh $CJ_SSH_OPTS "root@$HOST" "$@"; }
# shellcheck disable=SC2086
rscp()    { scp -q $CJ_SSH_OPTS "$@"; }
# ServerAlive*（2026-10-09）：ConnectTimeout 只管"连上之前"。连上之后设备休眠/拔线/WiFi 掉了，TCP 收不到 FIN，
# 没有保活的 ssh/scp 会一直挂着（推 20MB 书架载荷时最容易撞上）；5 秒一探、连续 3 次没回应（约 15 秒）就断开报错。
# 只是协议层心跳，设备端命令长时间不输出（shelf 安装、等重启）不受影响。
CJ_SSH_OPTS="-o BatchMode=yes -o ConnectTimeout=$CJ_SSH_TIMEOUT -o ServerAliveInterval=5 -o ServerAliveCountMax=3"

# 单引号转义，可安全拼进远端 shell 命令行
shquote() {
    printf "'%s'" "$(printf '%s' "$1" | sed "s/'/'\\\\''/g")"
}

# dev_pipe [ARG…]  ← stdin 是一段完整的设备端脚本，经 ssh 在设备上 `sh -s -- ARG…` 执行。
# 整段包成 `{ …; } </dev/null` 再送（2026-10-09）：
#   · shell 要读到配对的 `}` 才开始执行整组——传输中途断了（ssh 掉线、设备休眠）只会是语法错误、一条都不执行，
#     不会出现"前半截已执行、后半截没到"（卸载/写 /usr 的脚本跑一半最难收拾）；
#   · 组内命令的标准输入是 /dev/null：`sh -s` 边读边执行，任何读 stdin 的子命令都会把后面还没执行的脚本当输入吃掉。
dev_pipe() {
    ds_args=""
    for ds_a in "$@"; do ds_args="$ds_args $(shquote "$ds_a")"; done
    { echo '{'; cat; printf '\n} </dev/null\n'; } | rssh_in "sh -s --$ds_args"
}
# dev_script [ARG…]  ← stdin 是脚本体。devlib.sh + 脚本体经 dev_pipe 在设备上执行。
dev_script() { { cat "$CJ_PKG_DIR/devlib.sh"; cat; } | dev_pipe "$@"; }

# run_apply CMD…：跑一段可能让设备主动整机重启的设备端命令（内部调了 devlib.sh 的 cj_xochitl_apply）。输出照常
# 打到终端并留一份；见到设备端打印的 CJ-APPLY-REBOOTING 就按成功处理——ssh 随重启断开（退出码 255）不算失败——
# 并提示设备回来后跑 verify-on-device.sh；见到 CJ-APPLY-REBOOT-FAILED（systemctl reboot 本身失败）则不空等、按失败返回。
# 其余情况原样返回 CMD 的退出码。stdin 原样交给 CMD（可接 heredoc）。
run_apply() {
    ra_log="$(mktemp)"; ra_rcf="$(mktemp)"
    { ra_c=0; "$@" || ra_c=$?; echo "$ra_c" > "$ra_rcf"; } | tee "$ra_log"
    ra_rc="$(cat "$ra_rcf")"
    if grep -q '^CJ-APPLY-REBOOT-FAILED$' "$ra_log"; then
        rm -f "$ra_log" "$ra_rcf"
        echo "!! 设备没能排上整机重启（见上），改动尚未生效；设备上手动 reboot 后跑：sh verify-on-device.sh $HOST"
        [ "$ra_rc" != 0 ] || ra_rc=1
        return "$ra_rc"
    fi
    if grep -q '^CJ-APPLY-REBOOTING$' "$ra_log"; then
        rm -f "$ra_log" "$ra_rcf"
        # 告诉顶层编排者（device_awake_release）设备重启过了：唤醒锁已随重启消失，不必再连一次去放
        if [ -n "${CJ_AWAKE_REBOOTED_FILE:-}" ]; then echo rebooted > "$CJ_AWAKE_REBOOTED_FILE" 2>/dev/null || true; fi
        wait_reboot_and_verify
        return $?
    fi
    rm -f "$ra_log" "$ra_rcf"
    return "$ra_rc"
}

# wait_reboot_and_verify：设备已排上整机重启——先等它断开（免得关机前就连上又判"回来了"），再等它回来，
# 再给 xovi-reenable 与各服务一点时间起齐，然后跑 verify-on-device.sh，退出码即结果（有 ✗ 为 1）。
# CJ_APPLY_VERIFY=0 只提示不等；CJ_REBOOT_DOWN_WAIT / CJ_REBOOT_UP_WAIT / CJ_REBOOT_SETTLE（秒，缺省 60/240/20）。
wait_reboot_and_verify() {
    if [ "${CJ_APPLY_VERIFY:-1}" = 0 ]; then
        echo "✅ 改动已落盘，设备正在整机重启让它生效（约 1 分钟）。回来后核对：sh verify-on-device.sh $HOST"
        return 0
    fi
    echo "-- 改动已落盘，设备正在整机重启；等它回来后自动核对（不想等：Ctrl-C，之后自己跑 sh verify-on-device.sh $HOST）"
    wr_t=0
    while [ "$wr_t" -lt "${CJ_REBOOT_DOWN_WAIT:-60}" ] && rssh true >/dev/null 2>&1; do
        sleep 2; wr_t=$((wr_t + 2))
    done
    wr_t=0
    until rssh true >/dev/null 2>&1; do
        if [ "$wr_t" -ge "${CJ_REBOOT_UP_WAIT:-240}" ]; then
            echo "!! 设备 ${CJ_REBOOT_UP_WAIT:-240} 秒内没回来。检查连接（WiFi/USB）后手动跑：sh verify-on-device.sh $HOST"
            return 1
        fi
        sleep 5; wr_t=$((wr_t + 5))
    done
    sleep "${CJ_REBOOT_SETTLE:-20}"
    echo "-- 设备已回来，核对："
    sh "$CJ_PKG_DIR/verify-on-device.sh" "$HOST"
}

# host_arg USAGE "$@"：薄 deploy-*.sh 共用的参数解析——`[host]`，-h/--help 打印用法，多余/未知参数 exit 2。设 HOST。
host_arg() {
    ha_usage="$1"; shift
    case "${1:-}" in -h|--help) echo "$ha_usage"; exit 0 ;; esac
    [ $# -le 1 ] || { echo "!! 参数太多：$*"; echo "$ha_usage"; exit 2; }
    HOST="${1:-10.11.99.1}"
    case "$HOST" in -*) echo "!! 未知参数：$HOST"; echo "$ha_usage"; exit 2 ;; esac
}

# require_device：动手前先确认 ssh 通（BatchMode，不会卡在密码提示上）；不通给出下一步该查什么，exit 1。
# install-all 自己确认过后导出 CJ_DEVICE_OK=<host>，它编排的各步骤就不再各连一次（整轮省 10 次连接）；
# 之后设备真断了，该步骤的第一条 ssh 会原样报错、记为失败。
require_device() {
    [ "${CJ_DEVICE_OK:-}" != "$HOST" ] || return 0
    rd_err="$(rssh true 2>&1)" || {
        echo "!! 连不上 root@$HOST（${CJ_SSH_TIMEOUT}s 超时，BatchMode）。ssh 报错："
        echo "$rd_err" | sed 's/^/     /'
        echo "   下一步：① 设备是否休眠/没插 USB（接口消失=物理连接或休眠）；② 接口在但 IP 不对：sudo ip addr add 10.11.99.2/24 dev <网卡>；"
        echo "           ③ 提示 host key 变了（OTA/重装后常见）：ssh-keygen -R $HOST；④ 提示 Permission denied：ssh-copy-id root@$HOST"
        exit 1
    }
}

md5_local() { md5sum "$1" | awk '{print $1}'; }

# ── 部署期间不让设备自动休眠（2026-10-09）──────────────────────────────────
# 设备离开 USB 后几秒就会自动休眠（见 chrony-boot-wakelock.service 头注）：走 WiFi 部署时，两次 ssh 之间（交叉编译、
# 打包、推送 20MB 载荷）设备可能睡过去，下一条 ssh/scp 就卡住或失败，留下一个装了一半的步骤。
# device_awake_hold 在设备上持一把带超时的内核唤醒锁（/sys/power/wake_lock 写 "名字 超时纳秒"，到点内核自动释放——
# host 被 Ctrl-C、断网、崩掉都不会让设备永远不睡，最坏多醒 CJ_AWAKE_SECS 秒）；device_awake_release 提前放掉。
# 只由顶层编排者（install-all / uninstall-all / 单独跑的 deploy.sh）调用；它导出 CJ_AWAKE_HELD，被它编排的子脚本
# 不再各拿各放（否则子脚本收尾时会把整轮的锁提前放掉）。没有该接口（[ -w ] 不成立）时什么都不做。CJ_AWAKE=0 关掉。
CJ_AWAKE_SECS="${CJ_AWAKE_SECS:-1200}"
CJ_AWAKE_MINE=0
device_awake_hold() {
    [ -z "${CJ_AWAKE_HELD:-}" ] && [ "${CJ_AWAKE:-1}" != 0 ] || return 0
    rssh "[ -w /sys/power/wake_lock ] && echo cangjie-deploy ${CJ_AWAKE_SECS}000000000 > /sys/power/wake_lock 2>/dev/null; exit 0" >/dev/null 2>&1 || return 0
    CJ_AWAKE_MINE=1
    CJ_AWAKE_REBOOTED_FILE="$(mktemp)"
    export CJ_AWAKE_HELD=1 CJ_AWAKE_REBOOTED_FILE
}
device_awake_release() {
    [ "$CJ_AWAKE_MINE" = 1 ] || return 0
    CJ_AWAKE_MINE=0
    if [ ! -s "$CJ_AWAKE_REBOOTED_FILE" ]; then
        rssh "[ -w /sys/power/wake_unlock ] && echo cangjie-deploy > /sys/power/wake_unlock 2>/dev/null; exit 0" >/dev/null 2>&1 || true
    fi
    rm -f "$CJ_AWAKE_REBOOTED_FILE"
    unset CJ_AWAKE_HELD CJ_AWAKE_REBOOTED_FILE
}

# push_verified LOCAL REMOTE [LOCAL REMOTE …]：把一批文件 scp 到各自的"暂存路径"（调用方保证不在 extensions.d
# 之类的自动加载目录里）并核对 md5。md5 对不上的那个文件从设备上删掉、整体返回 1；最终位置从未被碰过。
# 连接次数：一次 ssh 建好所有目标目录并取回暂存路径上现有文件的 md5 → 只 scp 内容有变化的文件（2026-09-30 起：
# 暂存路径上已是同一内容就不重传，重复部署不再每次推 .so/脚本）→ 有上传才再一次 ssh 取回它们的 md5
# （2026-09-25 起合批；旧版每个文件 mkdir/scp/md5 各一次，deploy-xovi-ext 光推送就 12 次连接）。
pv_md5_cmd() { # 远端命令：对参数里（已 shquote）的每个路径输出一行 md5（缺失则空行），顺序与参数一致
    echo "for f in$1; do s=\$(md5sum \"\$f\" 2>/dev/null) || s=; echo \"\${s%% *}\"; done"
}
push_verified() {
    { [ $# -ge 2 ] && [ $(($# % 2)) -eq 0 ]; } || { echo "!! push_verified：参数必须成对（LOCAL REMOTE …）"; return 1; }
    pv_dirs=""; pv_rems=""; pv_odd=1
    for pv_a in "$@"; do
        if [ "$pv_odd" = 0 ]; then
            pv_d="$(shquote "$(dirname "$pv_a")")"
            case " $pv_dirs " in *" $pv_d "*) ;; *) pv_dirs="$pv_dirs $pv_d" ;; esac
            pv_rems="$pv_rems $(shquote "$pv_a")"
        fi
        pv_odd=$((1 - pv_odd))
    done
    pv_pre="$(rssh "mkdir -p$pv_dirs && $(pv_md5_cmd "$pv_rems")")" || return 1
    # 逐个比对：暂存路径上已是同一内容就跳过，否则 scp 并记下要复核的序号与路径
    pv_up=""; pv_uprems=""; pv_odd=1; pv_k=0
    for pv_a in "$@"; do
        if [ "$pv_odd" = 1 ]; then pv_local=$pv_a
        else
            pv_k=$((pv_k + 1))
            pv_l="$(md5_local "$pv_local")"
            if [ -n "$pv_l" ] && [ "$pv_l" = "$(printf '%s\n' "$pv_pre" | sed -n "${pv_k}p")" ]; then
                echo "-- 未变，不重传：$(basename "$pv_local")"
            else
                rscp "$pv_local" "root@$HOST:$pv_a" || { echo "!! scp $pv_local 失败"; return 1; }
                pv_up="$pv_up $pv_k"; pv_uprems="$pv_uprems $(shquote "$pv_a")"
            fi
        fi
        pv_odd=$((1 - pv_odd))
    done
    [ -n "$pv_up" ] || return 0
    # 刚传的每个文件一行 md5，顺序与 pv_up 一致
    pv_sums="$(rssh "$(pv_md5_cmd "$pv_uprems")")" || pv_sums=""
    pv_bad=""; pv_odd=1; pv_k=0; pv_j=0
    for pv_a in "$@"; do
        if [ "$pv_odd" = 1 ]; then pv_local=$pv_a
        else
            pv_k=$((pv_k + 1))
            if word_in "$pv_k" "$pv_up"; then
                pv_j=$((pv_j + 1))
                pv_l="$(md5_local "$pv_local")"
                pv_r="$(printf '%s\n' "$pv_sums" | sed -n "${pv_j}p")"
                if [ -z "$pv_r" ] || [ "$pv_l" != "$pv_r" ]; then
                    echo "!! md5 对不上：$(basename "$pv_local")（本地 $pv_l vs 设备 ${pv_r:-空}），传输可能损坏，不继续（已删设备上的暂存文件 $pv_a，最终位置未动）"
                    pv_bad="$pv_bad $(shquote "$pv_a")"
                else
                    echo "-- md5 一致：$(basename "$pv_local")"
                fi
            fi
        fi
        pv_odd=$((1 - pv_odd))
    done
    [ -z "$pv_bad" ] && return 0
    rssh "rm -f$pv_bad" || true
    return 1
}

# push_tar_verified LOCAL_TAR REMOTE_DIR MUST_FILE：整包载荷（tar）经 ssh 标准输入送到设备，md5 对上、解包后 MUST_FILE
# （相对 REMOTE_DIR）存在，才把 REMOTE_DIR 换成新的；任一步不对就删掉半成品、返回 1，现成的 REMOTE_DIR 原样不动。
# 2026-10-10（审计 PK-4）：旧版 deploy.sh 只看解包后有没有 install.sh——传输中途被截断/损坏但 tar 恰好还能解开时，
# 缺文件、坏二进制会一路装下去；其余步骤都用 push_verified 逐个 md5，这里补上同一道校验。
# tar 先整个落盘（REMOTE_DIR.tar.new）再算 md5、再解：设备 busybox 没有进程替换，边收边算要 fifo，不值得；代价是
# /home 上临时多占一份载荷大小（约 20MB）。设备端 md5sum 与 push_verified 同一个命令（busybox 自带）。
push_tar_verified() {
    # 远端会对 REMOTE_DIR 与它的 .new/.tar.new 做 rm -rf：只许设备 /home/root 下的具体子目录
    case "$2" in /home/root/?*) ;; *) echo "!! push_tar_verified：拒绝 /home/root 之外的目标 $2"; return 1 ;; esac
    case "$2" in *..*) echo "!! push_tar_verified：目标不许含 ..：$2"; return 1 ;; esac
    pt_sum="$(md5_local "$1")" || return 1
    pt_d="$(shquote "$2")"; pt_n="$(shquote "$2.new")"; pt_t="$(shquote "$2.tar.new")"; pt_m="$(shquote "$2.new/$3")"
    rssh_in "rm -rf $pt_n $pt_t && mkdir -p $pt_n || exit 1
if ! { cat > $pt_t && s=\$(md5sum $pt_t) && [ \"\${s%% *}\" = $pt_sum ] && tar -C $pt_n -xf $pt_t && [ -f $pt_m ]; }; then
    echo \"!! 载荷校验没过（md5 应为 $pt_sum，设备上收到 \${s%% *}；或解包后缺 $3）：传输可能截断/损坏，已删半成品，$2 未动\"
    rm -rf $pt_n $pt_t; exit 1
fi
rm -f $pt_t && rm -rf $pt_d && mv $pt_n $pt_d" < "$1"
}

# ── 固件安全门（install-all 用）──────────────────────────────────────────
# 白名单 = 仓库里的 firmware-allowlist.txt + 本机 firmware-allowlist.local.txt（--force 追加到后者，已 gitignore，
# 不再改动被 git 跟踪的文件，避免"未验证的哈希"被误提交；可用 CJ_ALLOWLIST_LOCAL 改路径）。
CJ_ALLOWLIST="${CJ_ALLOWLIST:-$CJ_PKG_DIR/firmware-allowlist.txt}"
CJ_ALLOWLIST_LOCAL="${CJ_ALLOWLIST_LOCAL:-$CJ_PKG_DIR/firmware-allowlist.local.txt}"
fw_gate() { # $1=FORCE(0/1)
    echo "═══ 固件安全门（root@$HOST）═══"
    fw_hash="$(rssh 'sha256sum /usr/bin/xochitl' | awk '{print $1}')"
    if [ -z "$fw_hash" ]; then
        echo "!! 没拿到 /usr/bin/xochitl 的 sha256（ssh 连不上，或设备上没有这个文件？）"
        return 1
    fi
    fw_line=""
    for fw_f in "$CJ_ALLOWLIST" "$CJ_ALLOWLIST_LOCAL"; do
        [ -f "$fw_f" ] || continue
        fw_line="$(grep "^${fw_hash}[[:space:]]" "$fw_f" | head -n 1)" && [ -n "$fw_line" ] && break
        fw_line=""
    done
    if [ -n "$fw_line" ]; then
        echo "-- 固件命中白名单：$(echo "$fw_line" | awk '{$1=""; print}')"
    elif [ "$1" = "1" ]; then
        echo "⚠️  固件不在白名单（sha256=$fw_hash），--force 强装——追加进 $CJ_ALLOWLIST_LOCAL（本机文件，不入 git）"
        echo "$fw_hash  (--force 追加，未验证，$(date +%Y-%m-%d))" >> "$CJ_ALLOWLIST_LOCAL"
    else
        echo "!! 固件不在白名单（sha256=$fw_hash）。"
        echo "   这台设备的 xochitl 没有在这套安装脚本上验证过注入定位，qmd/hook 偏移可能对不上。"
        echo "   确认这台设备的固件确实跟已验证过的版本一致，要强装就加 --force（会记录这个哈希到本机文件）。"
        return 1
    fi
}

# ── 步骤表（install-all / uninstall-all 共用；两边清单靠它对称）──────────────
# 顺序：先与 xovi/vellum 无关的独立项，再 wifi-watch，再依赖 xovi 的，shelf 最重，xovi-apply 放最后统一重启一次。
STEP_ORDER="chrony-cn chrony-boot-wakelock timezone-cn wifi-watch xovi-persist hl-snap ui-font shelf xovi-apply"
# 只落盘、不各自重启 xochitl 的步骤（install-all 给它们传 DEFER_XOVI_START=1，最后由 xovi-apply 统一重启）
# shellcheck disable=SC2034  # 由 install-all.sh 使用
STEP_DEFER="hl-snap ui-font"
# 已退役的步骤：install-all 不再装，uninstall-all 照样卸（装过的设备还能清干净）；安装件（deploy 脚本与载荷）已删，
# 没有 step_script 映射。
#   sidebar-entry：KOReader/WeRead 的 Sidebar 入口（2026-09-29 用户卸了设备上的 KOReader、WeRead 与 appload；
#                  2026-09-30 删掉 deploy-sidebar-entry.sh 与它的 qmd/图标——appload 不在，它本来也装不上）
#   battop：电池刺客（耗电诊断常驻服务 + /usr 单元），2026-09-30 用户要求移除；源码 enhance/battop 与 deploy-battop.sh 已删
#   handwriting-stroke：手写优化（xovi 扩展 hw-stroke.so），2026-09-30 同批移除；源码与 deploy-handwriting-stroke.sh 已删
# 清理函数：sidebar-entry 在 uninstall-all.sh，battop/handwriting-stroke 在 removal.sh（install-all 也要用）。
# shellcheck disable=SC2034  # 由 uninstall-all.sh 使用
STEP_RETIRED="sidebar-entry battop handwriting-stroke"
# 退役步骤里，install-all 每次重新部署时顺手自动清掉旧设备残留的（清理函数在 removal.sh，与 uninstall-all 同一份）。
# 在 xovi-apply 之前跑：摘了 xochitl 正加载着的 hw-stroke.so 会记待生效标记，由 xovi-apply 统一整机重启（不停/不重启
# xochitl、不在 xovi 已生效时跑 xovi/start）。sidebar-entry 不在这里：它会删 cangjie-icons.rcc，历史上别的 qmd 也用过
# 这个文件名，只在用户明确跑 uninstall-all 时才清。
# shellcheck disable=SC2034  # 由 install-all.sh 使用
STEP_RETIRED_AUTOCLEAN="battop handwriting-stroke"
# 没有"卸载"语义的步骤：配置覆写（chrony-cn/timezone-cn），以及纯动作（xovi-apply）
# shellcheck disable=SC2034  # 由 uninstall-all.sh 使用
STEP_CONFIG_ONLY="chrony-cn timezone-cn xovi-apply"

step_script() {
    case "$1" in
        chrony-cn) echo ./deploy-chrony-cn.sh ;;
        chrony-boot-wakelock) echo ./deploy-chrony-boot-wakelock.sh ;;
        timezone-cn) echo ./deploy-timezone-cn.sh ;;
        wifi-watch) echo ./deploy-wifi-watch.sh ;;
        xovi-persist) echo ./deploy-xovi-persist.sh ;;
        hl-snap) echo ./deploy-hl-snap.sh ;;
        ui-font) echo ./deploy-ui-font.sh ;;
        shelf) echo ./deploy.sh ;;
        xovi-apply) echo ./deploy-xovi-apply.sh ;;
        *) return 1 ;;
    esac
}
word_in() { case " $2 " in *" $1 "*) return 0 ;; *) return 1 ;; esac; }

# step_payload STEP：该步骤的 deploy-* 推到设备 $HOME 下的载荷——"目录名 文件…"（文件在前、子目录在后）。
# deploy-usr-unit / deploy-xovi-ext 按它定推送目录，uninstall-all 按它清（cj_rm_payload），两边同一份清单
#（2026-09-25 审计；原先两边各写一遍）。没有独立载荷目录的步骤返回 1。
step_payload() {
    case "$1" in
        chrony-boot-wakelock) echo "pkg-chrony-boot-wakelock chrony-boot-wakelock.service" ;;
        xovi-persist) echo "pkg-xovi-persist xovi-reenable.service" ;;
        wifi-watch) echo "pkg-wifi-watch wifi-watch.service wifi-watch.sh" ;;
        hl-snap) echo "hl-snap hl-snap.so deploy/install.sh deploy/xovi-ext-install.sh deploy/devlib.sh deploy" ;;
        ui-font) echo "ui-font ui-font.so deploy/install.sh deploy/xovi-ext-install.sh deploy/devlib.sh deploy" ;;
        # 已退役（2026-09-30）：只剩清理用——旧设备上 deploy-handwriting-stroke.sh 推过来的载荷目录
        handwriting-stroke) echo "hw-stroke hw-stroke.so deploy/install.sh deploy/xovi-ext-install.sh deploy/devlib.sh deploy" ;;
        *) return 1 ;;
    esac
}
step_payload_dir() { sp_p="$(step_payload "$1")" || return 1; echo "${sp_p%% *}"; }

# uninstall_only_skip STEP…：给 uninstall-all.sh 的 --skip 值——跳过除了 STEP… 之外的全部卸载步骤（逗号分隔）。
# verify-on-device.sh / 文档给"只清某几样"的命令时用它，步骤表变了命令跟着变，不用另写一份。
uninstall_only_skip() {
    uo_out=""
    for uo_s in $STEP_ORDER $STEP_RETIRED; do
        word_in "$uo_s" "$STEP_CONFIG_ONLY" && continue
        word_in "$uo_s" "$*" && continue
        uo_out="$uo_out,$uo_s"
    done
    echo "${uo_out#,}"
}

# ── 参数解析 / 步骤运行 ────────────────────────────────────────────────────
# parse_step_args "$@"  →  设 HOST FORCE PURGE SKIP DRY FORCE_APPLY；未知参数 exit 2。
# 用法：[host] [--force] [--purge] [--force-apply] [--dry-run] [--skip a,b | --skip=a,b] [-h|--help]
#   调用方需先定义 usage()（打印帮助）——-h/--help 调它后 exit 0。
# shellcheck disable=SC2034  # FORCE/PURGE/DRY/FORCE_APPLY 由调用方（install-all/uninstall-all）使用
parse_step_args() {
    HOST="10.11.99.1"; FORCE=0; PURGE=0; SKIP=""; DRY=0; FORCE_APPLY=0
    case "${1:-}" in ""|-*) ;; *) HOST="$1"; shift ;; esac
    while [ $# -gt 0 ]; do
        case "$1" in
            -h|--help) usage; exit 0 ;;
            --force) FORCE=1 ;;
            --purge) PURGE=1 ;;
            --force-apply) FORCE_APPLY=1 ;;
            --dry-run) DRY=1 ;;
            --skip=*) SKIP="${1#--skip=}" ;;
            --skip) [ $# -ge 2 ] || { echo "!! --skip 需要参数（逗号分隔的步骤名）"; exit 2; }; SKIP="$2"; shift ;;
            *) echo "!! 未知参数：$1（-h 看用法）"; exit 2 ;;
        esac
        shift
    done
    # 退役步骤也算已知（uninstall-all 仍会卸它们，--skip sidebar-entry 是合法的）
    for ps_s in $(echo "$SKIP" | tr ',' ' '); do
        word_in "$ps_s" "$STEP_ORDER $STEP_RETIRED" || echo "⚠ --skip 里的 '$ps_s' 不是已知步骤名（已知：$STEP_ORDER $STEP_RETIRED）"
    done
}

# 该步骤名是否要跑（--skip 没点名）
skip_has() { case ",$SKIP," in *",$1,"*) return 0 ;; *) return 1 ;; esac; }

# ── 设备预检（install-all 用）：只读检查，能提前拦住的全在这拦（磁盘满/非 root/写不了 /home），
#   其余（xovi/qrr 缺失）只报告——对应步骤自己会清楚报错或跳过。返回 1 = 不该继续装。──
CJ_MIN_FREE_KB="${CJ_MIN_FREE_KB:-51200}"     # /home 可用空间低于此值拒装（≈50MB：连 shelf 二进制都放不下）
CJ_WARN_FREE_KB="${CJ_WARN_FREE_KB:-204800}"  # 低于此值只警告（≈200MB：shelf 载荷+备份余量偏紧）
preflight_device() {
    echo "═══ 设备预检（root@$HOST，只读）═══"
    dev_script "$CJ_MIN_FREE_KB" "$CJ_WARN_FREE_KB" <<'DEVICE_SCRIPT'
set -eu
MINF="$1"; WARNF="$2"
cj_require_root || exit 1
[ -d "$CJ_HOME" ] && [ -w "$CJ_HOME" ] || { echo "!! $CJ_HOME 不可写——/home 分区没挂载好？先在设备上 mount | grep home"; exit 1; }
FREE="$(df -kP "$CJ_HOME" 2>/dev/null | awk 'END{print $4}')"
case "$FREE" in ''|*[!0-9]*) echo "⚠ 读不到 $CJ_HOME 的可用空间，跳过空间检查" ;; *)
    if [ "$FREE" -lt "$MINF" ]; then
        echo "!! $CJ_HOME 只剩 $((FREE / 1024)) MB 可用，装不下（至少 $((MINF / 1024)) MB）。先清理：ls -la $CJ_HOME、$CJ_BACKUP_DIR"; exit 1
    fi
    if [ "$FREE" -lt "$WARNF" ]; then echo "⚠ $CJ_HOME 只剩 $((FREE / 1024)) MB 可用（建议 ≥ $((WARNF / 1024)) MB），继续但空间偏紧"
    else echo "-- $CJ_HOME 可用 $((FREE / 1024)) MB"; fi ;;
esac
have() { [ -e "$1" ] && echo "有" || echo "无"; }
echo "-- xovi 本体 : $(have "$CJ_XOVI/xovi.so")   （无 → xovi-persist/hl-snap/ui-font/xovi-apply 会失败：先 vellum add xovi）"
echo "-- qt-resource-rebuilder : $(have "$CJ_XOVI/exthome/qt-resource-rebuilder")   （无 → shelf 的 qmd 自动跳过）"
if cj_verity_active; then echo "-- dm-verity : 激活 → 所有写 /usr 的单元（chrony-boot-wakelock/xovi-persist/wifi-watch/shelf 开机链接）会被跳过"; else echo "-- dm-verity : 未激活"; fi
if cj_xochitl_has_xovi; then echo "-- xochitl 里 xovi 已生效 → 有改动时最后一步换入后整机重启（不跑 xovi/start、不 restart xochitl）"; else echo "-- xochitl 里 xovi 尚未生效 → 最后一步让它生效：装了 xovi-reenable（本轮 xovi-persist 会装，dm-verity 下装不上）就整机重启，否则 xovi/start"; fi
DEVICE_SCRIPT
}

DONE=""
FAILED=""
SKIPPED=""
NOTAPPL=""   # 步骤自己判定前置条件不满足、跳过（退出 0，但不该算进"已安装"）

# step_skipped REASON：步骤因前置条件不满足/不适用而跳过（非失败）时调用，调用后照常 exit 0。
# 单独跑时只打印原因；在 run_step 编排下（它导出 CJ_STEP_SKIP_FILE）另把原因写进该文件，
# 汇总时这一步列进"已跳过（前置条件不满足）"而不是"已安装"——不然"跳过"混在"已安装"里，容易漏看。
step_skipped() {
    echo "-- $1——跳过，非失败"
    if [ -n "${CJ_STEP_SKIP_FILE:-}" ]; then printf '%s\n' "$1" > "$CJ_STEP_SKIP_FILE"; fi
}

# run_step NAME CMD [ARGS…]：跳过判定 + 执行 + 记账（DRY=1 时只打印将执行的命令，不执行、不连设备）
run_step() {
    rs_name="$1"; shift
    if skip_has "$rs_name"; then
        echo; echo "-- 跳过 $rs_name（--skip）"
        SKIPPED="$SKIPPED $rs_name"
        return 0
    fi
    echo; echo "═══ $rs_name ═══"
    if [ "${DRY:-0}" = "1" ]; then
        echo "-- [dry-run] 将执行：$*"
        DONE="$DONE $rs_name"
        return 0
    fi
    rs_skipf="$(mktemp)"
    export CJ_STEP_SKIP_FILE="$rs_skipf"
    if "$@"; then
        if [ -s "$rs_skipf" ]; then
            NOTAPPL="$NOTAPPL
   $rs_name：$(head -n 1 "$rs_skipf")"
        else
            DONE="$DONE $rs_name"
        fi
    else
        echo "!! $rs_name 失败（见上面这一步的原始报错）"
        FAILED="$FAILED $rs_name"
    fi
    unset CJ_STEP_SKIP_FILE
    rm -f "$rs_skipf"
}

# print_step_summary 动词：install-all / uninstall-all 收尾汇总的公共部分（动词=已安装/已卸载；DRY=1 时打印计划）
print_step_summary() {
    if [ "${DRY:-0}" = "1" ]; then echo "dry-run 计划（未连接设备、未执行）：${DONE:-（无）}"
    else echo "$1：${DONE:-（无）}"; fi
    [ -z "$SKIPPED" ] || echo "已跳过（--skip）：$SKIPPED"
    [ -z "$NOTAPPL" ] || echo "已跳过（前置条件不满足，非失败）：$NOTAPPL"
    [ -z "$FAILED" ] || echo "❌ 失败：$FAILED —— 看对应步骤上面的原始报错，不会自动重试"
}
