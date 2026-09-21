#!/bin/bash
# ═══════════════════════════════════════════════════════════════════════════
# 安装/卸载/部署脚本的本机模拟测试（不碰真机；2026-09-20 脚本审计后新增）。
#
# 做法：PATH 里放 tests/stubs/ 下的假 ssh/scp/systemctl/mount/dmsetup/id/sleep/curl/journalctl/rcc/python3，
# 临时目录当"设备"（$CJ_SIM_ROOT：home/root、usr/lib/systemd/system、proc/…）。假 ssh 把远端命令直接在本机沙箱里
# 执行，设备端脚本（devlib.sh / shelf/install.sh / 各 heredoc 脚本）因此跑的是**真代码**，只是 rootfs/systemd/
# mount 被桩住并写日志（$CJ_SIM_LOG），可断言"有没有 remount rw、最后一次 mount 是不是 ro、有没有 xovi/start"。
#
# 覆盖：devlib 各函数 · shelf install 幂等/缺载荷不留半成品/rw 窗口失败恢复 ro/只重启有变化的服务 ·
#       shelf uninstall 与 install 清单对称（含 mkdir-agent qmd、lo-alias.sh、shelf-uninstall、旧命名遗留、--only、--purge）·
#       deploy.sh（密码含特殊字符、shelf-pkg 换位）· hl-snap 部署（原子落位/备份不进 extensions.d/DEFER）·
#       xovi-apply 与 sidebar-entry 的 H1 判定（xovi 已生效 → restart，绝不 xovi/start）· appload 补丁的暂存+md5 ·
#       install-all → uninstall-all 整轮对称 · 静态守卫（remount,rw / xovi/start 只许出现在库里）。
# 一键：sh packaging/tests/run_sim_tests.sh     （也由 pytest 的 test_install_scripts_sim.py 调用，CI 会跑）
# ═══════════════════════════════════════════════════════════════════════════
set -u
# 保险：如果真是 root 在跑，拒绝（stub 的 id 会骗过脚本，而脚本里有 remount/rm 等真命令走假 PATH，但保守起见不冒险）
if [ "$(/usr/bin/id -u)" = "0" ] && [ -z "${CJ_SIM_ALLOW_ROOT:-}" ]; then
    echo "拒绝以 root 跑模拟测试（设 CJ_SIM_ALLOW_ROOT=1 强行跑）"; exit 2
fi
REAL_HOME="$HOME"
# 所有 XDG_* 一律指进沙箱（脚本会读它们；不设的话 install/uninstall --purge 会碰到真实用户目录——
# 2026-09-20 首版就踩过：--purge 测试清了开发机的 ~/.config/shelf 等，见 new_sandbox 里的导出与末尾的"真实 HOME 未被触碰"守卫）
guard_paths() { echo "$REAL_HOME/.config/shelf $REAL_HOME/.local/share/shelf $REAL_HOME/.local/state/shelf $REAL_HOME/.local/lib/shelf $REAL_HOME/cangjie-backups $REAL_HOME/.local/bin/shelf-uninstall $REAL_HOME/.cangjie-stage"; }
GUARD_BEFORE=""
for g in $(guard_paths); do [ -e "$g" ] && GUARD_BEFORE="$GUARD_BEFORE $g"; done
HERE="$(cd "$(dirname "$0")" && pwd)"
PKG="$(cd "$HERE/.." && pwd)"
REPO="$(cd "$PKG/.." && pwd)"
STUBS="$HERE/stubs"
TMPBASE="$(mktemp -d)"
CREATED_FILES="$TMPBASE/created-files.txt"; : > "$CREATED_FILES"
cleanup() {
    while read -r f; do rm -f "$f"; rmdir -p "$(dirname "$f")" 2>/dev/null; done < "$CREATED_FILES"
    # 还原被挪走的真交叉编译产物（见 mk_dummy_targets）
    if [ -f "$TMPBASE/real-targets.txt" ]; then
        while read -r f; do mkdir -p "$(dirname "$REPO/$f")"; mv "$TMPBASE/real-targets/$f" "$REPO/$f"; done < "$TMPBASE/real-targets.txt"
    fi
    rm -rf "$TMPBASE"
}
trap cleanup EXIT

PASS=0; FAIL=0
ok()   { PASS=$((PASS + 1)); echo "  ok   $1"; }
bad()  { FAIL=$((FAIL + 1)); echo "  FAIL $1"; }
check() { # 描述 命令…（命令成功=通过）
    d="$1"; shift
    if "$@" >/dev/null 2>&1; then ok "$d"; else bad "$d"; fi
}
sig_eq() { # 描述 期望sig 实际sig：不等时打印 diff（便于定位）
    if [ "$2" = "$3" ]; then ok "$1"; else bad "$1"; diff <(printf '%s\n' "$2") <(printf '%s\n' "$3") | head -20 | sed 's/^/       /'; fi
}
section() { echo; echo "── $1"; }

# ── 沙箱 ──
new_sandbox() {
    R="$(mktemp -d "$TMPBASE/sim.XXXXXX")"
    export CJ_SIM_ROOT="$R" CJ_SIM_LOG="$R/log.txt"
    mkdir -p "$R/home/root/xovi/extensions.d" "$R/home/root/xovi/exthome/qt-resource-rebuilder" "$R/home/root/xovi/exthome/appload" \
             "$R/home/root/.local/bin" "$R/usr/lib/systemd/system" "$R/proc/4242" "$R/usr/bin" "$R/tmp/shelf-0/shelf/services"
    : > "$CJ_SIM_LOG"
    B="$R/home/root/.local/bin"; Q="$R/home/root/xovi/exthome/qt-resource-rebuilder"
    : > "$R/home/root/xovi/xovi.so"
    printf '#!/bin/sh\necho XOVI_START >> "%s"\n' "$CJ_SIM_LOG" > "$R/home/root/xovi/start"; chmod +x "$R/home/root/xovi/start"
    echo "fake xochitl $$" > "$R/usr/bin/xochitl"
    echo '{}' > "$R/tmp/shelf-0/shelf/services/book.json"
    xovi_live off
    export HOME="$R/home/root" XDG_CONFIG_HOME="$R/home/root/.config" XDG_DATA_HOME="$R/home/root/.local/share" XDG_STATE_HOME="$R/home/root/.local/state" SHELF_REG_DIR="$R/tmp/shelf-0/shelf/services" CJ_SYSD="$R/usr/lib/systemd/system" CJ_PROC="$R/proc"
    export CJ_APPLY_GRACE=0 CJ_HEALTH_SLEEP=0 CJ_RETRY_SLEEP=0
    unset CJ_SIM_VERITY CJ_SIM_RW_FAIL CJ_SIM_INACTIVE CJ_SIM_SCP_CORRUPT CJ_HOME CJ_XOVI CJ_BACKUP_DIR CJ_BACKUP_KEEP CJ_STAGE_DIR
}
# xovi_live on|off：模拟 xochitl 进程里有/没有 LD_PRELOAD=xovi.so，及 maps 里有无扩展
xovi_live() {
    if [ "$1" = on ]; then
        printf 'FOO=1\0LD_PRELOAD=/home/root/xovi/xovi.so\0XOVI_ROOT=/x\0' > "$R/proc/4242/environ"
        printf '7f00 r-xp /home/root/xovi/xovi.so\n7f01 r-xp /home/root/xovi/extensions.d/hl-snap.so\n7f02 r-xp hw-stroke.so\n' > "$R/proc/4242/maps"
    else
        printf 'FOO=1\0' > "$R/proc/4242/environ"
        printf '7f00 r-xp /home/root/xovi/xovi.so\n7f01 r-xp hl-snap.so\n7f02 r-xp hw-stroke.so\n' > "$R/proc/4242/maps"
    fi
}
run() { PATH="$STUBS:$PATH" "$@"; }
count_log() { grep -c -- "$1" "$CJ_SIM_LOG" 2>/dev/null; }
# 设备文件树签名：只看常规文件与符号链接（路径 + 内容 md5 / 链接目标），排除 proc/日志/备份/用户数据
tree_sig() {
    ( cd "$R" && find ./home ./usr \( -type f -o -type l \) 2>/dev/null | sort | grep -v -e '^./home/root/cangjie-backups/' | while read -r f; do
        if [ -L "$f" ]; then echo "L $f -> $(readlink "$f")"; else echo "F $f $(md5sum < "$f" | cut -c1-32)"; fi
      done )
}
last_mount() { grep '^mount ' "$CJ_SIM_LOG" | tail -n 1; }

