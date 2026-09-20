#!/bin/sh
# host 侧一键探测+打上 appload 3.28 兼容补丁（appload_patch_328.py，见同目录
# appload-qmd-PROVENANCE.md）。**不接入 install-all.sh 的自动编排**——这个补丁改的是设备端
# 已装好的 appload.so 二进制内容，这台写代码的机器上没有真机可连，也没有真实的 appload.so
# 试跑过，字节替换逻辑只在构造出的假二进制片段上过了 host 单测（见 packaging/tests/
# test_appload_patch_328.py），对真实 appload.so 有没有效还没有真机验证过——按工程纪律
# "改变设备行为的改动，没在真机上跑通之前不说已完成"，先作为独立手动步骤存在，真机验证过
# 之后再考虑要不要并入 install-all.sh。
#
# 流程：探测设备上 appload.so 的当前状态（unpatched/patched/unknown）→ 只在 unpatched 时
# 备份+打补丁+传回+校验 → 提示需要重启 xochitl 才会在运行中的 xochitl 里生效（xovi 已生效时用 systemctl restart xochitl，不要 xovi/start）。
#
# 前置：设备已 vellum add appload（`/home/root/xovi/extensions.d/appload.so` 存在）。
#
# 用法：./deploy-appload-patch.sh [host]      host 默认 10.11.99.1
#
# 2026-09-20 改动（脚本审计 H3）：旧版直接 scp 覆盖 extensions.d/appload.so，md5 不符时坏 .so 已留在
# extensions.d 里，下次 xochitl 启动会加载它（崩溃循环风险），且原地写 xochitl 已映射的库本身也危险。
# 现在：补丁版先推到暂存目录（不在 extensions.d，因为 xovi 会把该目录下任意文件当扩展加载）并校验 md5，
# 设备端先备份原文件进 cangjie-backups，再原子 rename 覆盖；落位后再核对一次 md5，不符则从备份还原。
set -eu
cd "$(dirname "$0")"
# shellcheck disable=SC1091
. ./lib.sh
# shellcheck disable=SC2034  # HOST 由 lib.sh 的 rssh/rscp/dev_script 使用
HOST="${1:-10.11.99.1}"
REMOTE_SO=/home/root/xovi/extensions.d/appload.so
STAGE_SO="$CJ_STAGE_REMOTE/appload.so.new"
LOCAL_TMP="$(mktemp -d)"
trap 'rm -rf "$LOCAL_TMP"' EXIT

echo "== 探测设备端 appload.so =="
if ! rssh "[ -f $REMOTE_SO ]"; then
    echo "-- 设备没装 appload（$REMOTE_SO 不存在，先 vellum add appload）——跳过，非失败"
    exit 0
fi

echo "== 拉取设备端 appload.so 到本地判定状态 =="
rscp "root@$HOST:$REMOTE_SO" "$LOCAL_TMP/appload.so"

STATUS="$(python3 appload_patch_328.py --check "$LOCAL_TMP/appload.so")"
echo "-- 当前状态：$STATUS"
case "$STATUS" in
    patched)
        echo "✅ 已经是打过 3.28 补丁的版本，不需要再打（幂等，非失败）"
        exit 0
        ;;
    unknown)
        echo "!! 这份 appload.so 既不是已知的 v0.5.3 原始版本，也不是已知的打过补丁的版本——"
        echo "   可能是不同的 appload 版本，本工具只认 v0.5.3，不碰这个文件，避免在不确定的"
        echo "   情况下写坏它。见 appload-qmd-PROVENANCE.md。"
        exit 1
        ;;
    unpatched) ;;
    *)
        echo "!! 未知返回：$STATUS"
        exit 1
        ;;
esac

echo "== 打补丁（本地）=="
python3 appload_patch_328.py "$LOCAL_TMP/appload.so" "$LOCAL_TMP/appload.so.patched"

echo "== 推送补丁版到设备暂存目录（不在 extensions.d；md5 校验，不通过则最终位置从未被碰）=="
push_verified "$LOCAL_TMP/appload.so.patched" "$STAGE_SO"
LOCAL_MD5="$(md5_local "$LOCAL_TMP/appload.so.patched")"

echo "== 设备端：备份原文件到 cangjie-backups/ 再原子替换（绝不放 extensions.d/ 本身，见 工程纪律 红线）=="
dev_script "$REMOTE_SO" "$STAGE_SO" "$LOCAL_MD5" <<'DEVICE_SCRIPT'
set -eu
SO="$1"; STG="$2"; WANT="$3"
[ -f "$STG" ] || { echo "!! 暂存文件缺失"; exit 1; }
cj_backup_file "$SO"
BK="$(ls -1 "$CJ_BACKUP_DIR" | sort | grep "^appload\.so\.bak\.pre-" | tail -n 1)"
cj_safe_replace "$STG" "$SO" "$(dirname "$STG")" 644
GOT="$(md5sum "$SO" | awk '{print $1}')"
if [ "$GOT" != "$WANT" ]; then
    echo "!! 落位后 md5 不符（$GOT vs $WANT）——从备份 $BK 还原"
    [ -n "$BK" ] && cj_safe_replace "$CJ_BACKUP_DIR/$BK" "$SO" "$(dirname "$STG")" 644
    exit 1
fi
rm -f "$STG"; rmdir "$(dirname "$STG")" 2>/dev/null || true
echo "-- md5 一致，appload.so 已替换（备份：$CJ_BACKUP_DIR/$BK）"
DEVICE_SCRIPT

echo "== 完成（落盘）=="
echo "   要在当前运行中的 xochitl 里生效，需要重启 xochitl——xovi 已生效时用 systemctl restart xochitl"
echo "   （**不要** xovi/start：它在 xovi 已生效的 xochitl 上会 SEGV 整机重启，2026-09-20 事故），"
echo "   或跑 packaging/deploy-xovi-apply.sh（会自动判定并先提示打断阅读）。不在这里自动重启——见 install-all.sh"
echo "   头注为什么不让每一步各自触发重启。"
echo "   ⚠ 真机验证清单（还没做过，需要用户确认）：重启 xochitl 后健康（is-active/NRestarts/MainPID）"
echo "     + journalctl 里能看到 appload 自己的 'Loaded external AppLoad hooks in main UI' 成功信号"
echo "     + Sidebar 里挂的入口点了有反应。"
