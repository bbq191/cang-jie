# shellcheck shell=bash
# shellcheck disable=SC2154,SC2034  # R / PKG / REPO / STUBS / TMPBASE / CJ_SIM_LOG / CJ_SYSD 等由 run_sim_tests.sh（source 本文件者）设好；HOST/DRY/SKIP 由 lib.sh 的函数读
# ═══════════════════════════════════════════════════════════════════════════
# 主机端编排脚本（install-all / uninstall-all / lib.sh / deploy*.sh / verify-on-device / shelf/build.sh）的
# 中断、断线、回滚、临时文件、ssh 往返次数用例（2026-10-10 安装脚本加固）。
# 由 run_sim_tests.sh 末尾 source；沿用它的 new_sandbox / check / count_log / tree_sig 等。
#
# 本文件自带一层 ssh/scp 包装（$HW，排在 tests/stubs 之前），在原桩之上加三种"故障注入"：
#   CJ_SIM_KILL_ON=<ERE> CJ_SIM_KILL_WHO=<子串> CJ_SIM_KILL_SIG=<信号>：远端命令行第一次匹配 ERE 时，给命令行含
#       <子串> 的最近一个祖先进程发信号（模拟用户 Ctrl-C / 终端关闭 / kill），这次 ssh 本身照常执行；
#   CJ_SIM_LOSE_ON=<ERE>：远端命令行第一次匹配时，这次 ssh 以 255 失败、之后所有 ssh/scp 都失败（模拟设备断线/休眠）；
#   CJ_SIM_CAPTURE=<文件>：把送往 shelf-pkg.tar.new 的标准输入（书架载荷 tar）另存一份，供检查属主/权限/确定性。
# ═══════════════════════════════════════════════════════════════════════════

HW="$TMPBASE/hostwrap"; mkdir -p "$HW"
cat > "$HW/ssh" <<EOF
#!/bin/sh
REAL="$STUBS/ssh"
EOF
cat >> "$HW/ssh" <<'EOF'
FLAG="${CJ_SIM_ROOT:?}/.sim-lost"
if [ -e "$FLAG" ]; then
    echo "ssh(lost) $*" >> "$CJ_SIM_LOG"; echo "ssh: connect to host sim port 22: No route to host" >&2; exit 255
fi
if [ -n "${CJ_SIM_LOSE_ON:-}" ] && printf '%s' "$*" | grep -Eq -- "$CJ_SIM_LOSE_ON"; then
    : > "$FLAG"; echo "ssh(lost) $*" >> "$CJ_SIM_LOG"; echo "Connection to sim closed by remote host." >&2; exit 255
fi
if [ -n "${CJ_SIM_KILL_ON:-}" ] && [ ! -e "$CJ_SIM_ROOT/.sim-killed" ] && printf '%s' "$*" | grep -Eq -- "$CJ_SIM_KILL_ON"; then
    : > "$CJ_SIM_ROOT/.sim-killed"
    # 取命令行含 <子串> 的**最外层**祖先：脚本里的管道/命令替换会 fork 出同名子 shell，信号要发给脚本主进程
    p=$$; hit=""
    while [ "$p" -gt 1 ]; do
        p="$(sed 's/.*) [A-Za-z] \([0-9]*\) .*/\1/' "/proc/$p/stat" 2>/dev/null)" || break
        [ -n "$p" ] || break
        if tr '\0' ' ' < "/proc/$p/cmdline" 2>/dev/null | grep -q -- "$CJ_SIM_KILL_WHO"; then hit=$p; fi
    done
    if [ -n "$hit" ]; then
        # CJ_SIM_KILL_GROUP=1：发给它所在的整个进程组（终端 Ctrl-C / 挂断就是这样发的；用例用 setsid 把它放进独立进程组）
        if [ -n "${CJ_SIM_KILL_GROUP:-}" ]; then
            g="$(sed 's/.*) [A-Za-z] [0-9]* \([0-9]*\) .*/\1/' "/proc/$hit/stat")"; env kill -s "${CJ_SIM_KILL_SIG:-TERM}" -- "-$g"   # 用外部 kill：dash 内建 kill 既不认 `--` 也不认负数进程组号，CI 的 /bin/sh 是 dash
        else
            kill "-${CJ_SIM_KILL_SIG:-TERM}" "$hit"
        fi
    fi