# ── shelf 载荷 ──
SVCS="gateway book koreader font wallpaper ink transcribe mind note"
svc_of() { [ "$1" = gateway ] && echo gateway || echo "$1-serve"; }
mk_payload() { # DIR
    P="$1"; mkdir -p "$P/bin" "$P/systemd" "$P/lo-alias" "$P/xovi"
    for s in $SVCS; do
        b="$(svc_of "$s")"
        printf '#!/bin/sh\necho "%s $*" >> "%s"\n' "$b" "$CJ_SIM_LOG" > "$P/bin/$b"; chmod +x "$P/bin/$b"
        printf '[Unit]\nDescription=%s\n[Service]\nExecStart=/x/%s\n[Install]\nWantedBy=shelf.target\n' "$b" "$b" > "$P/systemd/$b.service"
    done
    printf '[Unit]\nDescription=t\n[Install]\nWantedBy=multi-user.target\n' > "$P/systemd/shelf.target"
    echo '#!/bin/sh' > "$P/lo-alias/lo-alias.sh"
    for q in font-menu-dynamic.qmd font-menu-dynamic-3.27.qmd shelf-trash-agent.qmd shelf-mkdir-agent.qmd shelf-comic-margins.qmd; do echo "qmd $q v1" > "$P/xovi/$q"; done
    cp "$REPO/shelf/install.sh" "$REPO/shelf/uninstall.sh" "$REPO/shelf/manifest.sh" "$PKG/devlib.sh" "$P/"
}

# ═══════════════════════════ 1. devlib 单元 ═══════════════════════════
section "devlib.sh：rw 窗口 / 原子替换 / 备份轮转 / xochitl 判定"
new_sandbox
# shellcheck disable=SC1091
. "$PKG/devlib.sh"
export PATH="$STUBS:$PATH"

: > "$CJ_SIM_LOG"; body_fail() { return 1; }; body_ok() { return 0; }
cj_with_rootfs_rw body_fail; rc=$?
check "with_rootfs_rw：body 失败 → 返回非 0" test "$rc" -ne 0
check "with_rootfs_rw：body 失败后最后一次 mount 是 remount,ro" test "$(last_mount)" = "mount -o remount,ro /"
: > "$CJ_SIM_LOG"; cj_with_rootfs_rw body_ok; rc=$?
check "with_rootfs_rw：body 成功 → 0 且最后 mount 是 ro" test "$rc" -eq 0 -a "$(last_mount)" = "mount -o remount,ro /"
: > "$CJ_SIM_LOG"; CJ_SIM_RW_FAIL=1 cj_with_rootfs_rw body_ok; rc=$?
check "with_rootfs_rw：remount rw 失败 → 非 0、不跑 body、无 ro 恢复调用" test "$rc" -ne 0 -a "$(count_log 'remount,ro')" = 0
# body 被信号杀死（模拟中途死亡）：仍恢复 ro，返回非 0
: > "$CJ_SIM_LOG"
body_kill() { kill -KILL "$BASHPID"; }
cj_with_rootfs_rw body_kill >/dev/null 2>&1; rc=$?
check "with_rootfs_rw：body 被 SIGKILL → 非 0，且仍恢复 ro" test "$rc" -ne 0 -a "$(last_mount)" = "mount -o remount,ro /"

# install_usr_unit：源缺失 → 不 remount；verity → 3 且不 remount；成功 → wants 链接；幂等
: > "$CJ_SIM_LOG"; cj_install_usr_unit x.service "$R/nope" multi-user.target.wants; rc=$?
check "install_usr_unit：源缺失 → 1，且从未 remount rw" test "$rc" -eq 1 -a "$(count_log 'remount,rw')" = 0
echo '[Unit]' > "$R/x.service.src"
: > "$CJ_SIM_LOG"; CJ_SIM_VERITY=1 cj_install_usr_unit x.service "$R/x.service.src" multi-user.target.wants; rc=$?
check "install_usr_unit：dm-verity 激活 → 3，不 remount" test "$rc" -eq 3 -a "$(count_log 'remount')" = 0
: > "$CJ_SIM_LOG"; cj_install_usr_unit x.service "$R/x.service.src" multi-user.target.wants >/dev/null; rc=$?
check "install_usr_unit：成功装单元+wants 链接" test "$rc" -eq 0 -a -f "$CJ_SYSD/x.service" -a -L "$CJ_SYSD/multi-user.target.wants/x.service"
: > "$CJ_SIM_LOG"; cj_install_usr_unit x.service "$R/x.service.src" multi-user.target.wants >/dev/null
check "install_usr_unit：再装一次 → 幂等，不 remount" test "$(count_log 'remount')" = 0
cj_remove_usr_unit x.service multi-user.target.wants >/dev/null
check "remove_usr_unit：单元与链接都删了" test ! -e "$CJ_SYSD/x.service" -a ! -L "$CJ_SYSD/multi-user.target.wants/x.service"
cj_install_usr_unit y.service "$R/x.service.src" - >/dev/null
check "install_usr_unit：wants='-' 不建开机链接" test -f "$CJ_SYSD/y.service" -a ! -e "$CJ_SYSD/multi-user.target.wants/y.service"

# safe_replace：内容、无残留、失败保留旧文件、相同内容不动
echo new > "$R/src.bin"; echo old > "$R/dst.bin"
cj_safe_replace "$R/src.bin" "$R/dst.bin" "$R/stg" 755
check "safe_replace：内容替换 + REPLACED=1" test "$(cat "$R/dst.bin")" = new -a "$CJ_REPLACED" = 1
check "safe_replace：暂存目录里无残留临时文件" test -z "$(ls -A "$R/stg")"
cj_safe_replace "$R/src.bin" "$R/dst.bin" "$R/stg" 755
check "safe_replace：内容相同 → REPLACED=0" test "$CJ_REPLACED" = 0
cj_safe_replace "$R/none" "$R/dst.bin" "$R/stg" 755 >/dev/null; rc=$?
check "safe_replace：源不存在 → 1，旧文件不变" test "$rc" -eq 1 -a "$(cat "$R/dst.bin")" = new

