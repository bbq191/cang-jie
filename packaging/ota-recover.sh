#!/bin/sh
# ═══════════════════════════════════════════════════════════════════════════
# cangjie OTA 登录触发恢复 —— reMarkable，由 ~/.profile 在**交互登录**时以子进程调用。
#
# 【为什么是登录触发而非开机服务】OTA 冲掉 /usr(rootfs) → 我们全部 systemd 单元 +
#   任何"开机检测服务"一起没了，启动路径零残留（这正是「不变砖」的来源），故**开机无法
#   自动触发**。唯一存活 OTA 又会被 stock 系统自动执行、且落在存活的 /home 上的点，就是
#   root 的 SSH 登录脚本。你 OTA 后本就 SSH 进来恢复——这里把它变成"进来就自动恢复"。
#
# 【三重守卫】① 仅交互（.profile 的 case $- 判定后才调本脚本，本脚本作子进程跑，故不自查
#   $-）；② 疑似 OTA 才动（xochitl 哈希 ∉ 白名单 且 xovi 确未注入=功能真失效）；③ 每新固件
#   只自动尝一次（按哈希落标记，避免不兼容固件下每次登录/启动反复重砸 xochitl）。
#
# 【关键正确性】OTA 后 hashtab 存活但**过时**（新固件 QML 布局变了）→ 恢复前必须删它、让
#   install 按新固件重建，否则拿旧表注入错位。恢复走 install.sh --force（新固件不在白名单，
#   强装并登记；靠 _xovi_shouldLoad/qrr-failsafe 多层兜底，最坏"没恢复但不砖"）。
#
# ⚠ 本脚本会重启 xochitl + 重建 hashtab（约 1-2 分钟），只在确认 OTA 时、每固件一次。
# ═══════════════════════════════════════════════════════════════════════════
set -u

HERE="$(cd "$(dirname "$0")" && pwd)"   # 恢复包所在目录（.bashrc 钩子传绝对路径来调本脚本）
ALLOW="$HERE/firmware-allowlist.txt"
XOCHITL=/usr/bin/xochitl
STATE=/home/root/.local/share/cangjie-ime
HASHTAB=/home/root/xovi/exthome/qt-resource-rebuilder/hashtab

# 守卫1：恢复环境齐全才继续（缺 bundle/白名单/sha256sum 一律静默退出，绝不打扰正常登录）
[ -f "$HERE/install.sh" ] && [ -f "$ALLOW" ] || exit 0
command -v sha256sum >/dev/null 2>&1 || exit 0
CUR="$(sha256sum "$XOCHITL" 2>/dev/null | cut -d' ' -f1)"
[ -n "$CUR" ] || exit 0

# 守卫2a：当前 xochitl 已在白名单（没 OTA，或恢复后已登记）→ 静默退出
grep -qi "^$CUR" "$ALLOW" 2>/dev/null && exit 0

# 守卫2b：哈希虽不在白名单，但 xovi 已注入（功能其实在）→ 无需恢复，静默退出
PID="$(systemctl show xochitl -p MainPID --value 2>/dev/null || echo 0)"
if [ "${PID:-0}" != "0" ] && grep -q cangjie-langhook "/proc/$PID/maps" 2>/dev/null; then
    exit 0
fi

# 守卫3：本固件已自动尝试过 → 不再重砸（失败后由用户手动 cangjie/install.sh）
MARK="$STATE/ota-recover.attempted.$CUR"
[ -f "$MARK" ] && exit 0

# —— 到这里：疑似 OTA（哈希新 + 功能失效 + 本固件未尝试过）——
echo ""
echo "══════════════════════════════════════════════════════════"
echo "  ⚠ cangjie：检测到固件疑似 OTA —— 增强功能已失效"
echo "     xochitl 新指纹 sha256=$CUR"
echo "     /home/root/cangjie/ 恢复包在场，将自动重装恢复。"
echo "     · 会重启一次 xochitl + 重建 qmd hashtab（约 1-2 分钟）"
echo "     · 本固件只自动尝试这一次（失败后手动 cangjie/install.sh --force）"
echo "     10 秒后开始 —— 按 Ctrl-C 取消（取消则下次登录再问）。"
echo "══════════════════════════════════════════════════════════"

# 显式取消：Ctrl-C → 打印提示 → 在写标记/动手之前退出（不依赖 shell 默认信号行为；
# 未落标记 → 下次登录再问）。倒计时结束后清 trap，避免恢复过程中误 abort。
trap 'printf "\n已取消（下次登录再问）。\n"; exit 130' INT

i=10
while [ "$i" -gt 0 ]; do
    printf "\r  倒计时 %2ds …" "$i"
    sleep 1
    i=$((i - 1))
done
printf "\r                 \n"
trap - INT   # 倒计时已过、进入恢复：还原默认信号处理，不让 install 半途被 cancel-trap 打断

# 落标记（此刻起视为"本固件已尝试"，即便下面失败也不每次登录重弹；Ctrl-C 在此之前=不落标记、下次再问）
mkdir -p "$STATE"
: > "$MARK"

# 删过时 hashtab → 让 ime/install.sh 按新固件重建（拿旧表注入新 QML 会错位）
rm -f "$HASHTAB"

echo "-- 开始恢复：cangjie/install.sh --force（新固件强装，多层 fail-safe 兜底）"
sh "$HERE/install.sh" --force