fi
if [ -n "${CJ_SIM_CAPTURE:-}" ] && printf '%s' "$*" | grep -q 'shelf-pkg.tar.new'; then
    tee "$CJ_SIM_CAPTURE" | "$REAL" "$@"; exit $?
fi
exec "$REAL" "$@"
EOF
cat > "$HW/scp" <<EOF
#!/bin/sh
if [ -e "\${CJ_SIM_ROOT:?}/.sim-lost" ]; then echo "scp(lost) \$*" >> "\$CJ_SIM_LOG"; echo "ssh: connect to host sim port 22: No route to host" >&2; exit 255; fi
exec "$STUBS/scp" "\$@"
EOF
chmod +x "$HW/ssh" "$HW/scp"
hrun() { PATH="$HW:$STUBS:$PATH" "$@"; }
# 一轮 install-all 用到的公共环境（不编译、固件 --force 记进沙箱里的本机白名单、唤醒锁接口在沙箱里）
h_env() {
    export CJ_SKIP_BUILD=1 SHELF_NO_BUILD=1 CJ_ALLOWLIST_LOCAL="$R/allow.local.txt"
    mkdir -p "$R/sys/power" "$R/htmp"; : > "$R/sys/power/wake_lock"; : > "$R/sys/power/wake_unlock"
    rm -f "$R/.sim-lost" "$R/.sim-killed"
}
h_unenv() { unset CJ_SKIP_BUILD SHELF_NO_BUILD CJ_ALLOWLIST_LOCAL; }
h_tmp_empty() { test -z "$(ls -A "$R/htmp" 2>/dev/null)"; }
IA_SKIP="--skip chrony-cn,timezone-cn"

# ═══════════════════════════ H. 主机端编排加固（2026-10-10）═══════════════════════════
section "主机端：lib.sh 被 source 不改调用方的 shell 选项 / trap / 环境"
( cd "$PKG" || exit 1
  before="$(set +o; trap -p; env | grep -v '^_=' | sort)"
  # shellcheck disable=SC1091
  . ./lib.sh
  after="$(set +o; trap -p; env | grep -v '^_=' | sort)"
  [ "$before" = "$after" ] ) >/dev/null 2>&1
check "source lib.sh：shell 选项、trap、导出的环境变量都不变" test $? -eq 0
( cd "$PKG" || exit 1
  # shellcheck disable=SC1091
  . ./lib.sh; HOST=127.0.0.1; DRY=0; SKIP=""
  run_step demo true >/dev/null
  [ -z "${CJ_STEP_SKIP_FILE+x}" ] && [ -z "$(env | grep '^CJ_STEP_SKIP_FILE=')" ] ) >/dev/null 2>&1
check "run_step 跑完不留下导出的 CJ_STEP_SKIP_FILE" test $? -eq 0

section "主机端：设备端片段的 umask 不随会话"
new_sandbox
( cd "$PKG" && umask 077 && . ./lib.sh && HOST=127.0.0.1 && PATH="$STUBS:$PATH" dev_script /home/root/umask-probe <<'DEVICE_SCRIPT'
: > "$1"; mkdir "$1.d"
DEVICE_SCRIPT
) >/dev/null 2>&1
check "dev_script：会话 umask 077 时，设备端新建的文件 644、目录 755（组内先 umask 022）" test "$(stat -c %a "$R/home/root/umask-probe")" = 644 -a "$(stat -c %a "$R/home/root/umask-probe.d")" = 755