# 备份轮转：只删自己生成的严格时间戳命名、保留最近 N 份、不动手工命名/大文件
export CJ_BACKUP_DIR="$R/bk" CJ_BACKUP_KEEP=3 CJ_BACKUP_MAXBYTES=100
mkdir -p "$R/bk"
for i in 1 2 3 4 5; do echo "v$i" > "$R/bk/foo.so.bak.pre-2026010$i-000000"; done
echo manual > "$R/bk/foo.so.bak.pre-manual"
echo other > "$R/bk/bar.so.bak.pre-20260101-000000"
head -c 500 /dev/zero > "$R/bk/foo.so.bak.pre-20250101-000000"   # 最老，但超过 MAXBYTES → 不动
mkdir "$R/bk/keepdir"; echo u > "$R/bk/keepdir/userdata"
echo x > "$R/foo.so"; cj_backup_file "$R/foo.so" >/dev/null
left="$(find "$R/bk" -maxdepth 1 -name 'foo.so.bak.pre-????????-??????' | wc -l)"
check "轮转：foo.so 的时间戳备份只剩 KEEP=3 份 + 1 份超大不动" test "$left" -eq 4
check "轮转：手工命名/别的文件/其它目录/用户数据 全部保留" test -f "$R/bk/foo.so.bak.pre-manual" -a -f "$R/bk/bar.so.bak.pre-20260101-000000" -a -f "$R/bk/keepdir/userdata"
check "轮转：超过大小上限的旧备份不被删" test -f "$R/bk/foo.so.bak.pre-20250101-000000"
check "轮转：最老的小备份被删、最新的保留" test ! -f "$R/bk/foo.so.bak.pre-20260101-000000" -a -f "$R/bk/foo.so.bak.pre-20260105-000000"
# 目录型备份轮转：只删"里面全是常规文件"的
for i in 1 2 3 4 5; do mkdir "$R/bk/shelf-2026020$i-000000"; echo b > "$R/bk/shelf-2026020$i-000000/f"; done
mkdir "$R/bk/shelf-20260101-000000"; mkdir "$R/bk/shelf-20260101-000000/sub"   # 含子目录 → 不动
cj_bk_prune shelf- d
n="$(find "$R/bk" -maxdepth 1 -name 'shelf-????????-??????' | wc -l)"
check "目录轮转：保留 KEEP 份 + 含子目录的那份不动" test "$n" -eq 4 -a -d "$R/bk/shelf-20260101-000000/sub"
unset CJ_BACKUP_DIR CJ_BACKUP_KEEP CJ_BACKUP_MAXBYTES

# xochitl_apply 的 H1 判定
: > "$CJ_SIM_LOG"; xovi_live on; out="$(cj_xochitl_apply 2>&1)"
check "xochitl_apply：xovi 已生效 → systemctl restart xochitl，且绝不跑 xovi/start" test "$(count_log 'systemctl restart xochitl')" = 1 -a "$(count_log XOVI_START)" = 0
check "xochitl_apply：重启前打印\"打断阅读\"提示" test -n "$(printf '%s' "$out" | grep '打断')"
: > "$CJ_SIM_LOG"; xovi_live off; cj_xochitl_apply >/dev/null 2>&1
check "xochitl_apply：xovi 没生效 → 才跑 xovi/start" test "$(count_log XOVI_START)" = 1 -a "$(count_log 'systemctl restart xochitl')" = 0
rm "$HOME/xovi/start"; xovi_live off; cj_xochitl_apply >/dev/null 2>&1; rc=$?
check "xochitl_apply：没有 xovi/start 也没 xovi → 失败而不是乱重启" test "$rc" -ne 0
xovi_live on
check "count_maps：只输出一个整数（grep -c 为 0 时不出 '0\\n0'）" test "$(cj_count_maps nonexistent-tag 4242)" = 0
check "count_maps：有匹配" test "$(cj_count_maps hl-snap 4242)" -ge 1

# ═══════════════════════════ 2. shelf install / uninstall ═══════════════════════════
trap cleanup EXIT   # devlib 的 cj_with_rootfs_rw 会清掉 EXIT trap（被 source 进本 shell 时），这里重新挂上
section "shelf/install.sh：幂等 / 缺载荷不留半成品 / rw 窗口失败恢复 ro"
new_sandbox
PL="$R/payload"; mk_payload "$PL"
PRE_SIG="$(tree_sig)"
: > "$CJ_SIM_LOG"
run sh "$PL/install.sh" --no-systemd >/dev/null 2>&1
check "install --no-systemd：不 remount、不写 /usr" test "$(count_log 'remount')" = 0 -a -z "$(ls -A "$CJ_SYSD")"
new_sandbox; PL="$R/payload"; mk_payload "$PL"; PRE_SIG="$(tree_sig)"
: > "$CJ_SIM_LOG"
run sh "$PL/install.sh" >"$R/out1.txt" 2>&1; rc=$?
check "install 全量：退出 0" test "$rc" -eq 0
check "install：9 个服务二进制都在" test -x "$B/gateway" -a -x "$B/book-serve" -a -x "$B/note-serve" -a -x "$B/wallpaper-serve"
check "install：辅助脚本 lo-alias.sh / shelf-uninstall / 库 已装" test -x "$B/lo-alias.sh" -a -x "$B/shelf-uninstall" -a -f "$R/home/root/.local/lib/shelf/manifest.sh" -a -f "$R/home/root/.local/lib/shelf/devlib.sh"
check "install：单元 + shelf.target + wants 链接" test -f "$CJ_SYSD/gateway.service" -a -L "$CJ_SYSD/shelf.target.wants/book-serve.service" -a -L "$CJ_SYSD/multi-user.target.wants/shelf.target"
check "install：四个 qmd（字体/回收站/建夹/漫画页边距）都在 qrr 目录" test -f "$Q/font-menu-dynamic.qmd" -a -f "$Q/shelf-trash-agent.qmd" -a -f "$Q/shelf-mkdir-agent.qmd" -a -f "$Q/shelf-comic-margins.qmd"
check "install：rw 窗口只开一次、最后一次 mount 是 ro" test "$(count_log 'remount,rw')" = 1 -a "$(last_mount)" = "mount -o remount,ro /"
check "install：不跑 xovi/start、不重启 xochitl（只打印提示）" test "$(count_log XOVI_START)" = 0 -a "$(count_log 'restart xochitl')" = 0
check "install：提示里 xovi 未生效时指路 xovi/start（xovi 生效时指路 systemctl restart）" grep -q 'xovi/start' "$R/out1.txt"
check "install：暂存目录已清" test ! -e "$R/home/root/.cangjie-stage"
SIG1="$(tree_sig)"
: > "$CJ_SIM_LOG"; run sh "$PL/install.sh" >"$R/out2.txt" 2>&1; rc=$?
check "install 第二次：退出 0" test "$rc" -eq 0
sig_eq "install 第二次：文件树逐字节不变（幂等）" "$SIG1" "$(tree_sig)"
check "install 第二次：不 remount、不重启任何服务、不留空备份目录" test "$(count_log 'remount')" = 0 -a "$(count_log 'systemctl restart')" = 0 -a -z "$(ls -A "$R/home/root/cangjie-backups" 2>/dev/null)"
xovi_live on; run sh "$PL/install.sh" >"$R/out3.txt" 2>&1
check "install：xovi 已生效时提示 systemctl restart xochitl，且不再教 xovi/start" grep -q 'systemctl restart xochitl' "$R/out3.txt"
# 更新一个二进制：只重启它，且备份进 cangjie-backups
echo '#!/bin/sh' > "$PL/bin/font-serve"; echo '# v2' >> "$PL/bin/font-serve"
: > "$CJ_SIM_LOG"; run sh "$PL/install.sh" >/dev/null 2>&1
check "install：更新 font-serve → 只重启 font-serve.service" test "$(count_log 'systemctl restart')" = 1 -a "$(count_log 'restart font-serve.service')" = 1
check "install：旧 font-serve 备份进 cangjie-backups/shelf-*/" test -n "$(ls "$R"/home/root/cangjie-backups/shelf-*/font-serve 2>/dev/null)"

