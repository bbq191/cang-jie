#!/bin/sh
# ═══════════════════════════════════════════════════════════════════════════
# timezone-cn.sh —— reMarkable 设备默认时区设为 Asia/Shanghai（把 /etc/localtime 指向
# /usr/share/zoneinfo/Asia/Shanghai），写进 **rootfs 底层**（普通重启不丢；OTA 冲掉 rootfs
# 后重跑本脚本即可）。持久化技法跟 chrony-cn.sh 完全同构（同一个 /etc overlay 架构问题，
# 直接复用同一套已经真机验证过的 remount+bind 手法，不发明新机制）——两者拆成独立脚本只是
# 职责单一，install-all.sh 里各自可以单独 --skip。
#
# 用法：packaging/deploy-timezone-cn.sh [host]（install-all.sh 也是调它）。手动跑要先拼上设备侧函数库 devlib.sh
#   （改 rootfs 底层的 rw/bind 窗口在库里，见 cj_etc_lower_edit，2026-10-10 起与 chrony-cn.sh 共用）：
#     cat packaging/devlib.sh packaging/timezone-cn.sh | ssh root@10.11.99.1 sh -s
# 幂等：底层已指向 Asia/Shanghai 就不再写。
#
# ⚠️ 不直接指望 `timedatectl set-timezone`——即便这条命令存在，它改的也只是 /etc/localtime 这个
# overlay 上层（tmpfs）文件，不解决"重启就丢"的持久化问题，跟 chrony.conf 面对的是同一个架构
# 问题，所以改用 remount+bind 直接写 rootfs 底层。
#
# ⚠️ 前提：设备镜像里得真的有 /usr/share/zoneinfo/Asia/Shanghai 这份 tzdata——没有真机核实过
# 这台设备是否带（精简固件可能裁掉整个 zoneinfo），缺失时清楚警告并跳过（exit 0，不算失败，
# 不拖垮 install-all.sh 其它步骤）。
# ═══════════════════════════════════════════════════════════════════════════
set -u
umask 022   # 改前记录 timezone-cn.log 等生成文件的权限不随调用方 umask 走

# 下面几个路径只为本机模拟测试（packaging/tests）可覆盖，设备上一律用默认值
TARGET="${CJ_ZONEINFO:-/usr/share/zoneinfo/Asia/Shanghai}"
LOCALTIME="${CJ_LOCALTIME:-/etc/localtime}"
BK_DIR="${CJ_BACKUP_DIR:-/home/root/cangjie-backups}"
MOUNTS="${CJ_MOUNTS:-/proc/mounts}"

command -v cj_etc_lower_edit >/dev/null 2>&1 || { echo "!! 缺 devlib.sh：cat packaging/devlib.sh packaging/timezone-cn.sh | ssh root@10.11.99.1 sh -s"; exit 1; }
[ "$(id -u)" = "0" ] || { echo "!! 需 root"; exit 1; }
if [ ! -e "$TARGET" ]; then
    echo "⚠ 设备镜像没有 $TARGET（缺 tzdata？），跳过时区设置，不影响其它安装步骤"
    exit 0
fi

is_shanghai() { # $1=待测的 /etc/localtime 路径：符号链接指向 target，或内容与 target 逐字节相同
    if [ -L "$1" ]; then
        [ "$(readlink "$1")" = "$TARGET" ] && return 0
    fi
    [ -e "$1" ] && cmp -s "$1" "$TARGET"
}

prev_desc() { # $1=路径：人读的"改之前是什么"，写进备份记录
    if [ -L "$1" ]; then echo "symlink -> $(readlink "$1")";
    elif [ -e "$1" ]; then echo "regular file";
    else echo "(missing)"; fi
}

changed=0
# 在 rootfs 底层的 /etc 里改（cj_etc_lower_edit 已 remount rw + bind，CJ_LOWER=绑定点；结束后它负责卸 bind、恢复 ro）
edit_lower() {
    LOWER="$CJ_LOWER/etc/localtime"
    if is_shanghai "$LOWER"; then
        echo "-- rootfs 底层已是 Asia/Shanghai，跳过"
        return 0
    fi
    mkdir -p "$BK_DIR"
    echo "$(date +%Y%m%d-%H%M%S) /etc/localtime 改前：$(prev_desc "$LOWER")" >> "$BK_DIR/timezone-cn.log"
    # 失败要如实报错退出——旧版不看返回值，写失败也打印"已改"（2026-09-25 审计）
    { ln -sfn "$TARGET" "$LOWER" && sync; } || { echo "!! 改 rootfs 底层 /etc/localtime 失败（底层未动）"; return 1; }
    echo "-- rootfs 底层已改（记录见 $BK_DIR/timezone-cn.log）"
    changed=1
}
if grep -q " /etc overlay " "$MOUNTS"; then
    # ── overlay：改 rootfs 底层（dm-verity 激活时返回 3：只改下面的当前视图，重启会丢）──
    rc=0
    cj_etc_lower_edit timezone-cn edit_lower || rc=$?
    if [ "$rc" = 3 ]; then
        echo "   只改本次开机的 overlay 视图（重启会丢）"
    elif [ "$rc" != 0 ]; then
        exit 1
    fi
fi
# overlay 当前视图跟底层不一致（overlay 缓存旧值）→ 直接改当前视图让本次开机立即生效
if ! is_shanghai "$LOCALTIME"; then
    ln -sfn "$TARGET" "$LOCALTIME" && changed=1 && echo "-- 当前视图已改（立即生效）"
fi

echo "-- $(date '+%F %T %Z')"
if is_shanghai "$LOCALTIME"; then
    echo "✅ 时区已是 Asia/Shanghai"
    # ⚠️ 之前这行写成 `[ "$changed" = "1" ] && echo ...` 且是脚本最后一条语句——changed=0 时
    # `[ ]` 测试本身为假、且没有 set -e，脚本不会中断，但也没有后续语句再覆盖 $?，于是整个
    # 脚本以这条 `[ ]` 的非零退出码收尾：明明打印"✅ 已是目标时区"却整体判定失败，真机 install-
    # all.sh 实测踩过（changed=0 的幂等分支必现）。改成 if 分支+显式 exit 0，不依赖最后一条
    # 语句的隐式退出码。
    if [ "$changed" = "1" ]; then
        echo "   （首次生效或刚改，若发现服务里有缓存旧时区的进程，建议重启确认）"
    fi
    exit 0
else
    echo "⚠ 时区设置后校验仍不是 Asia/Shanghai，检查上面输出"; exit 2
fi
