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
#   cj_traps / cj_mktemp / cj_tmp_register   本机临时文件登记 + 统一的 EXIT/INT/TERM/HUP 收尾（2026-10-10）
#   device_awake_hold / device_awake_release   顶层编排期间持一把带超时的设备唤醒锁，别让设备在两次 ssh 之间睡过去
#   pv_set / pv_exec 一批文件推到"暂存路径"并逐个 md5 对拍后才跑安装命令；不对就删暂存并失败，绝不落到最终位置（H3）。
#                    比对与安装同一次 ssh：没变化时一次往返装完，有变化时 scp 后再一次往返"复核+安装"（2026-10-10）
#   push_tar_verified 整包 tar 经 ssh stdin 送上设备 → md5 对拍 + 解包核对 → 才换位（PK-4，deploy.sh 的 shelf 载荷）；
#                    设备上已是同一份载荷就不重传（2026-10-10）
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
#   preflight_device 设备只读预检：root/ /home 可写与剩余空间/xovi·qrr·verity 现状（install-all）；同一次 ssh 取固件
#                    sha256 交给 fw_gate、预检通过后顺手拿唤醒锁（2026-10-10 合并，原先三次往返）
#
# 被 source 时只定义函数与 CJ_* / 步骤表变量：不改 shell 选项（set）、不装 trap、不 export（cj_traps 由调用方显式调用；
# 只有 device_awake_hold 与 run_step 会按文档导出 CJ_AWAKE_HELD 等给子脚本）。
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

# ── 本机临时文件与退出收尾（2026-10-10）──────────────────────────────────────
# 旧版各脚本各自 mktemp、各自挂 trap：run_step 的跳过标记、run_apply 的输出/退出码文件在 Ctrl-C/kill 时留在 /tmp；
# verify-on-device 只挂了 EXIT——bash/dash 收到 TERM/HUP 默认直接死、不跑 EXIT trap。现在统一：
#   cj_mktemp [-d]       mktemp 并登记，结果放 $CJ_TMP（不用命令替换：子 shell 里登记会丢）
#   cj_tmp_register P    登记一个本机路径（脚本自己在登记过的目录里建的文件）
#   cj_traps             EXIT → cj_cleanup；INT/TERM/HUP → exit 130/143/129（再经 EXIT 收尾）
#   cj_cleanup           放唤醒锁（若持有）→ 调用方定义的 cj_cleanup_local（如有）→ 删登记的文件、rmdir 登记的目录
# 目录只 rmdir、不递归删；要整树删的（deploy.sh 的载荷暂存树）由调用方在 cj_cleanup_local 里自己删。
# 退出码：EXIT trap 里不调用 exit，脚本原本的退出码不变。
CJ_TMP=""
CJ_TMP_FILES=""
CJ_TMP_DIRS=""
CJ_NL="
"
cj_tmp_register() { CJ_TMP_FILES="$CJ_TMP_FILES$1$CJ_NL"; }
cj_mktemp() {
    if [ "${1:-}" = "-d" ]; then
        CJ_TMP="$(mktemp -d)" || return 1
        CJ_TMP_DIRS="$CJ_TMP$CJ_NL$CJ_TMP_DIRS"   # 后建的先 rmdir
    else
        CJ_TMP="$(mktemp)" || return 1
        cj_tmp_register "$CJ_TMP"
    fi
}
cj_cleanup() {
    device_awake_release
    if command -v cj_cleanup_local >/dev/null 2>&1; then cj_cleanup_local || true; fi
    cc_ifs=$IFS; IFS=$CJ_NL
    for cc_p in $CJ_TMP_FILES; do [ -z "$cc_p" ] || rm -f -- "$cc_p"; done
    for cc_p in $CJ_TMP_DIRS; do [ -z "$cc_p" ] || rmdir -- "$cc_p" 2>/dev/null || true; done
    IFS=$cc_ifs
    CJ_TMP_FILES=""; CJ_TMP_DIRS=""
}
cj_traps() {
    trap 'cj_cleanup' EXIT
    trap 'exit 130' INT
    trap 'exit 143' TERM
    trap 'exit 129' HUP
}

# 单引号转义，可安全拼进远端 shell 命令行
shquote() {
    printf "'%s'" "$(printf '%s' "$1" | sed "s/'/'\\\\''/g")"
}