# 缺载荷：什么都不写
new_sandbox; PL="$R/payload"; mk_payload "$PL"; rm "$PL/bin/note-serve"; PRE_SIG="$(tree_sig)"; : > "$CJ_SIM_LOG"
run sh "$PL/install.sh" >"$R/out.txt" 2>&1; rc=$?
check "缺 bin/note-serve：退出非 0" test "$rc" -ne 0
sig_eq "缺 bin/note-serve：文件树完全未变（无半成品混装）" "$PRE_SIG" "$(tree_sig)"
check "缺 bin/note-serve：从未 remount" test "$(count_log remount)" = 0
new_sandbox; PL="$R/payload"; mk_payload "$PL"; rm "$PL/systemd/mind-serve.service"; PRE_SIG="$(tree_sig)"; : > "$CJ_SIM_LOG"
run sh "$PL/install.sh" >"$R/out.txt" 2>&1; rc=$?
check "缺 mind-serve.service：退出非 0、从未 remount（旧版会在 rw 状态半路退出）" test "$rc" -ne 0 -a "$(count_log remount)" = 0
sig_eq "缺 mind-serve.service：文件树未变" "$PRE_SIG" "$(tree_sig)"
# rw 窗口内失败：单元源不可读 → cp 失败 → 仍恢复 ro
new_sandbox; PL="$R/payload"; mk_payload "$PL"; chmod 000 "$PL/systemd/book-serve.service"; : > "$CJ_SIM_LOG"
run sh "$PL/install.sh" >"$R/out.txt" 2>&1; rc=$?
chmod 644 "$PL/systemd/book-serve.service"
check "rw 窗口内 cp 失败：退出非 0，且最后一次 mount 是 remount,ro（H2）" test "$rc" -ne 0 -a "$(last_mount)" = "mount -o remount,ro /"
# verity
new_sandbox; PL="$R/payload"; mk_payload "$PL"; : > "$CJ_SIM_LOG"
CJ_SIM_VERITY=1 run sh "$PL/install.sh" >"$R/out.txt" 2>&1
check "dm-verity 激活：不写 /usr、不 remount，二进制照装" test "$(count_log remount)" = 0 -a -x "$R/home/root/.local/bin/gateway" -a -z "$(ls -A "$CJ_SYSD")"
# 未知服务令牌
new_sandbox; PL="$R/payload"; mk_payload "$PL"
run sh "$PL/install.sh" --only bogus >/dev/null 2>&1; rc=$?
check "--only 未知令牌：退出 2、不写任何东西" test "$rc" -eq 2 -a ! -e "$R/home/root/.local/bin/gateway"
# 旧命名迁移
new_sandbox; PL="$R/payload"; mk_payload "$PL"
echo old > "$CJ_SYSD/shelf-gateway.service"; ln -s ../shelf-gateway.service "$CJ_SYSD/multi-user.target.wants-x" 2>/dev/null
echo old > "$R/home/root/.local/bin/shelf-gateway"; echo old > "$R/home/root/.local/bin/cangjie-lo-alias.sh"
run sh "$PL/install.sh" >/dev/null 2>&1
check "M7 旧命名迁移：shelf-gateway 单元/二进制、cangjie-lo-alias.sh 被清，且备份进 cangjie-backups" test ! -e "$CJ_SYSD/shelf-gateway.service" -a ! -e "$R/home/root/.local/bin/shelf-gateway" -a ! -e "$R/home/root/.local/bin/cangjie-lo-alias.sh" -a -f "$(ls "$R"/home/root/cangjie-backups/shelf-*/shelf-gateway | head -n 1)"
rm -f "$CJ_SYSD/multi-user.target.wants-x"
# 密码文件
new_sandbox; PL="$R/payload"; mk_payload "$PL"; printf '%s' "p a\"s;s\$x" > "$R/pw"
run sh "$PL/install.sh" --password-file "$R/pw" >/dev/null 2>&1
check "--password-file：密码原样传给 gateway passwd（特殊字符不被 shell 解释）" grep -Fq "gateway passwd p a\"s;s\$x" "$CJ_SIM_LOG"
check "--password-file：文件读后即删" test ! -e "$R/pw"
new_sandbox; PL="$R/payload"; mk_payload "$PL"; rm "$PL/bin/gateway"; printf 'pw' > "$R/pw"
run sh "$PL/install.sh" --password-file "$R/pw" >/dev/null 2>&1
check "--password-file：载荷校验失败退出时密码文件也已删" test ! -e "$R/pw"

section "shelf/uninstall.sh：与 install 清单对称"
new_sandbox; PL="$R/payload"; mk_payload "$PL"; PRE_SIG="$(tree_sig)"
run sh "$PL/install.sh" >/dev/null 2>&1
echo old > "$CJ_SYSD/shelf-gateway.service"; echo old > "$R/home/root/.local/bin/shelf-gateway"
echo "user note" > "$R/home/root/.local/share/shelf/user-file.txt"
mkdir -p "$R/home/root/.local/state/notes"; echo keep > "$R/home/root/.local/state/notes/entries.json"
: > "$CJ_SIM_LOG"
run sh "$R/home/root/.local/bin/shelf-uninstall" >"$R/out.txt" 2>&1; rc=$?
check "uninstall 全量：退出 0" test "$rc" -eq 0
POST_SIG="$(tree_sig | grep -v -e 'home/root/\.config/shelf/' -e 'home/root/\.local/share/shelf/' -e 'home/root/\.local/state/shelf/' -e 'home/root/\.local/state/notes/')"
sig_eq "uninstall：装过的每个文件都被删（文件树回到安装前，仅剩用户数据）" "$PRE_SIG" "$POST_SIG"
check "uninstall：comic-margins qmd 也删了" test ! -e "$Q/shelf-comic-margins.qmd"
check "uninstall：mkdir-agent qmd / lo-alias.sh / shelf-uninstall / 库 / 旧命名遗留 全清" test ! -e "$Q/shelf-mkdir-agent.qmd" -a ! -e "$B/lo-alias.sh" -a ! -e "$B/shelf-uninstall" -a ! -e "$R/home/root/.local/lib/shelf" -a ! -e "$CJ_SYSD/shelf-gateway.service" -a ! -e "$B/shelf-gateway"
check "uninstall：用户数据（含 share/shelf 里的用户文件、笔记线数据）保留" test -f "$R/home/root/.local/share/shelf/user-file.txt" -a -f "$R/home/root/.local/state/notes/entries.json"
check "uninstall：壁纸还原调用了 wallpaper-serve disable" grep -q 'wallpaper-serve disable' "$CJ_SIM_LOG"
check "uninstall：rw 窗口后最后一次 mount 是 ro" test "$(last_mount)" = "mount -o remount,ro /"
# --only
new_sandbox; PL="$R/payload"; mk_payload "$PL"; run sh "$PL/install.sh" >/dev/null 2>&1
run sh "$R/home/root/.local/bin/shelf-uninstall" --only book >/dev/null 2>&1
check "uninstall --only book：book 的二进制/单元/两个 qmd 删了，其它服务与共享件保留" test ! -e "$B/book-serve" -a ! -e "$CJ_SYSD/book-serve.service" -a ! -e "$Q/shelf-trash-agent.qmd" -a ! -e "$Q/shelf-mkdir-agent.qmd" -a -x "$B/gateway" -a -x "$B/lo-alias.sh" -a -x "$B/shelf-uninstall" -a -f "$Q/font-menu-dynamic.qmd"
run sh "$R/home/root/.local/bin/shelf-uninstall" --only bogus >/dev/null 2>&1; rc=$?
check "uninstall --only 未知令牌：退出 2" test "$rc" -eq 2
# --purge
new_sandbox; PL="$R/payload"; mk_payload "$PL"; run sh "$PL/install.sh" >/dev/null 2>&1
mkdir -p "$R/home/root/.local/state/notes"; echo keep > "$R/home/root/.local/state/notes/e.json"; echo d > "$R/home/root/.local/share/shelf/d.txt"
run sh "$R/home/root/.local/bin/shelf-uninstall" --purge >/dev/null 2>&1
check "uninstall --purge：只清 shelf 三个 XDG 目录，笔记线数据仍在" test ! -e "$R/home/root/.config/shelf" -a ! -e "$R/home/root/.local/share/shelf" -a ! -e "$R/home/root/.local/state/shelf" -a -f "$R/home/root/.local/state/notes/e.json"
# 用符号链接冒充 shelf 数据目录：拒绝清除
new_sandbox; PL="$R/payload"; mk_payload "$PL"; run sh "$PL/install.sh" >/dev/null 2>&1
mkdir -p "$R/precious"; echo keep > "$R/precious/f"; rm -rf "$R/home/root/.local/share/shelf"; ln -s "$R/precious" "$R/home/root/.local/share/shelf"
run sh "$R/home/root/.local/bin/shelf-uninstall" --purge >/dev/null 2>&1
check "uninstall --purge：数据目录是符号链接 → 拒绝，目标内容不动" test -f "$R/precious/f"