section "主机端：中途被信号打断 → 退出码对应信号、本机临时文件全清、不往下执行、可重跑"
new_sandbox; h_env; xovi_live on
( cd "$PKG" && CJ_SIM_KILL_ON="sh '[^']*/hl-snap/deploy/install\\.sh'" CJ_SIM_KILL_WHO='install-all.sh' CJ_SIM_KILL_SIG=TERM TMPDIR="$R/htmp" \
    hrun sh install-all.sh 127.0.0.1 --force $IA_SKIP ) >"$R/out.txt" 2>&1; rc=$?
check "install-all 收到 TERM（hl-snap 步骤进行中）→ 退出 143" test "$rc" -eq 143
check "  └ 本机 TMPDIR 里不留任何临时文件（步骤跳过标记、唤醒锁状态文件等）" h_tmp_empty
check "  └ 当前步骤跑完就停：没继续装 shelf、没整机重启" test ! -e "$R/home/root/shelf-pkg" -a "$(count_log 'systemctl reboot')" = 0
check "  └ 唤醒锁照样放掉（没重启）" test "$(count_log 'wake_unlock')" = 1
rm -f "$R/.sim-killed"
( cd "$PKG" && TMPDIR="$R/htmp" hrun sh install-all.sh 127.0.0.1 $IA_SKIP ) >"$R/out2.txt" 2>&1; rc=$?
check "  └ 再跑一遍 install-all：退出 0，shelf 装上、最后整机重启一次" test "$rc" -eq 0 -a -x "$R/home/root/.local/bin/gateway" -a "$(count_log 'systemctl reboot')" = 1
SIG_RECOVERED="$(tree_sig | grep -v '^F ./home/root/xovi/start ')"   # xovi/start 桩里写着各沙箱自己的日志路径，不比
new_sandbox; h_env; xovi_live on
( cd "$PKG" && hrun sh install-all.sh 127.0.0.1 --force $IA_SKIP ) >/dev/null 2>&1
sig_eq "  └ 中断后重跑的结果 = 一次不中断装完的结果（文件树一致）" "$(tree_sig | grep -v '^F ./home/root/xovi/start ')" "$SIG_RECOVERED"

new_sandbox; h_env; xovi_live on
( cd "$PKG" && CJ_SIM_KILL_ON="sh '[^']*/hl-snap/deploy/install\\.sh'" CJ_SIM_KILL_WHO='deploy-xovi-ext.sh' CJ_SIM_KILL_SIG=INT CJ_SIM_KILL_GROUP=1 TMPDIR="$R/htmp" \
    hrun setsid -w sh deploy-hl-snap.sh 127.0.0.1 ) >"$R/out.txt" 2>&1; rc=$?
check "单独跑 deploy-hl-snap 被 Ctrl-C（设备端安装进行中）→ 退出 130、本机临时文件（run_apply 的输出/退出码文件）全清" test "$rc" -eq 130 && h_tmp_empty
new_sandbox; h_env
( cd "$PKG" && CJ_SIM_KILL_ON='shelf-pkg\.tar\.new' CJ_SIM_KILL_WHO='deploy.sh' CJ_SIM_KILL_SIG=HUP CJ_SIM_KILL_GROUP=1 TMPDIR="$R/htmp" \
    hrun setsid -w sh deploy.sh 127.0.0.1 ) >"$R/out.txt" 2>&1; rc=$?
check "单独跑 deploy.sh 推送途中终端被关（HUP）→ 退出 129、本机载荷暂存目录全清" test "$rc" -eq 129 && h_tmp_empty
new_sandbox; h_env
( cd "$PKG" && CJ_SIM_KILL_ON=' sh -s' CJ_SIM_KILL_WHO='verify-on-device.sh' CJ_SIM_KILL_SIG=TERM TMPDIR="$R/htmp" \
    hrun sh verify-on-device.sh 127.0.0.1 ) >"$R/out.txt" 2>&1; rc=$?
check "verify-on-device 采集中被 TERM → 退出 143、本机临时目录清掉（旧版只挂了 EXIT，bash/dash 收到 TERM 不跑 EXIT trap）" test "$rc" -eq 143 && h_tmp_empty
h_unenv

