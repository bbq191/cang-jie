# shellcheck shell=bash
# shellcheck disable=SC2154  # R/B/Q/CJ_SIM_LOG/STUBS/PKG/TMPBASE 等由 run_sim_tests.sh 的沙箱函数设置
# ═══════════════════════════════════════════════════════════════════════════
# 设备端安装/卸载脚本加固（2026-10-10，第七轮审计后）的本机模拟用例。
# 由 run_sim_tests.sh 在末尾 source，复用它的 new_sandbox / mk_payload / run / check / count_log / last_mount。
# 覆盖：rw 窗口与 /etc 下层窗口"remount rw 与挂 trap 之间被信号打断"、绑定点是符号链接、原子替换的中断残留回收、
#       shelf 重跑时重启仍跑着旧二进制的服务、只改密码也重启网关、qmd 写到一半失败也记待生效标记、
#       调用方 umask 000 时生成文件不全局可写、chrony-cn 不再经 /tmp 固定名中转、改 /etc 视图失败如实报错。
# ═══════════════════════════════════════════════════════════════════════════

section "设备端加固（10-10）：信号窗口 / 中断残留 / 重跑收敛 / 权限"

# 本节自带的 mount 桩：remount rw 之后立刻给调用它的 shell 发 TERM（模拟 ssh 断开、Ctrl-C 恰好落在这一刻）
SD_STUBS="$TMPBASE/simdev-stubs"; mkdir -p "$SD_STUBS"
cat > "$SD_STUBS/mount" <<'STUB'
#!/bin/sh
echo "mount $*" >> "${CJ_SIM_LOG:-/dev/null}"
case "$*" in *remount,rw*) [ -n "${CJ_SIM_SIG_AFTER_RW:-}" ] && kill -TERM "$PPID" ;; esac
exit 0
STUB
chmod +x "$SD_STUBS/mount"
# 没有"其他用户可写"位
not_ww() { m="$(stat -c %a "$1" 2>/dev/null)" || return 1; [ $(( 0$m & 2 )) -eq 0 ]; }

# ── devlib：信号落在 remount rw 之后、挂 trap 之前 ──
new_sandbox; : > "$CJ_SIM_LOG"
CJ_SIM_SIG_AFTER_RW=1 PATH="$SD_STUBS:$STUBS:$PATH" sh -c ". '$PKG/devlib.sh'; body() { :; }; cj_with_rootfs_rw body" >/dev/null 2>&1
check "with_rootfs_rw：remount rw 刚成功就收到 TERM → 仍恢复 ro（先挂 trap 再 remount）" test "$(last_mount)" = "mount -o remount,ro /"
mkdir -p "$R/elsig"; : > "$CJ_SIM_LOG"
CJ_SIM_SIG_AFTER_RW=1 CJ_TMPDIR="$R/elsig" PATH="$SD_STUBS:$STUBS:$PATH" sh -c ". '$PKG/devlib.sh'; f() { :; }; cj_etc_lower_edit t f" >/dev/null 2>&1
check "etc_lower_edit：remount rw 刚成功就收到 TERM → 仍恢复 ro、不 bind" test "$(last_mount)" = "mount -o remount,ro /" -a "$(count_log -- '--bind')" = 0

# ── devlib：/etc 下层窗口的绑定点已是符号链接 → 拒绝，什么都不碰 ──
mkdir -p "$R/elsym/target"; ln -s "$R/elsym/target" "$R/elsym/t.rootbind"; : > "$CJ_SIM_LOG"
( . "$PKG/devlib.sh"; export PATH="$STUBS:$PATH"; f() { echo ran > "$R/elsym/ran"; }; CJ_TMPDIR="$R/elsym" cj_etc_lower_edit t f ) >"$R/out.txt" 2>&1; rc=$?
check "etc_lower_edit：绑定点是符号链接 → 返回 1、不 remount、不跑 FUNC、链接原样" test "$rc" -eq 1 -a "$(count_log '^mount')" = 0 -a ! -e "$R/elsym/ran" -a -L "$R/elsym/t.rootbind" -a -n "$(grep '符号链接' "$R/out.txt")"