# ═══════════════════════════ 3. 部署脚本（假 ssh/scp）═══════════════════════════
section "packaging/deploy.sh：密码特殊字符 / shelf-pkg 换位"
mk_dummy_targets() {
    T=aarch64-unknown-linux-musl
    for f in shelf/target/$T/release/book-serve shelf/target/$T/release/koreader-serve gateway/target/$T/release/gateway \
             enhance/wallpaper-serve/target/$T/release/wallpaper-serve enhance/font-serve/target/$T/release/font-serve \
             notes/target/$T/release/ink-serve notes/target/$T/release/transcribe-serve notes/target/$T/release/mind-serve notes/target/$T/release/note-serve; do
        # 仓库里已有**真**交叉编译产物（开发机上构建过）时先挪走再造假的：否则 deploy.sh 会把真 aarch64 二进制装进
        # 沙箱去执行（Exec format error）。结束时 cleanup 无条件还原。
        if [ -e "$REPO/$f" ] && ! head -c 2 "$REPO/$f" 2>/dev/null | grep -q '^#!'; then
            mkdir -p "$TMPBASE/real-targets/$(dirname "$f")"
            mv "$REPO/$f" "$TMPBASE/real-targets/$f"
            echo "$f" >> "$TMPBASE/real-targets.txt"
        fi
        if [ ! -e "$REPO/$f" ]; then
            mkdir -p "$(dirname "$REPO/$f")"
            printf '#!/bin/sh\necho "%s $*" >> "%s"\n' "$(basename "$f")" '$CJ_SIM_LOG' > "$REPO/$f"
            # 让 stub 二进制写日志：日志路径运行期取 CJ_SIM_LOG（单引号里的 $CJ_SIM_LOG 在设备上由环境展开）
            sed -i 's|"\$CJ_SIM_LOG"|"${CJ_SIM_LOG:-/dev/null}"|' "$REPO/$f"
            chmod +x "$REPO/$f"; echo "$REPO/$f" >> "$CREATED_FILES"
        fi
    done
}
mk_dummy_targets
new_sandbox
( cd "$PKG" && SHELF_NO_BUILD=1 run sh deploy.sh 127.0.0.1 --only book,font --password 'p a"s;s$x' ) >"$R/out.txt" 2>&1; rc=$?
[ "$rc" -eq 0 ] || sed 's/^/     | /' "$R/out.txt" | tail -12   # 失败时把 deploy.sh 的输出末尾带出来，便于定位
check "deploy.sh：--only book,font + 含空格/引号/分号的密码 → 退出 0" test "$rc" -eq 0
check "deploy.sh：密码原样到达 gateway passwd（没被远端 shell 解释）" grep -Fq 'gateway passwd p a"s;s$x' "$CJ_SIM_LOG"
check "deploy.sh：密码不出现在任何 ssh 命令行里" test -z "$(grep '^ssh' "$CJ_SIM_LOG" | grep -F 's;s')"
check "deploy.sh：设备上无 .pw 残留、无 shelf-pkg.new 残留、shelf-pkg 完整" test ! -e "$R/home/root/shelf-pkg/.pw" -a ! -e "$R/home/root/shelf-pkg.new" -a -f "$R/home/root/shelf-pkg/shelf/install.sh" -a -f "$R/home/root/shelf-pkg/shelf/manifest.sh" -a -f "$R/home/root/shelf-pkg/shelf/devlib.sh"
check "deploy.sh：只装了 gateway/book/font 三个服务（--only 透传）" test -x "$R/home/root/.local/bin/book-serve" -a -x "$R/home/root/.local/bin/font-serve" -a ! -e "$R/home/root/.local/bin/note-serve"
# 换位保护：载荷里没有 install.sh 时不能把现成 shelf-pkg 清掉（直接测远端换位命令的语义）
mkdir -p "$R/home/root/shelf-pkg/shelf"; echo keepme > "$R/home/root/shelf-pkg/shelf/marker"
empty="$R/empty.tar"; mkdir -p "$R/emptyroot/shelf"; tar -C "$R/emptyroot" -cf "$empty" shelf
( cd "$R/home/root" && PATH="$STUBS:$PATH" sh -c "ssh -o x=y root@h 'rm -rf /home/root/shelf-pkg.new && mkdir -p /home/root/shelf-pkg.new && tar -C /home/root/shelf-pkg.new -xf - && [ -f /home/root/shelf-pkg.new/shelf/install.sh ] && rm -rf /home/root/shelf-pkg && mv /home/root/shelf-pkg.new /home/root/shelf-pkg' < '$empty'" ) >/dev/null 2>&1
check "shelf-pkg 换位：新载荷缺 install.sh → 旧 shelf-pkg 原样保留" test -f "$R/home/root/shelf-pkg/shelf/marker"