# dev_pipe [ARG…]  ← stdin 是一段完整的设备端脚本，经 ssh 在设备上 `sh -s -- ARG…` 执行。
# 整段包成 `{ …; } </dev/null` 再送（2026-10-09）：
#   · shell 要读到配对的 `}` 才开始执行整组——传输中途断了（ssh 掉线、设备休眠）只会是语法错误、一条都不执行，
#     不会出现"前半截已执行、后半截没到"（卸载/写 /usr 的脚本跑一半最难收拾）；
#   · 组内命令的标准输入是 /dev/null：`sh -s` 边读边执行，任何读 stdin 的子命令都会把后面还没执行的脚本当输入吃掉。
#   · 组内第一条 `umask 022`（2026-10-10）：设备端片段新建的文件/目录权限不取决于 sshd 会话继承来的 umask
#     （devlib.sh 作为库不改调用方 umask；需要更严的地方各自在子 shell 里 umask 077，如 deploy.sh 的密码文件）。
dev_pipe() {
    ds_args=""
    for ds_a in "$@"; do ds_args="$ds_args $(shquote "$ds_a")"; done
    # CJ_PV_GUARD（只在 pv_exec 调 CMD 期间非空）放在命令行最前：守卫在 `sh -s` 读脚本之前跑，要上传时以 97 退出、
    # 脚本一条都不读不执行
    { echo '{'; echo 'umask 022'; cat; printf '\n} </dev/null\n'; } | rssh_in "${CJ_PV_GUARD}sh -s --$ds_args"
}
# dev_script [ARG…]  ← stdin 是脚本体。devlib.sh + 脚本体经 dev_pipe 在设备上执行。
dev_script() { { cat "$CJ_PKG_DIR/devlib.sh"; cat; } | dev_pipe "$@"; }

# run_apply CMD…：跑一段可能让设备主动整机重启的设备端命令（内部调了 devlib.sh 的 cj_xochitl_apply）。输出照常
# 打到终端并留一份；见到设备端打印的 CJ-APPLY-REBOOTING 就按成功处理——ssh 随重启断开（退出码 255）不算失败——
# 并提示设备回来后跑 verify-on-device.sh；见到 CJ-APPLY-REBOOT-FAILED（systemctl reboot 本身失败）则不空等、按失败返回。
# 其余情况原样返回 CMD 的退出码。stdin 原样交给 CMD（可接 heredoc）。
# 设备端另打一行 CJ-AWAKE-RELEASED（deploy-xovi-apply 在 install-all 末尾顺手放了唤醒锁）时，同样记进
# CJ_AWAKE_REBOOTED_FILE，顶层编排者收尾时就不再单独连一次去放（2026-10-10）。
# 输出/退出码两个本机临时文件经 cj_mktemp 登记（Ctrl-C 也会被 cj_cleanup 删掉），同一进程里反复调用复用同一对。
CJ_RA_LOG=""; CJ_RA_RCF=""
ra_prepare() {
    [ -z "$CJ_RA_LOG" ] || return 0
    cj_mktemp || return 1; CJ_RA_LOG=$CJ_TMP; cj_mktemp || return 1; CJ_RA_RCF=$CJ_TMP
}
run_apply() {
    ra_prepare || return 1
    { ra_c=0; "$@" || ra_c=$?; echo "$ra_c" > "$CJ_RA_RCF"; } | tee "$CJ_RA_LOG"
    ra_rc="$(cat "$CJ_RA_RCF")"
    if grep -q '^CJ-AWAKE-RELEASED$' "$CJ_RA_LOG" && [ -n "${CJ_AWAKE_REBOOTED_FILE:-}" ]; then
        echo released > "$CJ_AWAKE_REBOOTED_FILE" 2>/dev/null || true
    fi
    if grep -q '^CJ-APPLY-REBOOT-FAILED$' "$CJ_RA_LOG"; then
        echo "!! 设备没能排上整机重启（见上），改动尚未生效；设备上手动 reboot 后跑：sh verify-on-device.sh $HOST"
        [ "$ra_rc" != 0 ] || ra_rc=1
        return "$ra_rc"
    fi
    if grep -q '^CJ-APPLY-REBOOTING$' "$CJ_RA_LOG"; then
        # 告诉顶层编排者（device_awake_release）设备重启过了：唤醒锁已随重启消失，不必再连一次去放
        if [ -n "${CJ_AWAKE_REBOOTED_FILE:-}" ]; then echo rebooted > "$CJ_AWAKE_REBOOTED_FILE" 2>/dev/null || true; fi
        wait_reboot_and_verify
        return $?
    fi
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
    # 上面刚连通过：让 verify 跳过它自己的连通检查（省一次往返）
    CJ_DEVICE_OK="$HOST" sh "$CJ_PKG_DIR/verify-on-device.sh" "$HOST"
}