# ── devlib：原子替换 / 待换入区 回收上次中断留下的暂存文件 ──
mkdir -p "$R/sr"; echo new > "$R/sr/src"; echo old > "$R/sr/dst"; echo half > "$R/sr/.dst.new.99999"; ln -s "$R/sr/src" "$R/sr/.dst.new.88888"
( . "$PKG/devlib.sh"; cj_safe_replace "$R/sr/src" "$R/sr/dst" ) >/dev/null 2>&1; rc=$?
check "safe_replace：回收 .<名>.new.<pid> 中断残留（只删常规文件，符号链接不碰），照常替换" test "$rc" -eq 0 -a "$(cat "$R/sr/dst")" = new -a ! -e "$R/sr/.dst.new.99999" -a -L "$R/sr/.dst.new.88888"
mkdir -p "$R/home/root/.cangjie-stage/so-pending"; echo half > "$R/home/root/.cangjie-stage/so-pending/.x.so.new.4321"; echo so > "$R/sr/x.so"
# 前面的 devlib 单元段把库 source 进了本 shell（CJ_SO_PENDING_DIR 等仍指着旧沙箱），这里显式指定
# shellcheck disable=SC2034  # 给紧接着 source 的 devlib.sh 用
( CJ_SO_PENDING_DIR="$R/home/root/.cangjie-stage/so-pending"; . "$PKG/devlib.sh"; cj_so_stage "$R/sr/x.so" ) >/dev/null 2>&1
check "so_stage：回收待换入区里的中断残留" test ! -e "$R/home/root/.cangjie-stage/so-pending/.x.so.new.4321" -a -f "$R/home/root/.cangjie-stage/so-pending/x.so"

# ── shelf/install.sh：重跑收敛 ──
new_sandbox; PL="$R/payload"; mk_payload "$PL"
run sh "$PL/install.sh" >/dev/null 2>&1
# 上一轮换了二进制却没走到重启（中断）：服务主进程还跑着被删的旧 inode
ln -sfn "$R/home/root/.local/bin/gateway (deleted)" "$R/proc/4242/exe"; : > "$CJ_SIM_LOG"
run sh "$PL/install.sh" >/dev/null 2>&1; rc=$?
check "shelf 重跑：二进制已相同但服务还跑着被替换的旧二进制（exe 带 (deleted)）→ 重启它" test "$rc" -eq 0 -a "$(count_log 'restart gateway.service')" = 1
ln -sfn "$R/home/root/.local/bin/gateway" "$R/proc/4242/exe"; : > "$CJ_SIM_LOG"
run sh "$PL/install.sh" >/dev/null 2>&1
check "shelf 重跑：服务跑的就是当前二进制 → 不重启任何服务" test "$(count_log 'systemctl restart')" = 0
rm -f "$R/proc/4242/exe"
# 中断残留：~/.local/bin 里上次被打断的暂存文件
echo half > "$B/.gateway.new.99999"
run sh "$PL/install.sh" >/dev/null 2>&1
check "shelf 重跑：~/.local/bin 里上次中断留下的 .gateway.new.<pid> 被回收" test ! -e "$B/.gateway.new.99999"
# 只改密码（二进制都没变）：网关要重启才读到新密码
printf 'newpw' > "$R/pw"; : > "$CJ_SIM_LOG"
run sh "$PL/install.sh" --password-file "$R/pw" >/dev/null 2>&1; rc=$?
check "shelf 重装只改密码：重启 gateway（且只重启它）" test "$rc" -eq 0 -a "$(count_log 'restart gateway.service')" = 1 -a "$(count_log 'systemctl restart')" = 1

# qmd 写到一半失败（第二个 qmd 读不了）：已换上的那个要留下待生效标记，重跑时才不会判"无需生效"
new_sandbox; PL="$R/payload"; mk_payload "$PL"; chmod 000 "$PL/xovi/shelf-mkdir-agent.qmd"
run sh "$PL/install.sh" >"$R/out.txt" 2>&1; rc=$?
chmod 644 "$PL/xovi/shelf-mkdir-agent.qmd"
check "shelf qmd 写到一半失败：退出非 0，已换上的 qmd 已记待生效标记 shelf-qmd" test "$rc" -ne 0 -a -f "$Q/shelf-trash-agent.qmd" -a -f "$CJ_PENDING_DIR/shelf-qmd"

# 调用方 umask 000：生成的目录/文件不全局可写
new_sandbox; PL="$R/payload"; mk_payload "$PL"
( umask 000; run sh "$PL/install.sh" ) >/dev/null 2>&1
echo '#!/bin/sh' > "$PL/bin/book-serve"; echo '# v2' >> "$PL/bin/book-serve"
( umask 000; run sh "$PL/install.sh" ) >/dev/null 2>&1
sd_shelf_perms() {
    not_ww "$XDG_CONFIG_HOME/shelf" && not_ww "$XDG_STATE_HOME/shelf" && not_ww "$R/home/root/.local/lib/shelf" && not_ww "$R/home/root/cangjie-backups" || return 1
    for f in "$R"/home/root/cangjie-backups/shelf-*/ "$R"/home/root/cangjie-backups/shelf-*/book-serve; do not_ww "$f" || return 1; done
}
check "shelf umask 000：~/.config/shelf、~/.local/lib/shelf、备份目录与备份文件都不全局可写" sd_shelf_perms