section "主机端：设备中途断线 → 停在可重跑的状态，不再逐步白等超时"
new_sandbox; h_env; xovi_live on
( cd "$PKG" && CJ_SIM_LOSE_ON="sh '[^']*/hl-snap/deploy/install\\.sh'" hrun sh install-all.sh 127.0.0.1 --force $IA_SKIP ) >"$R/out.txt" 2>&1; rc=$?
after_loss="$(sed -n '/^ssh(lost)/,$p' "$CJ_SIM_LOG" | grep -c '^s')"
check "install-all：hl-snap 安装时断线 → 退出非 0" test "$rc" -ne 0
check "  └ 断线后只再探一次连通（旧版其余每步各等一次超时），不去放唤醒锁（带超时，自己会到期）" test "$after_loss" = 2
check "  └ 汇总列出没执行的步骤，并提示重连后重跑同一条命令" bash -c "grep -q '未执行（设备连不上）' '$R/out.txt' && grep -q 'ui-font' '$R/out.txt' && grep -q '重跑' '$R/out.txt'"
check "  └ 没整机重启、xochitl 没被动" test "$(count_log 'systemctl reboot')" = 0 -a -z "$(grep -E 'systemctl (stop|start|restart) xochitl' "$CJ_SIM_LOG")"
rm -f "$R/.sim-lost"; : > "$CJ_SIM_LOG"
( cd "$PKG" && hrun sh install-all.sh 127.0.0.1 $IA_SKIP ) >"$R/out2.txt" 2>&1; rc=$?
check "  └ 重连后重跑：退出 0、hl-snap/ui-font/shelf 都装上、整机重启一次" test "$rc" -eq 0 -a -f "$R/home/root/xovi/extensions.d/ui-font.so" -a -x "$R/home/root/.local/bin/gateway" -a "$(count_log 'systemctl reboot')" = 1
h_env
( cd "$PKG" && CJ_SIM_LOSE_ON='hl-snap' hrun sh uninstall-all.sh 127.0.0.1 ) >"$R/out.txt" 2>&1; rc=$?
after_loss="$(sed -n '/^ssh(lost)/,$p' "$CJ_SIM_LOG" | grep -c '^s')"
check "uninstall-all：中途断线 → 退出非 0，只再探一次连通，其余步骤列为未执行" test "$rc" -ne 0 -a "$after_loss" = 2 && grep -q '未执行（设备连不上）' "$R/out.txt"
rm -f "$R/.sim-lost"
( cd "$PKG" && hrun sh uninstall-all.sh 127.0.0.1 ) >/dev/null 2>&1; rc=$?
check "  └ 重连后重跑 uninstall-all：退出 0、卸干净" test "$rc" -eq 0 -a ! -e "$R/home/root/.local/bin/gateway" -a ! -e "$R/home/root/xovi/extensions.d/hl-snap.so"
h_unenv

section "主机端：有步骤失败时不整机重启（落盘的改动留着待生效，修好后重跑即可）"
new_sandbox; h_env; xovi_live on
( cd "$PKG" && CJ_SIM_SCP_CORRUPT=ui-font.so hrun sh install-all.sh 127.0.0.1 --force $IA_SKIP ) >"$R/out.txt" 2>&1; rc=$?
check "ui-font 传输损坏 → install-all 退出非 0、最后一步不整机重启" test "$rc" -ne 0 -a "$(count_log 'systemctl reboot')" = 0
check "  └ 汇总说明为什么没生效、怎么补（重跑 install-all 或 deploy-xovi-apply.sh）" bash -c "grep -q '前面有步骤失败' '$R/out.txt' && grep -q 'deploy-xovi-apply.sh' '$R/out.txt'"
check "  └ 已落盘的 hl-snap 仍记为待生效（待换入区/标记还在）" test -n "$(ls -A "$CJ_PENDING_DIR" "$R/home/root/.cangjie-stage/so-pending" 2>/dev/null)"
: > "$CJ_SIM_LOG"
( cd "$PKG" && hrun sh install-all.sh 127.0.0.1 $IA_SKIP ) >/dev/null 2>&1; rc=$?
check "  └ 修好后重跑：退出 0、ui-font 装上、整机重启一次" test "$rc" -eq 0 -a -f "$R/home/root/xovi/extensions.d/ui-font.so" -a "$(count_log 'systemctl reboot')" = 1
h_unenv