# host_arg USAGE "$@"：薄 deploy-*.sh 共用的参数解析——`[host]`，-h/--help 打印用法，多余/未知参数 exit 2。设 HOST。
host_arg() {
    ha_usage="$1"; shift
    case "${1:-}" in -h|--help) echo "$ha_usage"; exit 0 ;; esac
    [ $# -le 1 ] || { echo "!! 参数太多：$*"; echo "$ha_usage"; exit 2; }
    HOST="${1:-10.11.99.1}"
    case "$HOST" in -*) echo "!! 未知参数：$HOST"; echo "$ha_usage"; exit 2 ;; esac
}

# require_device [--hold]：动手前先确认 ssh 通（BatchMode，不会卡在密码提示上）；不通给出下一步该查什么，exit 1。
# install-all 自己确认过后导出 CJ_DEVICE_OK=<host>，它编排的各步骤就不再各连一次（整轮省 10 次连接）；
# 之后设备真断了，该步骤的第一条 ssh 会原样报错、记为失败。
# --hold（2026-10-10）：连通检查这一次往返顺带拿部署唤醒锁（= device_awake_hold，规则相同），省一次 ssh。
require_device() {
    if [ "${CJ_DEVICE_OK:-}" = "$HOST" ]; then
        [ "${1:-}" != "--hold" ] || device_awake_hold
        return 0
    fi
    rd_cmd=true
    if [ "${1:-}" = "--hold" ] && awake_wanted; then rd_cmd="$(awake_lock_cmd)"; fi
    rd_err="$(rssh "$rd_cmd" 2>&1)" || {
        echo "!! 连不上 root@$HOST（${CJ_SSH_TIMEOUT}s 超时，BatchMode）。ssh 报错："
        echo "$rd_err" | sed 's/^/     /'
        echo "   下一步：① 设备是否休眠/没插 USB（接口消失=物理连接或休眠）；② 接口在但 IP 不对：sudo ip addr add 10.11.99.2/24 dev <网卡>；"
        echo "           ③ 提示 host key 变了（OTA/重装后常见）：ssh-keygen -R $HOST；④ 提示 Permission denied：ssh-copy-id root@$HOST"
        exit 1
    }
    [ "$rd_cmd" = true ] || awake_mark_held
}

md5_local() { md5sum "$1" | awk '{print $1}'; }