section "packaging/deploy-hl-snap.sh：原子落位 / 备份不进 extensions.d / DEFER"
new_sandbox; EXT="$R/home/root/xovi/extensions.d"
echo OLDSO > "$EXT/hl-snap.so"; : > "$CJ_SIM_LOG"
( cd "$PKG" && CJ_SKIP_BUILD=1 DEFER_XOVI_START=1 run sh deploy-hl-snap.sh 127.0.0.1 ) >"$R/out.txt" 2>&1; rc=$?
check "hl-snap DEFER：退出 0" test "$rc" -eq 0
check "hl-snap：设备上的 .so 与仓库产物 md5 一致" test "$(md5sum < "$EXT/hl-snap.so")" = "$(md5sum < "$REPO/enhance/hl-snap/hl-snap.so")"
check "hl-snap：旧 .so 备份进 cangjie-backups，extensions.d 里只有 hl-snap.so（无 .bak/.new/临时文件）" test -f "$(ls "$R"/home/root/cangjie-backups/hl-snap.so.bak.pre-* | head -n 1)" -a "$(ls -A "$EXT")" = "hl-snap.so"
check "hl-snap DEFER：不重启 xochitl、不 xovi/start" test "$(count_log 'restart xochitl')" = 0 -a "$(count_log XOVI_START)" = 0
check "hl-snap：暂存/推送目录里带了 xovi-ext-install.sh 与 devlib.sh" test -f "$R/home/root/hl-snap/deploy/xovi-ext-install.sh" -a -f "$R/home/root/hl-snap/deploy/devlib.sh"
check "hl-snap：reading-qol.json 首次装才建" test -f "$R/home/root/.local/share/cangjie-ime/reading-qol.json"
echo '{"user":"setting"}' > "$R/home/root/.local/share/cangjie-ime/reading-qol.json"
( cd "$PKG" && CJ_SKIP_BUILD=1 DEFER_XOVI_START=1 run sh deploy-hl-snap.sh 127.0.0.1 ) >/dev/null 2>&1
check "hl-snap 重复部署：已有的 reading-qol.json 不被覆盖" grep -q '"user":"setting"' "$R/home/root/.local/share/cangjie-ime/reading-qol.json"
# 非 DEFER：xovi 已生效 → restart（H1）
xovi_live on; : > "$CJ_SIM_LOG"
( cd "$PKG" && CJ_SKIP_BUILD=1 run sh deploy-hl-snap.sh 127.0.0.1 ) >"$R/out.txt" 2>&1; rc=$?
check "hl-snap 非 DEFER + xovi 已生效：systemctl restart xochitl、绝不 xovi/start、健康检查通过" test "$rc" -eq 0 -a "$(count_log 'systemctl restart xochitl')" = 1 -a "$(count_log XOVI_START)" = 0
xovi_live off; : > "$CJ_SIM_LOG"
( cd "$PKG" && CJ_SKIP_BUILD=1 run sh deploy-hl-snap.sh 127.0.0.1 ) >/dev/null 2>&1
check "hl-snap 非 DEFER + xovi 未生效：走 xovi/start" test "$(count_log XOVI_START)" = 1
# md5 传输损坏
new_sandbox; EXT="$R/home/root/xovi/extensions.d"; echo OLDSO > "$EXT/hl-snap.so"
( cd "$PKG" && CJ_SIM_SCP_CORRUPT=hl-snap.so CJ_SKIP_BUILD=1 DEFER_XOVI_START=1 run sh deploy-hl-snap.sh 127.0.0.1 ) >"$R/out.txt" 2>&1; rc=$?
check "scp 传输损坏：md5 对不上 → 非 0，extensions.d 里旧 .so 原样、无残留" test "$rc" -ne 0 -a "$(cat "$EXT/hl-snap.so")" = OLDSO -a "$(ls -A "$EXT")" = "hl-snap.so"

section "packaging/deploy-handwriting-stroke.sh（同一份数据驱动流程）"
new_sandbox; EXT="$R/home/root/xovi/extensions.d"
( cd "$PKG" && CJ_SKIP_BUILD=1 DEFER_XOVI_START=1 run sh deploy-handwriting-stroke.sh 127.0.0.1 ) >"$R/out.txt" 2>&1; rc=$?
check "hw-stroke DEFER：落位 hw-stroke.so，配置键是 hwStrokeWidthFactor" test "$rc" -eq 0 -a -f "$EXT/hw-stroke.so" -a -n "$(grep hwStrokeWidthFactor "$R/home/root/.local/share/cangjie-ime/reading-qol.json")"

section "packaging/deploy-xovi-apply.sh：H1"
new_sandbox; echo x > "$R/home/root/xovi/extensions.d/hl-snap.so"; xovi_live on; : > "$CJ_SIM_LOG"
( cd "$PKG" && run sh deploy-xovi-apply.sh 127.0.0.1 ) >"$R/out.txt" 2>&1; rc=$?
check "xovi-apply：xovi 已生效 → systemctl restart xochitl，绝不 xovi/start（2026-09-20 事故）" test "$rc" -eq 0 -a "$(count_log 'systemctl restart xochitl')" = 1 -a "$(count_log XOVI_START)" = 0
check "xovi-apply：输出里有\"打断\"提示" grep -q '打断' "$R/out.txt"
xovi_live off; : > "$CJ_SIM_LOG"
( cd "$PKG" && run sh deploy-xovi-apply.sh 127.0.0.1 ) >/dev/null 2>&1
check "xovi-apply：xovi 未生效 → xovi/start" test "$(count_log XOVI_START)" = 1 -a "$(count_log 'restart xochitl')" = 0

section "packaging/deploy-appload-patch.sh：H3 暂存 + md5 + 原子替换"
new_sandbox; SO="$R/home/root/xovi/extensions.d/appload.so"; echo ORIGAPPLOAD > "$SO"
( cd "$PKG" && run sh deploy-appload-patch.sh 127.0.0.1 ) >"$R/out.txt" 2>&1; rc=$?
check "appload：补丁版落位，备份进 cangjie-backups，extensions.d 里没有多余文件" test "$rc" -eq 0 -a "$(tail -n 1 "$SO")" = PATCHED -a -n "$(ls "$R"/home/root/cangjie-backups/appload.so.bak.pre-* 2>/dev/null)" -a "$(ls -A "$R/home/root/xovi/extensions.d")" = "appload.so"
check "appload：不自动重启 xochitl、不 xovi/start" test "$(count_log 'restart xochitl')" = 0 -a "$(count_log XOVI_START)" = 0
new_sandbox; SO="$R/home/root/xovi/extensions.d/appload.so"; echo ORIGAPPLOAD > "$SO"
( cd "$PKG" && CJ_SIM_SCP_CORRUPT=appload.so.new run sh deploy-appload-patch.sh 127.0.0.1 ) >"$R/out.txt" 2>&1; rc=$?
check "appload 传输损坏：非 0，extensions.d 里 appload.so 原样（H3：坏文件不会落到自动加载目录）" test "$rc" -ne 0 -a "$(cat "$SO")" = ORIGAPPLOAD -a "$(ls -A "$R/home/root/xovi/extensions.d")" = "appload.so"

section "packaging/deploy-usr-unit（wifi-watch / chrony-boot-wakelock / xovi-persist）"
new_sandbox; : > "$CJ_SIM_LOG"
( cd "$PKG" && run sh deploy-wifi-watch.sh 127.0.0.1 ) >"$R/out.txt" 2>&1; rc=$?
check "wifi-watch：单元 + wants 链接 + ~/.local/bin/wifi-watch.sh + 已 restart" test "$rc" -eq 0 -a -f "$CJ_SYSD/wifi-watch.service" -a -L "$CJ_SYSD/multi-user.target.wants/wifi-watch.service" -a -x "$R/home/root/.local/bin/wifi-watch.sh" -a "$(count_log 'restart wifi-watch.service')" = 1
: > "$CJ_SIM_LOG"; ( cd "$PKG" && run sh deploy-wifi-watch.sh 127.0.0.1 ) >/dev/null 2>&1
check "wifi-watch 再装一次：不 remount、不重启服务（幂等）" test "$(count_log remount)" = 0 -a "$(count_log 'restart wifi-watch.service')" = 0
new_sandbox; : > "$CJ_SIM_LOG"; CJ_SIM_VERITY=1 bash -c "cd '$PKG' && PATH='$STUBS:'\$PATH sh deploy-chrony-boot-wakelock.sh 127.0.0.1" >"$R/out.txt" 2>&1; rc=$?
check "chrony-boot-wakelock + dm-verity：退出 0（跳过非失败）、不 remount、不写 /usr" test "$rc" -eq 0 -a "$(count_log remount)" = 0 -a -z "$(ls -A "$CJ_SYSD")"
new_sandbox; rm "$R/home/root/xovi/start"; ( cd "$PKG" && run sh deploy-xovi-persist.sh 127.0.0.1 ) >"$R/out.txt" 2>&1; rc=$?
check "xovi-persist：没装 xovi 本体 → 失败，且不写 /usr、不 remount" test "$rc" -ne 0 -a "$(count_log remount)" = 0 -a -z "$(ls -A "$CJ_SYSD")"