section "主机端：固件门只认 64 位十六进制的 sha256"
new_sandbox; h_env
mkdir -p "$R/fakebin"; printf '#!/bin/sh\necho "garbage-output"\n' > "$R/fakebin/sha256sum"; chmod +x "$R/fakebin/sha256sum"
( cd "$PKG" && PATH="$R/fakebin:$HW:$STUBS:$PATH" sh install-all.sh 127.0.0.1 --force $IA_SKIP ) >"$R/out.txt" 2>&1; rc=$?
check "设备返回的不是 sha256（垃圾输出）+ --force → 仍拒装，且不把垃圾写进本机白名单" test "$rc" -ne 0 -a ! -s "$R/allow.local.txt" -a -z "$(ls -A "$R/home/root/xovi/extensions.d")"
h_unenv

section "主机端：rootfs rw 窗口（deploy-usr-unit）任何失败路径都恢复 ro"
new_sandbox; chmod 555 "$CJ_SYSD"; : > "$CJ_SIM_LOG"
( cd "$PKG" && hrun sh deploy-wifi-watch.sh 127.0.0.1 ) >"$R/out.txt" 2>&1; rc=$?
chmod 755 "$CJ_SYSD"
check "写 /usr 失败（rw 窗口内 cp 出错）→ 退出非 0、最后一次 mount 是 ro" test "$rc" -ne 0 -a "$(count_log 'remount,rw')" = 1 -a "$(last_mount)" = "mount -o remount,ro /"
new_sandbox; : > "$CJ_SIM_LOG"
mkdir -p "$R/killbin"; cat > "$R/killbin/chmod" <<'EOF'
#!/bin/sh
# rw 窗口里 chmod 单元暂存文件时，模拟 ssh 断开：给远端的 sh -s 发 HUP
case "$*" in *.wifi-watch.service.new*)
    p=$$
    while [ "$p" -gt 1 ]; do
        p="$(sed 's/.*) [A-Za-z] \([0-9]*\) .*/\1/' "/proc/$p/stat" 2>/dev/null)" || break
        if tr '\0' ' ' < "/proc/$p/cmdline" 2>/dev/null | grep -q '^sh -s'; then kill -HUP "$p"; break; fi
    done ;;
esac
exec /usr/bin/chmod "$@"
EOF
chmod +x "$R/killbin/chmod"
( cd "$PKG" && PATH="$R/killbin:$HW:$STUBS:$PATH" sh deploy-wifi-watch.sh 127.0.0.1 ) >"$R/out.txt" 2>&1; rc=$?
check "rw 窗口里 ssh 断开（远端 shell 收到 HUP）→ 退出非 0、最后一次 mount 是 ro" test "$rc" -ne 0 -a "$(last_mount)" = "mount -o remount,ro /"
: > "$CJ_SIM_LOG"; ( cd "$PKG" && hrun sh deploy-wifi-watch.sh 127.0.0.1 ) >/dev/null 2>&1; rc=$?
check "  └ 重跑：退出 0、单元与链接就位、以 ro 收尾" test "$rc" -eq 0 -a -L "$CJ_SYSD/multi-user.target.wants/wifi-watch.service" -a "$(last_mount)" = "mount -o remount,ro /"