# ── 部署期间不让设备自动休眠（2026-10-09）──────────────────────────────────
# 设备离开 USB 后几秒就会自动休眠（见 chrony-boot-wakelock.service 头注）：走 WiFi 部署时，两次 ssh 之间（交叉编译、
# 打包、推送 20MB 载荷）设备可能睡过去，下一条 ssh/scp 就卡住或失败，留下一个装了一半的步骤。
# device_awake_hold 在设备上持一把带超时的内核唤醒锁（/sys/power/wake_lock 写 "名字 超时纳秒"，到点内核自动释放——
# host 被 Ctrl-C、断网、崩掉都不会让设备永远不睡，最坏多醒 CJ_AWAKE_SECS 秒）；device_awake_release 提前放掉。
# 只由顶层编排者（install-all / uninstall-all / 单独跑的 deploy.sh）调用；它导出 CJ_AWAKE_HELD，被它编排的子脚本
# 不再各拿各放（否则子脚本收尾时会把整轮的锁提前放掉）。没有该接口（[ -w ] 不成立）时什么都不做。CJ_AWAKE=0 关掉。
# 2026-10-10：拿锁可以并进别的往返（require_device --hold、preflight_device），放锁也可以由最后一个设备端脚本顺手做
# （它打印 CJ-AWAKE-RELEASED，run_apply / 调用方据此往 CJ_AWAKE_REBOOTED_FILE 写 released）——该文件非空时收尾不再连设备。
# 设备已判定连不上（CJ_DEVICE_LOST=1，见 run_step）时也不再去放：锁带超时，到点自己放，免得收尾再白等一次超时。
CJ_AWAKE_SECS="${CJ_AWAKE_SECS:-1200}"
CJ_AWAKE_MINE=0
CJ_AWAKE_LOCK_PATH=/sys/power/wake_lock
CJ_AWAKE_UNLOCK_PATH=/sys/power/wake_unlock
awake_wanted() { [ -z "${CJ_AWAKE_HELD:-}" ] && [ "${CJ_AWAKE:-1}" != 0 ]; }
awake_lock_cmd() { echo "[ -w $CJ_AWAKE_LOCK_PATH ] && echo cangjie-deploy ${CJ_AWAKE_SECS}000000000 > $CJ_AWAKE_LOCK_PATH 2>/dev/null; exit 0"; }
awake_mark_held() {
    CJ_AWAKE_MINE=1
    cj_mktemp || return 0
    CJ_AWAKE_REBOOTED_FILE=$CJ_TMP
    export CJ_AWAKE_HELD=1 CJ_AWAKE_REBOOTED_FILE
}
device_awake_hold() {
    awake_wanted || return 0
    rssh "$(awake_lock_cmd)" >/dev/null 2>&1 || return 0
    awake_mark_held
}
# awake_release_arg：给"最后一个设备端脚本"的参数——本进程或上层编排者持着锁时是放锁接口路径，否则 "-"
awake_release_arg() {
    if [ -n "${CJ_AWAKE_HELD:-}" ] && [ -n "${CJ_AWAKE_REBOOTED_FILE:-}" ]; then echo "$CJ_AWAKE_UNLOCK_PATH"; else echo -; fi
}
device_awake_release() {
    [ "$CJ_AWAKE_MINE" = 1 ] || return 0
    CJ_AWAKE_MINE=0
    if [ "${CJ_DEVICE_LOST:-0}" = 1 ]; then
        echo "-- 设备连不上，不去放部署唤醒锁（它带 ${CJ_AWAKE_SECS} 秒超时，到点自动释放）"
    elif [ ! -s "$CJ_AWAKE_REBOOTED_FILE" ]; then
        rssh "[ -w $CJ_AWAKE_UNLOCK_PATH ] && echo cangjie-deploy > $CJ_AWAKE_UNLOCK_PATH 2>/dev/null; exit 0" >/dev/null 2>&1 || true
    fi
    rm -f "$CJ_AWAKE_REBOOTED_FILE"
    unset CJ_AWAKE_HELD CJ_AWAKE_REBOOTED_FILE
}