section "packaging/deploy-battop.sh：有意不开机自启 / 原子替换 / 保留用户关闭状态"
new_sandbox; echo BIN1 > "$R/battop.bin"; : > "$CJ_SIM_LOG"
( cd "$PKG" && CJ_BATTOP_BIN="$R/battop.bin" run sh deploy-battop.sh 127.0.0.1 ) >"$R/out.txt" 2>&1; rc=$?
check "battop 首装：二进制+单元就位，起服务" test "$rc" -eq 0 -a "$(cat "$R/home/root/battop/battop")" = BIN1 -a -f "$CJ_SYSD/battop.service" -a "$(count_log 'restart battop.service')" = 1
check "battop：不建 wants 链接、不 systemctl enable（有意不开机自启）" test ! -e "$CJ_SYSD/multi-user.target.wants/battop.service" -a "$(count_log 'systemctl enable')" = 0
echo BIN2 > "$R/battop.bin"; : > "$CJ_SIM_LOG"
( cd "$PKG" && CJ_BATTOP_BIN="$R/battop.bin" run sh deploy-battop.sh 127.0.0.1 ) >/dev/null 2>&1
check "battop 更新：旧二进制备份进 cangjie-backups；旧的在跑 → restart 载入新版；没有先 stop 再 scp" test "$(cat "$R/home/root/battop/battop")" = BIN2 -a -n "$(ls "$R"/home/root/cangjie-backups/battop.bak.pre-* 2>/dev/null)" -a "$(count_log 'systemctl stop')" = 0 -a "$(count_log 'restart battop.service')" = 1
: > "$CJ_SIM_LOG"; ( cd "$PKG" && CJ_BATTOP_BIN="$R/battop.bin" run sh deploy-battop.sh 127.0.0.1 ) >/dev/null 2>&1
check "battop 重复部署同一份二进制：不重启（不重复触发 cgroup 迁移）、不再堆备份" test "$(count_log 'restart battop.service')" = 0 -a "$(ls "$R"/home/root/cangjie-backups/battop.bak.pre-* | wc -l)" -eq 1
echo BIN3 > "$R/battop.bin"; : > "$CJ_SIM_LOG"
CJ_SIM_INACTIVE=1 bash -c "cd '$PKG' && CJ_BATTOP_BIN='$R/battop.bin' PATH='$STUBS:'\$PATH sh deploy-battop.sh 127.0.0.1" >/dev/null 2>&1
check "battop 已装但当前停着（用户在网页关了）→ 更新二进制但不擅自启动" test "$(cat "$R/home/root/battop/battop")" = BIN3 -a "$(count_log 'restart battop.service')" = 0
new_sandbox; CJ_SIM_VERITY=1 bash -c "cd '$PKG' && CJ_BATTOP_BIN='$R/battop.bin' PATH='$STUBS:'\$PATH sh deploy-battop.sh 127.0.0.1" >/dev/null 2>&1
check "battop + dm-verity：不 remount（补上原先缺的 verity 门）" test "$(count_log remount)" = 0

section "packaging/deploy-sidebar-entry.sh"
new_sandbox; Q="$R/home/root/xovi/exthome/qt-resource-rebuilder"; echo OLDQMD > "$Q/koreader-sidebar-entry.qmd"
( cd "$PKG" && DEFER_XOVI_START=1 PATH="$STUBS:$PATH" sh deploy-sidebar-entry.sh 127.0.0.1 ) >"$R/out.txt" 2>&1; rc=$?
check "sidebar-entry DEFER：qmd/rcc 落位，旧 qmd 备份进 cangjie-backups，qrr 目录里无 .bak/暂存残留" test "$rc" -eq 0 -a -f "$Q/cangjie-icons.rcc" -a -n "$(ls "$R"/home/root/cangjie-backups/koreader-sidebar-entry.qmd.bak.pre-* 2>/dev/null)" -a -z "$(find "$Q" -maxdepth 1 \( -name '*.bak*' -o -name '*.new*' \) )" -a ! -e "$R/home/root/.cangjie-stage"
check "sidebar-entry DEFER：不重启 xochitl" test "$(count_log 'restart xochitl')" = 0 -a "$(count_log XOVI_START)" = 0
xovi_live on; : > "$CJ_SIM_LOG"
( cd "$PKG" && PATH="$STUBS:$PATH" sh deploy-sidebar-entry.sh 127.0.0.1 ) >"$R/out.txt" 2>&1; rc=$?
check "sidebar-entry 非 DEFER + xovi 已生效：systemctl restart xochitl，绝不 xovi/start" test "$rc" -eq 0 -a "$(count_log 'systemctl restart xochitl')" = 1 -a "$(count_log XOVI_START)" = 0
new_sandbox; CJ_SIM_SCP_CORRUPT=cangjie-icons.rcc bash -c "cd '$PKG' && DEFER_XOVI_START=1 PATH='$STUBS:'\$PATH sh deploy-sidebar-entry.sh 127.0.0.1" >/dev/null 2>&1; rc=$?
check "sidebar-entry rcc 传输损坏：非 0，qrr 目录里没有 rcc/qmd（H3 同类：坏文件不进自动加载路径）" test "$rc" -ne 0 -a ! -e "$R/home/root/xovi/exthome/qt-resource-rebuilder/cangjie-icons.rcc"

# ═══════════════════════════ 4. install-all → uninstall-all 整轮对称 ═══════════════════════════
section "install-all → uninstall-all 整轮"
new_sandbox
echo BATTOPBIN > "$R/battop.bin"
export CJ_BATTOP_BIN="$R/battop.bin" CJ_SKIP_BUILD=1 SHELF_NO_BUILD=1
export CJ_ALLOWLIST_LOCAL="$R/allow.local.txt"
ALLOW_MD5="$(md5sum < "$PKG/firmware-allowlist.txt")"
PRE_SIG="$(tree_sig)"; : > "$CJ_SIM_LOG"
( cd "$PKG" && run sh install-all.sh 127.0.0.1 --skip chrony-cn,timezone-cn ) >"$R/out.txt" 2>&1; rc=$?
check "install-all 固件不在白名单：拒装（退出非 0）、不 remount" test "$rc" -ne 0 -a "$(count_log remount)" = 0
sig_eq "install-all 固件不在白名单：什么都没装" "$PRE_SIG" "$(tree_sig)"
xovi_live on
( cd "$PKG" && run sh install-all.sh 127.0.0.1 --force --skip chrony-cn,timezone-cn ) >"$R/out.txt" 2>&1; rc=$?
check "install-all --force：整轮退出 0" test "$rc" -eq 0
check "install-all --force：未验证哈希写进本机 allowlist.local，被 git 跟踪的白名单文件不变（L5）" test -s "$R/allow.local.txt" -a "$ALLOW_MD5" = "$(md5sum < "$PKG/firmware-allowlist.txt")"
check "install-all：整轮下来 xovi 已生效 → 只 systemctl restart xochitl 一次，全程没有 xovi/start（H1）" test "$(count_log 'systemctl restart xochitl')" = 1 -a "$(count_log XOVI_START)" = 0
check "install-all：hl-snap/hw-stroke 落进 extensions.d，且目录里只有它俩" test -f "$R/home/root/xovi/extensions.d/hl-snap.so" -a -f "$R/home/root/xovi/extensions.d/hw-stroke.so" -a "$(ls "$R/home/root/xovi/extensions.d" | wc -l)" -eq 2
W="$CJ_SYSD/multi-user.target.wants"
check "install-all：wifi-watch（M1）/ xovi-reenable / chrony-boot-wakelock 单元在 /usr 且有 wants 链接" test -L "$W/wifi-watch.service" -a -L "$W/xovi-reenable.service" -a -L "$W/chrony-boot-wakelock.service"
check "install-all：battop 单元在 /usr 但没有 wants 链接（有意不开机自启）" test -f "$CJ_SYSD/battop.service" -a ! -e "$W/battop.service" -a ! -L "$W/battop.service"
check "install-all：qmd 全部就位（sidebar/字体/回收站/建夹）" test -f "$R/home/root/xovi/exthome/qt-resource-rebuilder/koreader-sidebar-entry.qmd" -a -f "$R/home/root/xovi/exthome/qt-resource-rebuilder/shelf-mkdir-agent.qmd" -a -f "$R/home/root/xovi/exthome/qt-resource-rebuilder/font-menu-dynamic.qmd"
check "install-all：每次 rw 窗口都以 ro 收尾" test "$(last_mount)" = "mount -o remount,ro /"
SIG_INSTALLED="$(tree_sig)"
: > "$CJ_SIM_LOG"
( cd "$PKG" && run sh install-all.sh 127.0.0.1 --skip chrony-cn,timezone-cn ) >"$R/out2.txt" 2>&1; rc=$?
check "install-all 第二遍（固件已在本机白名单）：退出 0" test "$rc" -eq 0
sig_eq "install-all 第二遍：文件树不变（幂等）" "$SIG_INSTALLED" "$(tree_sig)"
check "install-all 第二遍：不 remount rootfs（单元都已是最新）、仍无 xovi/start" test "$(count_log remount)" = 0 -a "$(count_log XOVI_START)" = 0
( cd "$PKG" && run sh install-all.sh 127.0.0.1 --skip chrony-cn,timezone-cn,bogus-step ) >"$R/out3.txt" 2>&1
check "--skip 未知步骤名：给出警告而不是静默" grep -q '不是已知步骤名' "$R/out3.txt"