section "主机端：书架载荷 tar 的属主/权限与调用者 umask 无关、内容没变不重传"
new_sandbox; h_env
( cd "$PKG" && umask 077 && CJ_SIM_CAPTURE="$R/cap.tar" hrun sh deploy.sh 127.0.0.1 --only book ) >"$R/out.txt" 2>&1; rc=$?
check "umask 077 下部署：退出 0" test "$rc" -eq 0
check "  └ 载荷里所有条目属主 0/0（设备上 root 解包不会把文件留给开发机的 uid）" test -s "$R/cap.tar" -a -z "$(tar -tv --numeric-owner -f "$R/cap.tar" | awk '$2 != "0/0"')"
check "  └ 权限不随调用者 umask：目录/可执行 755、其余 644" test -z "$(tar -tvf "$R/cap.tar" | awk '$1 !~ /^(drwxr-xr-x|-rwxr-xr-x|-rw-r--r--)$/')"
ino1="$(stat -c %i "$R/home/root/shelf-pkg")"; : > "$CJ_SIM_LOG"
( cd "$PKG" && CJ_SIM_CAPTURE="$R/cap2.tar" hrun sh deploy.sh 127.0.0.1 --only book ) >"$R/out.txt" 2>&1; rc=$?
check "  └ 再部署一次（载荷没变）：退出 0、不再重传、设备上的 shelf-pkg 没被换掉，install.sh 照常跑" test "$rc" -eq 0 -a "$ino1" = "$(stat -c %i "$R/home/root/shelf-pkg")" -a -n "$(grep '不重传' "$R/out.txt")" -a "$(count_log 'gateway')" -ge 0 && grep -q 'shelf/install.sh' "$CJ_SIM_LOG"
( cd "$PKG" && SHELF_FORCE_PUSH=1 hrun sh deploy.sh 127.0.0.1 --only book ) >/dev/null 2>&1; rc=$?
check "  └ SHELF_FORCE_PUSH=1：强制重传（换成新推的载荷）" test "$rc" -eq 0 -a "$ino1" != "$(stat -c %i "$R/home/root/shelf-pkg")"
: > "$CJ_SIM_LOG"
( cd "$PKG" && CJ_DEVICE_OK=127.0.0.1 CJ_AWAKE_HELD=1 hrun sh deploy.sh 127.0.0.1 --only book --password 'p w;"x' ) >"$R/out.txt" 2>&1; rc=$?
check "被编排时（已连通、已持锁）带 --password 重跑：2 次 ssh（载荷比对 + 写密码并安装同一次连接），密码到达、不留 .pw" test "$rc" -eq 0 -a "$(count_log '^ssh')" = 2 -a ! -e "$R/home/root/shelf-pkg/.pw" && grep -Fq 'gateway passwd p w;"x' "$CJ_SIM_LOG"
echo junk > "$R/home/root/shelf-pkg.tar.new"
( cd "$PKG" && hrun sh uninstall-all.sh 127.0.0.1 ) >/dev/null 2>&1; rc=$?
check "uninstall-all：推送被打断留下的 shelf-pkg.tar.new 一并清掉" test "$rc" -eq 0 -a ! -e "$R/home/root/shelf-pkg.tar.new"
h_unenv