# ── 推送 + 校验 + 安装（2026-10-10 起比对与安装同一次往返）──────────────────────────
# pv_set LOCAL REMOTE [LOCAL REMOTE …]：登记一批要推到设备"暂存路径"的文件（调用方保证不在 extensions.d 之类的自动加载
# 目录里），只在本机算一次 md5，不连设备。
# pv_exec CMD…：跑调用方的安装命令 CMD，CMD 送上设备的命令行必须以 "$CJ_PV_GUARD"（一段设备端 shell）开头——经
# dev_script/dev_pipe 送的自动带上（dev_pipe 会拼）；直接 rssh 的由 CMD 自己拼（如 rssh "${CJ_PV_GUARD}sh install.sh"）：
#   ① 第一次（probe）：守卫在设备上建好目录、逐个比对暂存文件的 md5——全一致（重复部署的常态）就接着装，一次往返完事；
#      有不一致/缺失的，守卫打印 CJ-PV-NEED <序号…> 后以 97 退出，**安装命令一条都没执行**；
#   ② 只 scp 那几个文件，再跑一次 CMD（strict）：守卫复核 md5，对不上的那个从设备上删掉、打印"md5 对不上"、以 1 退出，
#      安装照样一条都不执行——最终位置从未被碰过（与旧 push_verified 同一语义，H3）。
# 往返：没变化 1 次（旧版 2 次：建目录+取 md5、安装）；有变化 2 次 + scp（旧版 3 次：取 md5、复核、安装）。
# CMD 的退出码原样返回（97 只由守卫产生，且必须同时见到 CJ-PV-NEED 才当成"要上传"）。
# shellcheck disable=SC2034  # CJ_PV_GUARD 由调用方（pv_exec 的 CMD）读
PV_LOCALS=""; PV_REMS=""; PV_CHECKS=""; PV_DIRS=""; CJ_PV_GUARD=""
CJ_PV_LOG=""; CJ_PV_RCF=""
pv_set() {
    { [ $# -ge 2 ] && [ $(($# % 2)) -eq 0 ]; } || { echo "!! pv_set：参数必须成对（LOCAL REMOTE …）"; return 1; }
    PV_LOCALS=""; PV_REMS=""; PV_CHECKS=""; PV_DIRS=""; pv_n=0
    while [ $# -gt 0 ]; do
        [ -f "$1" ] || { echo "!! 本机缺要推送的文件：$1"; return 1; }
        pv_sum="$(md5_local "$1")" || return 1
        pv_n=$((pv_n + 1))
        PV_LOCALS="$PV_LOCALS$1$CJ_NL"; PV_REMS="$PV_REMS$2$CJ_NL"
        pv_d="$(shquote "$(dirname "$2")")"
        case " $PV_DIRS " in *" $pv_d "*) ;; *) PV_DIRS="$PV_DIRS $pv_d" ;; esac
        PV_CHECKS="$PV_CHECKS cj_pv_chk $pv_n $(shquote "$2") $pv_sum $(shquote "$(basename "$1")");"
        shift 2
    done
}
# pv_guard MODE [已上传的序号…]：生成设备端守卫（一行，以 "; " 结尾）。MODE=probe 只比对；strict 复核并删坏件
pv_guard() {
    pg_m=$1; shift
    printf '%s' "cj_pv_m=$pg_m; cj_pv_up=' $* '; cj_pv_need=; cj_pv_bad=0; "
    # shellcheck disable=SC2016  # 设备端代码，有意不在本机展开
    printf '%s' 'cj_pv_chk() { cj_pv_s=$(md5sum "$2" 2>/dev/null) || cj_pv_s=; cj_pv_s=${cj_pv_s%% *}; if [ "$cj_pv_s" = "$3" ]; then case "$cj_pv_up" in *" $1 "*) echo "-- md5 一致：$4" ;; *) if [ "$cj_pv_m" = probe ]; then echo "-- 未变，不重传：$4"; fi ;; esac; elif [ "$cj_pv_m" = probe ]; then cj_pv_need="$cj_pv_need $1"; else echo "!! md5 对不上：$4（本地 $3 vs 设备 ${cj_pv_s:-空}），传输可能损坏，不继续（已删设备上的暂存文件 $2，最终位置未动）"; rm -f "$2"; cj_pv_bad=1; fi; }; '
    printf '%s' "mkdir -p$PV_DIRS || exit 1;$PV_CHECKS "
    # shellcheck disable=SC2016
    printf '%s' 'if [ -n "$cj_pv_need" ]; then echo "CJ-PV-NEED$cj_pv_need"; exit 97; fi; [ "$cj_pv_bad" = 0 ] || exit 1; '
}
pv_exec() {
    [ -n "$PV_REMS" ] || { echo "!! pv_exec：先 pv_set"; return 1; }
    # 本机临时文件在当前 shell 建好并登记（下面的管道是子 shell，里面登记的会丢）；run_apply 的一对也一并预建
    if [ -z "$CJ_PV_LOG" ]; then cj_mktemp || return 1; CJ_PV_LOG=$CJ_TMP; cj_mktemp || return 1; CJ_PV_RCF=$CJ_TMP; fi
    ra_prepare || return 1
    CJ_PV_GUARD="$(pv_guard probe)"
    { pe_c=0; "$@" || pe_c=$?; echo "$pe_c" > "$CJ_PV_RCF"; } | tee "$CJ_PV_LOG"
    pe_rc="$(cat "$CJ_PV_RCF")"
    pe_need="$(sed -n 's/^CJ-PV-NEED//p' "$CJ_PV_LOG" | tail -n 1)"
    case "$pe_need" in *[!0-9\ ]*) pe_need="" ;; esac
    if [ "$pe_rc" != 97 ] || [ -z "$pe_need" ]; then CJ_PV_GUARD=""; return "$pe_rc"; fi
    for pe_k in $pe_need; do
        pe_l="$(printf '%s' "$PV_LOCALS" | sed -n "${pe_k}p")"; pe_r="$(printf '%s' "$PV_REMS" | sed -n "${pe_k}p")"
        [ -n "$pe_l" ] && [ -n "$pe_r" ] || { echo "!! 设备要求上传的序号 $pe_k 不在清单里"; CJ_PV_GUARD=""; return 1; }
        rscp "$pe_l" "root@$HOST:$pe_r" || { echo "!! scp $pe_l 失败"; CJ_PV_GUARD=""; return 1; }
    done
    # shellcheck disable=SC2086  # pe_need 是空格分隔的序号，有意按词展开
    CJ_PV_GUARD="$(pv_guard strict $pe_need)"
    pe_rc=0; "$@" || pe_rc=$?
    # shellcheck disable=SC2034
    CJ_PV_GUARD=""
    return "$pe_rc"
}