# ── xovi 扩展安装：umask 000 ──
new_sandbox
( umask 000; cd "$PKG" && CJ_SKIP_BUILD=1 DEFER_XOVI_START=1 run sh deploy-hl-snap.sh 127.0.0.1 ) >/dev/null 2>&1; rc=$?
sd_ext_perms() { test "$rc" -eq 0 && not_ww "$R/home/root/.local/share/cangjie-ime" && not_ww "$R/home/root/.local/share/cangjie-ime/reading-qol.json"; }
check "hl-snap umask 000：cangjie-ime/ 与 reading-qol.json 不全局可写" sd_ext_perms

# ── chrony-cn / timezone-cn ──
new_sandbox; printf 'none / rootfs ro 0 0\n' > "$R/mounts-plain"; printf 'server a.google.com iburst\n' > "$R/chrony.conf"; chmod 644 "$R/chrony.conf"
( umask 000; cd "$PKG" && CJ_CHRONY_CONF="$R/chrony.conf" CJ_MOUNTS="$R/mounts-plain" CJ_BACKUP_DIR="$R/cbk" CJ_TMPDIR="$R" run sh deploy-chrony-cn.sh 127.0.0.1 ) >/dev/null 2>&1; rc=$?
check "chrony-cn umask 000：改写后的 chrony.conf 是 644" test "$rc" -eq 0 -a "$(stat -c %a "$R/chrony.conf")" = 644 -a "$(grep -c '^server ntp' "$R/chrony.conf")" -ge 1
# 改 /etc 视图失败（目录只读）：如实报错退出 1，不留 .new
mkdir -p "$R/ro"; printf 'server a.google.com iburst\n' > "$R/ro/chrony.conf"; chmod 555 "$R/ro"
( cd "$PKG" && CJ_CHRONY_CONF="$R/ro/chrony.conf" CJ_MOUNTS="$R/mounts-plain" CJ_BACKUP_DIR="$R/cbk" CJ_TMPDIR="$R" run sh deploy-chrony-cn.sh 127.0.0.1 ) >"$R/out.txt" 2>&1; rc=$?
chmod 755 "$R/ro"
check "chrony-cn：改 /etc 视图失败 → 退出非 0、报错、不留 chrony.conf.new" test "$rc" -ne 0 -a -n "$(grep "改 $R/ro/chrony.conf 失败" "$R/out.txt")" -a ! -e "$R/ro/chrony.conf.new"
# overlay 成功路径不再经 /tmp 固定名中转：那里预先放一个符号链接，不能被跟着写
printf 'overlay /etc overlay rw 0 0\n' > "$R/mounts-ov"; mkdir -p "$R/chrony-cn.rootbind/etc"
printf 'server a.google.com iburst\n' > "$R/chrony-cn.rootbind/etc/chrony.conf"; printf 'server a.google.com iburst\n' > "$R/chrony.conf"
ln -s "$R/victim" "$R/chrony-cn.lower"
( cd "$PKG" && CJ_CHRONY_CONF="$R/chrony.conf" CJ_MOUNTS="$R/mounts-ov" CJ_BACKUP_DIR="$R/cbk" CJ_TMPDIR="$R" run sh deploy-chrony-cn.sh 127.0.0.1 ) >/dev/null 2>&1; rc=$?
check "chrony-cn overlay：不写 \$CJ_TMPDIR/chrony-cn.lower（预置的符号链接没被跟着写），当前视图照样同步" test "$rc" -eq 0 -a ! -e "$R/victim" -a "$(grep -c '^server ntp' "$R/chrony.conf")" -ge 1 -a "$(last_mount)" = "mount -o remount,ro /"
rm -f "$R/chrony-cn.lower"
echo TZ > "$R/Shanghai"; mkdir -p "$R/timezone-cn.rootbind/etc"; echo old > "$R/localtime"
( umask 000; cd "$PKG" && CJ_ZONEINFO="$R/Shanghai" CJ_LOCALTIME="$R/localtime" CJ_MOUNTS="$R/mounts-ov" CJ_BACKUP_DIR="$R/tbk" CJ_TMPDIR="$R" run sh deploy-timezone-cn.sh 127.0.0.1 ) >/dev/null 2>&1; rc=$?
sd_tz_perms() { test "$rc" -eq 0 && not_ww "$R/tbk" && not_ww "$R/tbk/timezone-cn.log"; }
check "timezone-cn umask 000：改前记录 timezone-cn.log 与备份目录不全局可写" sd_tz_perms