: > "$CJ_SIM_LOG"
( cd "$PKG" && run sh uninstall-all.sh 127.0.0.1 ) >"$R/uout.txt" 2>&1; rc=$?
check "uninstall-all：整轮退出 0" test "$rc" -eq 0
POST_SIG="$(tree_sig | grep -v \
    -e 'home/root/\.config/shelf/' -e 'home/root/\.local/share/shelf/' -e 'home/root/\.local/state/shelf/' \
    -e 'home/root/battop/' -e 'home/root/hl-snap/' -e 'home/root/hw-stroke/' -e 'home/root/pkg-' -e 'home/root/shelf-pkg/' -e 'home/root/\.local/share/cangjie-ime/')"
sig_eq "uninstall-all：文件树回到安装前（仅剩有意保留的暂存目录/用户数据/battop 数据/配置）" "$PRE_SIG" "$POST_SIG"
check "uninstall-all：不重启 xochitl、不 xovi/start" test "$(count_log 'restart xochitl')" = 0 -a "$(count_log XOVI_START)" = 0
check "uninstall-all：最后一次 mount 是 ro" test "$(last_mount)" = "mount -o remount,ro /"
( cd "$PKG" && run sh uninstall-all.sh 127.0.0.1 ) >"$R/uout2.txt" 2>&1; rc=$?
check "uninstall-all 第二遍：仍退出 0（幂等）" test "$rc" -eq 0
: > "$CJ_SIM_LOG"
( cd "$PKG" && run sh uninstall-all.sh 127.0.0.1 --purge --skip shelf ) >/dev/null 2>&1
check "uninstall-all --purge：battop 目录才被清" test ! -e "$R/home/root/battop"
unset CJ_BATTOP_BIN CJ_SKIP_BUILD SHELF_NO_BUILD CJ_ALLOWLIST_LOCAL

# ═══════════════════════════ 5. 静态守卫 / 清单对称 ═══════════════════════════
section "静态守卫"
cd "$REPO" || exit 1
# 1) 只有库/既有的 /etc 覆写脚本可以 remount rw
viol="$(grep -rn 'remount,rw' packaging/*.sh shelf/*.sh enhance/*/install.sh enhance/*/deploy/*.sh 2>/dev/null | grep -v -e '^packaging/devlib.sh' -e '^packaging/chrony-cn.sh' -e '^packaging/timezone-cn.sh' -e ':[0-9]*:[[:space:]]*#' -e 'echo')"
check "remount,rw 只出现在 devlib.sh（带 trap）与 chrony-cn/timezone-cn（自带重试收尾）里" test -z "$viol"
[ -n "$viol" ] && echo "$viol"
# 2) 可执行的 xovi/start 只允许在 devlib.sh
viol="$(grep -rnE '^[[:space:]]*("\$[A-Za-z_]*/start"|/home/root/xovi/start)[[:space:]]*(\|\||;|$)' packaging/*.sh shelf/*.sh enhance/*/install.sh enhance/*/deploy/*.sh 2>/dev/null | grep -v '^packaging/devlib.sh')"
check "xovi/start 不再被任何脚本直接执行（统一走 devlib 的 cj_xochitl_apply）" test -z "$viol"
[ -n "$viol" ] && echo "$viol"
# 3) 步骤表对称：每个步骤都有脚本；非配置类步骤在 uninstall-all 里有 uninstall_ 函数
# shellcheck disable=SC1091
( cd "$PKG" && . ./lib.sh
  for step in $STEP_ORDER; do
      s="$(step_script "$step")" || { echo "步骤 $step 无脚本映射"; exit 1; }
      [ -f "$s" ] || { echo "步骤 $step 的脚本 $s 不存在"; exit 1; }
      if ! word_in "$step" "$STEP_CONFIG_ONLY"; then
          fn="uninstall_$(echo "$step" | tr '-' '_')"
          grep -q "^$fn()" uninstall-all.sh || { echo "步骤 $step 缺 $fn"; exit 1; }
      fi
  done ) >"$TMPBASE/sym.txt" 2>&1
check "install-all/uninstall-all 步骤表对称（每步有脚本；非配置步骤有 uninstall_ 函数）" test ! -s "$TMPBASE/sym.txt"
cat "$TMPBASE/sym.txt"
# 4) shelf 清单对称：manifest 里声明的每个 qmd/辅助脚本，install.sh 与 uninstall.sh 都是靠 manifest 函数处理，不再硬编码
check "shelf/uninstall.sh 不再硬编码 qmd 文件名（走 manifest）" test -z "$(grep -n 'shelf-trash-agent\|shelf-mkdir-agent\|font-menu-dynamic' shelf/uninstall.sh | grep -v '^[0-9]*:#')"
# 5) 备份不进 extensions.d：脚本里不存在把 .bak 写进 extensions.d 的写法
check "没有脚本往 extensions.d 里写 .bak/.new" test -z "$(grep -rn 'extensions.d/[^"]*\.\(bak\|new\)' packaging/*.sh enhance/*/deploy/*.sh 2>/dev/null | grep -v ':[0-9]*:[[:space:]]*#')"

# 守卫：真实 HOME 下不该出现任何测试产物
GUARD_AFTER=""
for g in $(guard_paths); do [ -e "$g" ] && GUARD_AFTER="$GUARD_AFTER $g"; done
check "真实 HOME 未被测试触碰（无新增 shelf/cangjie-backups/.stage 等目录）" test "$GUARD_BEFORE" = "$GUARD_AFTER"

echo
echo "════ 结果：通过 $PASS，失败 $FAIL ════"
[ "$FAIL" -eq 0 ]