# push_tar_verified LOCAL_TAR REMOTE_DIR MUST_FILE：整包载荷（tar）经 ssh 标准输入送到设备，md5 对上、解包后 MUST_FILE
# （相对 REMOTE_DIR）存在，才把 REMOTE_DIR 换成新的；任一步不对就删掉半成品、返回 1，现成的 REMOTE_DIR 原样不动。
# 2026-10-10（审计 PK-4）：旧版 deploy.sh 只看解包后有没有 install.sh——传输中途被截断/损坏但 tar 恰好还能解开时，
# 缺文件、坏二进制会一路装下去；其余步骤都用 push_verified 逐个 md5，这里补上同一道校验。
# tar 先整个落盘（REMOTE_DIR.tar.new）再算 md5、再解：设备 busybox 没有进程替换，边收边算要 fifo，不值得；代价是
# /home 上临时多占一份载荷大小（约 20MB）。设备端 md5sum 与 pv_exec 的守卫同一个命令（busybox 自带）。
# 不重传（2026-10-10）：换位前把 tar 的 md5 记进 REMOTE_DIR/.payload-md5；下次同一份载荷（deploy.sh 打的 tar 是确定性的）
# 设备端先比对它、MUST_FILE 也在，就打印 CJ-PAYLOAD-SAME 直接退出 0、不读标准输入——重复跑 install-all 不再每次推 20MB、
# 不再在闪存上重写一遍载荷。CJ_FORCE_PUSH=1 跳过比对、照常重传。比对通过时 PT_SAME=1（调用方可据此提示）。
# shellcheck disable=SC2034  # PT_SAME 供调用方读
PT_SAME=0
push_tar_verified() {
    # 远端会对 REMOTE_DIR 与它的 .new/.tar.new 做 rm -rf：只许设备 /home/root 下的具体子目录
    case "$2" in /home/root/?*) ;; *) echo "!! push_tar_verified：拒绝 /home/root 之外的目标 $2"; return 1 ;; esac
    case "$2" in *..*) echo "!! push_tar_verified：目标不许含 ..：$2"; return 1 ;; esac
    PT_SAME=0
    pt_sum="$(md5_local "$1")" || return 1
    pt_d="$(shquote "$2")"; pt_n="$(shquote "$2.new")"; pt_t="$(shquote "$2.tar.new")"; pt_m="$(shquote "$2.new/$3")"
    pt_f="$(shquote "$2/.payload-md5")"; pt_dm="$(shquote "$2/$3")"
    pt_same=""
    if [ "${CJ_FORCE_PUSH:-0}" != 1 ]; then
        pt_same="if [ -f $pt_f ] && [ \"\$(cat $pt_f)\" = $pt_sum ] && [ -f $pt_dm ]; then echo CJ-PAYLOAD-SAME; exit 0; fi
"
    fi
    pt_rc=0
    pt_out="$(rssh_in "$pt_same""rm -rf $pt_n $pt_t && mkdir -p $pt_n || exit 1