section "主机端：ssh 往返次数（每次往返都会唤醒设备）"
new_sandbox; h_env; xovi_live on; : > "$CJ_SIM_LOG"
( cd "$PKG" && hrun sh install-all.sh 127.0.0.1 --force $IA_SKIP ) >/dev/null 2>&1; rc=$?
n1="$(count_log '^ssh')"; s1="$(count_log '^scp')"
: > "$CJ_SIM_LOG"
( cd "$PKG" && hrun sh install-all.sh 127.0.0.1 $IA_SKIP ) >/dev/null 2>&1; rc2=$?
n2="$(count_log '^ssh')"; s2="$(count_log '^scp')"
: > "$CJ_SIM_LOG"
( cd "$PKG" && hrun sh uninstall-all.sh 127.0.0.1 ) >/dev/null 2>&1; rc3=$?
n3="$(count_log '^ssh')"
echo "       （install-all 首装 ssh=$n1 scp=$s1；重跑 ssh=$n2 scp=$s2；uninstall-all ssh=$n3；加固前依次 24/12、20/0、13）"
check "install-all 首装：17 次 ssh（加固前 24）" test "$rc" -eq 0 -a "$n1" = 17
check "install-all 重跑（无变化）：12 次 ssh、不传任何文件（加固前 20，且每次重推 20MB 书架载荷）" test "$rc2" -eq 0 -a "$n2" = 12 -a "$s2" = 0
check "uninstall-all：11 次 ssh（加固前 13）" test "$rc3" -eq 0 -a "$n3" = 11
new_sandbox; : > "$CJ_SIM_LOG"
( cd "$PKG" && CJ_SKIP_BUILD=1 DEFER_XOVI_START=1 hrun sh deploy-hl-snap.sh 127.0.0.1 ) >"$R/out.txt" 2>&1; rc=$?
check "单独 deploy-hl-snap 首装：3 次 ssh（连通/比对/复核并安装）+ 4 次 scp" test "$rc" -eq 0 -a "$(count_log '^ssh')" = 3 -a "$(count_log '^scp')" = 4
: > "$CJ_SIM_LOG"
( cd "$PKG" && CJ_SKIP_BUILD=1 DEFER_XOVI_START=1 hrun sh deploy-hl-snap.sh 127.0.0.1 ) >"$R/out.txt" 2>&1; rc=$?
check "  └ 再跑（载荷没变）：2 次 ssh（连通/比对并安装）、不 scp、4 行'未变，不重传'" test "$rc" -eq 0 -a "$(count_log '^ssh')" = 2 -a "$(count_log '^scp')" = 0 -a "$(grep -c '未变，不重传' "$R/out.txt")" = 4
new_sandbox; : > "$CJ_SIM_LOG"
( cd "$PKG" && CJ_SIM_SCP_CORRUPT=hl-snap.so CJ_SKIP_BUILD=1 DEFER_XOVI_START=1 hrun sh deploy-hl-snap.sh 127.0.0.1 ) >"$R/out.txt" 2>&1; rc=$?
check "  └ 传输损坏：退出非 0、删掉坏的暂存件、安装脚本没跑（extensions.d 与待换入区都是空的）" test "$rc" -ne 0 -a ! -e "$R/home/root/hl-snap/hl-snap.so" -a -z "$(ls -A "$R/home/root/xovi/extensions.d")" -a ! -e "$R/home/root/.cangjie-stage/so-pending/hl-snap.so" && grep -q 'md5 对不上：hl-snap.so' "$R/out.txt"
new_sandbox; : > "$CJ_SIM_LOG"
( cd "$PKG" && hrun sh deploy-wifi-watch.sh 127.0.0.1 ) >/dev/null 2>&1; rc=$?
check "单独 deploy-wifi-watch 首装：3 次 ssh（加固前 4）" test "$rc" -eq 0 -a "$(count_log '^ssh')" = 3
h_unenv

section "主机端：shelf/build.sh"
mkdir -p "$TMPBASE/cargobin"; printf '#!/bin/sh\necho "cargo $*" >> "%s/cargo.log"\n' "$TMPBASE" > "$TMPBASE/cargobin/cargo"; chmod +x "$TMPBASE/cargobin/cargo"
: > "$TMPBASE/cargo.log"
PATH="$TMPBASE/cargobin:$PATH" sh "$REPO/shelf/build.sh" >"$TMPBASE/build.out" 2>&1; rc=$?
check "build.sh：不再做用不上的 host release 构建（只 host 测试 + 交叉编译），cargo 一律 --locked" test "$rc" -eq 0 -a -z "$(grep -v -- '--target' "$TMPBASE/cargo.log" | grep 'cargo build')" -a -z "$(grep -v -- '--locked' "$TMPBASE/cargo.log")"
check "build.sh：set -u（未定义变量当场报错而不是悄悄当空串）" grep -q '^set -eu' "$REPO/shelf/build.sh"