if ! { cat > $pt_t && s=\$(md5sum $pt_t) && [ \"\${s%% *}\" = $pt_sum ] && tar -C $pt_n -xf $pt_t && [ -f $pt_m ]; }; then
    echo \"!! 载荷校验没过（md5 应为 $pt_sum，设备上收到 \${s%% *}；或解包后缺 $3）：传输可能截断/损坏，已删半成品，$2 未动\"
    rm -rf $pt_n $pt_t; exit 1
fi
echo $pt_sum > $pt_n/.payload-md5 && rm -f $pt_t && rm -rf $pt_d && mv $pt_n $pt_d" < "$1")" || pt_rc=$?
    if [ "$pt_out" = CJ-PAYLOAD-SAME ]; then
        # shellcheck disable=SC2034
        PT_SAME=1
        echo "-- 设备上的 $2 已是同一份载荷（md5 $pt_sum），不重传"
    elif [ -n "$pt_out" ]; then
        printf '%s\n' "$pt_out"
    fi
    return "$pt_rc"
}

# ── 固件安全门（install-all 用）──────────────────────────────────────────
# 白名单 = 仓库里的 firmware-allowlist.txt + 本机 firmware-allowlist.local.txt（--force 追加到后者，已 gitignore，
# 不再改动被 git 跟踪的文件，避免"未验证的哈希"被误提交；可用 CJ_ALLOWLIST_LOCAL 改路径）。
CJ_ALLOWLIST="${CJ_ALLOWLIST:-$CJ_PKG_DIR/firmware-allowlist.txt}"
CJ_ALLOWLIST_LOCAL="${CJ_ALLOWLIST_LOCAL:-$CJ_PKG_DIR/firmware-allowlist.local.txt}"
# fw_gate FORCE [SHA]：SHA 由 preflight_device 那次往返带回（2026-10-10）；不给就自己连一次取。
# 只认 64 位小写十六进制（2026-10-10）：旧版取 awk 的第一列，设备回了别的输出（报错、登录横幅）时会拿它当哈希，
# 加 --force 还会把这串垃圾写进本机白名单，之后任何设备都能"命中"。
fw_gate() { # $1=FORCE(0/1) [$2=SHA]
    echo "═══ 固件安全门（root@$HOST）═══"
    if [ $# -ge 2 ]; then fw_hash="$2"; else fw_hash="$(rssh 'sha256sum /usr/bin/xochitl' | awk '{print $1}')"; fi
    case "$fw_hash" in *[!0-9a-f]*) fw_hash="" ;; esac
    [ "${#fw_hash}" = 64 ] || fw_hash=""
    if [ -z "$fw_hash" ]; then
        echo "!! 没拿到 /usr/bin/xochitl 的 sha256（ssh 连不上、设备上没有这个文件，或设备回的不是 sha256？）"
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
# preflight_device FORCE（2026-10-10 起一次往返做完三件事，原先 fw_gate / 预检 / 拿唤醒锁各一次 ssh）：
#   设备端先打印 CJ-FW-SHA <sha256>，再做只读预检；预检全过且需要唤醒锁时顺手写锁并打印 CJ-AWAKE-HELD。
#   本机据 sha 过固件门（fw_gate），再打印预检输出。任一项不过返回 1（已拿的锁由调用方的 cj_cleanup 放掉）。
#   锁接口路径作为参数传（不写死在脚本体里），日志里看得到"这次往返碰了 wake_lock"。
preflight_device() { # $1=FORCE(0/1)
    pf_lock=-; pf_val=-
    if awake_wanted; then pf_lock="$CJ_AWAKE_LOCK_PATH"; pf_val="cangjie-deploy ${CJ_AWAKE_SECS}000000000"; fi
    cj_mktemp || return 1; pf_out=$CJ_TMP
    pf_rc=0
    dev_script "$CJ_MIN_FREE_KB" "$CJ_WARN_FREE_KB" /usr/bin/xochitl "$pf_lock" "$pf_val" > "$pf_out" 2>&1 <<'DEVICE_SCRIPT' || pf_rc=$?
set -eu
MINF="$1"; WARNF="$2"; XBIN="$3"; LOCK="$4"; LOCKV="$5"
FW="$(sha256sum "$XBIN" 2>/dev/null)" || FW=""
echo "CJ-FW-SHA ${FW%% *}"
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
if [ "$LOCK" != - ] && [ -w "$LOCK" ] && echo "$LOCKV" > "$LOCK" 2>/dev/null; then echo CJ-AWAKE-HELD; fi
exit 0
DEVICE_SCRIPT
    if grep -q '^CJ-AWAKE-HELD$' "$pf_out"; then awake_mark_held; fi
    pf_sha="$(sed -n 's/^CJ-FW-SHA //p' "$pf_out" | head -n 1)"
    if ! fw_gate "$1" "$pf_sha"; then
        # 没拿到哈希多半是连接/权限问题：把设备端原始输出带出来
        [ -n "$pf_sha" ] || grep -v -e '^CJ-FW-SHA' -e '^CJ-AWAKE-HELD$' "$pf_out" | sed 's/^/     /'
        return 1
    fi
    echo "═══ 设备预检（root@$HOST，只读）═══"
    grep -v -e '^CJ-FW-SHA' -e '^CJ-AWAKE-HELD$' "$pf_out" || true
    [ "$pf_rc" = 0 ]
}

DONE=""
FAILED=""
SKIPPED=""
NOTAPPL=""   # 步骤自己判定前置条件不满足、跳过（退出 0，但不该算进"已安装"）
NOTRUN=""    # 设备中途连不上之后没有执行的步骤（2026-10-10）
CJ_DEVICE_LOST=0
CJ_RS_SKIPF=""

# step_skipped REASON：步骤因前置条件不满足/不适用而跳过（非失败）时调用，调用后照常 exit 0。
# 单独跑时只打印原因；在 run_step 编排下（它导出 CJ_STEP_SKIP_FILE）另把原因写进该文件，
# 汇总时这一步列进"已跳过（前置条件不满足）"而不是"已安装"——不然"跳过"混在"已安装"里，容易漏看。
step_skipped() {
    echo "-- $1——跳过，非失败"
    if [ -n "${CJ_STEP_SKIP_FILE:-}" ]; then printf '%s\n' "$1" > "$CJ_STEP_SKIP_FILE"; fi
}

# run_step NAME CMD [ARGS…]：跳过判定 + 执行 + 记账（DRY=1 时只打印将执行的命令，不执行、不连设备）
# 断线即停（2026-10-10）：某步失败后再探一次连通；连不上就置 CJ_DEVICE_LOST=1，之后的步骤一律记"未执行"而不去连——
# 旧版每个剩下的步骤各等一次 ssh 超时、各报一次失败，汇总里分不清哪步是真坏了。各步骤本身可重复执行，重连后
# 重跑同一条命令即可从头补齐（已完成的步骤再跑一遍不重复写、不重复重启）。
run_step() {
    rs_name="$1"; shift
    if skip_has "$rs_name"; then
        echo; echo "-- 跳过 $rs_name（--skip）"
        SKIPPED="$SKIPPED $rs_name"
        return 0
    fi
    if [ "$CJ_DEVICE_LOST" = 1 ]; then
        echo; echo "-- 未执行 $rs_name（设备连不上）"
        NOTRUN="$NOTRUN $rs_name"
        return 0
    fi
    echo; echo "═══ $rs_name ═══"
    if [ "${DRY:-0}" = "1" ]; then
        echo "-- [dry-run] 将执行：$*"
        DONE="$DONE $rs_name"
        return 0
    fi
    # 跳过原因文件：整轮复用一个、经 cj_mktemp 登记（旧版每步 mktemp 一个，被 Ctrl-C 打断时留在 /tmp）
    if [ -z "$CJ_RS_SKIPF" ]; then cj_mktemp || return 1; CJ_RS_SKIPF=$CJ_TMP; fi
    : > "$CJ_RS_SKIPF"
    export CJ_STEP_SKIP_FILE="$CJ_RS_SKIPF"
    if "$@"; then
        if [ -s "$CJ_RS_SKIPF" ]; then
            NOTAPPL="$NOTAPPL
   $rs_name：$(head -n 1 "$CJ_RS_SKIPF")"
        else
            DONE="$DONE $rs_name"
        fi
    else
        echo "!! $rs_name 失败（见上面这一步的原始报错）"
        FAILED="$FAILED $rs_name"
        if ! rssh true >/dev/null 2>&1; then
            CJ_DEVICE_LOST=1
            echo "!! root@$HOST 已连不上（休眠/断线/重启？）——后面的步骤不再执行，免得每步各等一次超时"
        fi
    fi
    unset CJ_STEP_SKIP_FILE
}

# print_step_summary 动词：install-all / uninstall-all 收尾汇总的公共部分（动词=已安装/已卸载；DRY=1 时打印计划）
print_step_summary() {
    if [ "${DRY:-0}" = "1" ]; then echo "dry-run 计划（未连接设备、未执行）：${DONE:-（无）}"
    else echo "$1：${DONE:-（无）}"; fi
    [ -z "$SKIPPED" ] || echo "已跳过（--skip）：$SKIPPED"
    [ -z "$NOTAPPL" ] || echo "已跳过（前置条件不满足，非失败）：$NOTAPPL"
    [ -z "$FAILED" ] || echo "❌ 失败：$FAILED —— 看对应步骤上面的原始报错，不会自动重试"
    if [ -n "$NOTRUN" ]; then
        echo "⏸ 未执行（设备连不上）：$NOTRUN"
        echo "   设备连回来后重跑同一条命令即可：各步骤可重复执行，已完成的不会重复写、不会重复重启"
    fi
}
