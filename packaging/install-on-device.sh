#!/bin/sh
# ═══════════════════════════════════════════════════════════════════════════
# cang-jie 全项目一键安装器（设备端） —— reMarkable Paper Pro Move，root 运行。
#
# 本脚本随打包器 packaging/package.sh 生成的 tar 包一起分发，解包后就在包根
# （cangjie/install.sh）。它**只编排既有的、逐条真机验证过的子安装器**，自身不
# 复刻高危逻辑——把"重启不丢/误升级不砖"的保证落在下面四层上：
#
#   ① 固件门（新老设备通用）：逐字节比对 /usr/bin/xochitl 的 sha256 与
#      firmware-allowlist.txt。同一固件的任意设备哈希相同即命中→装；装错固件默认
#      拒绝（qmd 定位会错位崩溃），--force 可强装并自动登记当前哈希。
#   ② 重启不丢：systemd 单元 + .wants 链接写进 /usr(rootfs，普通重启不丢)；
#      cangjie-xovi-reenable.service 在 /home 加密盘挂载后重跑 xovi/start 重注入
#      xovi。→ 每次开机自恢复全部功能，无需手动。
#   ③ 误升级不变砖，最多丧失全部功能：
# · 绝不给 xochitl.service 加 /home 依赖（红线）；
#      · 写 /usr 前实检 dm-verity，激活即跳过（2026-08-16 "写/usr+重启→A/B回滚变砖"
#        的病根就是 verity 激活）；
#      · OTA 冲掉 rootfs → 单元没了 → xochitl 裸启原生 → 功能全丢但机器正常，
#        重跑本安装器即恢复。
#   ④ 一键：一个 tar 包 + 一条命令；幂等，可反复跑。
#
# 用法：
#   ./install.sh                装（固件门把关）
#   ./install.sh --force        固件门不命中也强装（自动登记当前哈希）
#   ./install.sh --no-systemd   只装功能层，不碰 /usr（不装开机持久；重启后手动 xovi/start）
# ═══════════════════════════════════════════════════════════════════════════
set -eu

HERE="$(cd "$(dirname "$0")" && pwd)"
ROOT=/home/root
DEST="$ROOT/weread"                 # reading + pkm 二进制落点（systemd 单元硬编码此路径）
XOVI="$ROOT/xovi"
ALLOW="$HERE/firmware-allowlist.txt"
XOCHITL=/usr/bin/xochitl
SYSD=/usr/lib/systemd/system
# 开机自恢复单元集（权威表 = restore-after-ota.sh；wr-renew.service 是 static、不建 .wants）
UNITS="cangjie-xovi-reenable.service wr-serve.service wr-renew.service wr-renew.timer cj-stars.service"
WANTS_MU="cangjie-xovi-reenable.service wr-serve.service cj-stars.service"   # multi-user.target.wants
WANTS_TM="wr-renew.timer"                                                    # timers.target.wants

FORCE=0
DO_SYSTEMD=1
for a in "$@"; do
    case "$a" in
        --force) FORCE=1 ;;
        --no-systemd) DO_SYSTEMD=0 ;;
        *) echo "!! 未知参数：$a"; exit 2 ;;
    esac
done

echo "═══════════════════════════════════════════════════"
echo "  cang-jie 全项目一键安装（设备端编排）"
echo "═══════════════════════════════════════════════════"

# ── 0. 前置 ──────────────────────────────────────────────────────────────
[ "$(id -u)" = "0" ] || { echo "!! 需要 root 运行"; exit 1; }
[ -f "$HERE/ime/install.sh" ] || { echo "!! 包不完整：缺 ime/install.sh"; exit 1; }
[ -f "$XOVI/xovi.so" ] || { echo "!! 缺 $XOVI/xovi.so —— 先跑：vellum add xovi qt-resource-rebuilder"; exit 1; }

# ── 1. 固件门（rmtool 范式：架构 → 平台 → 版本 → xochitl 逐字节哈希）──────────
ARCH="$(uname -m)"
[ "$ARCH" = "aarch64" ] || { echo "!! 架构 $ARCH != aarch64，拒绝"; exit 1; }
MACHINE="$(cat /sys/devices/soc0/machine 2>/dev/null || echo '?')"
FWVER="$(cat /etc/version 2>/dev/null || echo '?')"
[ -f "$XOCHITL" ] || { echo "!! 缺 $XOCHITL"; exit 1; }
command -v sha256sum >/dev/null 2>&1 || { echo "!! 设备无 sha256sum，无法核验固件"; exit 1; }
CUR="$(sha256sum "$XOCHITL" 2>/dev/null | cut -d' ' -f1)"
[ -n "$CUR" ] || { echo "!! xochitl sha256 计算失败"; exit 1; }
echo "-- 设备：machine=$MACHINE  /etc/version=$FWVER"
echo "-- xochitl sha256=$CUR"

if grep -qi "^$CUR" "$ALLOW" 2>/dev/null; then
    LABEL="$(grep -i "^$CUR" "$ALLOW" | head -n1 | sed 's/^[0-9a-fA-F]*[[:space:]]*//')"
    echo "✓ 固件门命中白名单：$LABEL"
elif [ "$FORCE" = "1" ]; then
    echo "⚠ 固件指纹不在白名单 —— --force 强装，登记当前哈希并交给设备端多层 fail-safe 兜底。"
    printf '%s  forced-%s machine=%s ver=%s\n' "$CUR" "$(date +%Y%m%d)" "$MACHINE" "$FWVER" >> "$ALLOW"
else
    echo "✋ 固件指纹不在白名单——本包未在该固件验证过，qmd 注入定位可能错位。"
    echo "   · 确认设备就是受支持固件、想强装：加 --force（自动登记 + fail-safe 兜底）"
    echo "   · 或换匹配固件的安装包。中止（未做任何改动）。"
    exit 1
fi

# ── 2. 备份（改设备前先备份，工程纪律）────────────────────────────────────
BK="$ROOT/cangjie-backups/pkg-$(date +%Y%m%d-%H%M%S)"
mkdir -p "$BK"
[ -f "$XOVI/extensions.d/cangjie-langhook.so" ] && cp "$XOVI/extensions.d/cangjie-langhook.so" "$BK/" 2>/dev/null || true
for u in $UNITS; do [ -f "$SYSD/$u" ] && cp "$SYSD/$u" "$BK/" 2>/dev/null || true; done
echo "-- 已备份现有 .so/单元 → $BK"

# ── 3. 第 1 层：中文化 + xovi + qmd + 字体 + UI 汉化（复用真机验证过的 install.sh）──
# 它自带 set -eu、CJK 预检、hashtab 重建、xovi/start 应用、健康检查，失败即非零退出。
echo
echo "── 第 1 层：中文化/输入法/字体/qmd/汉化（ime/install.sh）──"
sh "$HERE/ime/install.sh" || { echo "!! 第 1 层失败，中止（reading/pkm 未部署）。"; exit 1; }

# ── 4. 第 2 层：reading（墨香）+ pkm（★待办）二进制 → /home/root/weread ─────────
echo
echo "── 第 2 层：reading + pkm 二进制 → $DEST ──"
mkdir -p "$DEST"
if [ -d "$HERE/bin" ]; then
    for b in "$HERE"/bin/*; do
        [ -f "$b" ] || continue
        cp "$b" "$DEST/" && chmod +x "$DEST/$(basename "$b")"
    done
    # shellcheck disable=SC2012  # 仅列出我们自己命名的二进制（无特殊字符），ls 足够
    echo "-- 已部署：$(ls "$HERE/bin" | tr '\n' ' ')"
else
    echo "-- （包内无 bin/，跳过 reading/pkm 二进制）"
fi

# ── 5. 第 3 层：systemd 开机持久化（写 /usr rootfs；dm-verity 门 + 红线）────────
echo
echo "── 第 3 层：开机自恢复（systemd → /usr rootfs）──"
if [ "$DO_SYSTEMD" = "0" ]; then
    echo "-- --no-systemd：跳过。重启后需手动：$XOVI/start（或 vellum reenable）"
elif dmsetup ls --target verity 2>/dev/null | grep -q .; then
    # verity 激活时写 /usr + 重启 → root hash 变 → A/B 回滚变砖。绝不盲写。
    echo "✋ 检测到 dm-verity 激活 —— 跳过写 /usr（不装开机持久，避免变砖）。"
    echo "   功能已装好，但重启后需手动 $XOVI/start 才恢复 xovi。换回 verity 固件请人工确认持久化方案。"
elif [ ! -d "$HERE/systemd" ]; then
    echo "-- （包内无 systemd/，跳过开机持久）"
else
    mount -o remount,rw / || { echo "!! remount rw / 失败"; exit 1; }
    (
        set -e
        cd "$SYSD"
        for u in $UNITS; do
            [ -f "$HERE/systemd/$u" ] && { cp "$HERE/systemd/$u" "$u"; chmod 644 "$u"; }
        done
        mkdir -p multi-user.target.wants timers.target.wants
        for u in $WANTS_MU; do ln -sf "../$u" "multi-user.target.wants/$u"; done
        for u in $WANTS_TM; do ln -sf "../$u" "timers.target.wants/$u"; done
        sync
    )
    mount -o remount,ro / || true
    systemctl daemon-reload
    echo "-- 单元 + .wants 已写入 /usr（普通重启不丢）："
    for u in $WANTS_MU $WANTS_TM; do
        t=multi-user.target.wants; case "$u" in *.timer) t=timers.target.wants;; esac
        # shellcheck disable=SC2012  # 固定单元名，仅打印链接确认，ls 足够
        ls -l "$SYSD/$t/$u" 2>/dev/null | sed "s|$SYSD/||" || true
    done
fi

# ── 5b. OTA 登录触发恢复钩子（挂 ~/.bashrc）──────────────────────────────────
# OTA 冲掉 /usr → 启动路径零残留、开机无法自动触发恢复；唯一存活 OTA 又会自动跑的点是
# root 的 SSH 交互登录（/etc/profile 无条件 source ~/.bashrc）。挂 case $- 守卫→仅交互触发，
# scp/非交互 ssh cmd 不动。恢复动作复用本 bundle 的 ota-recover.sh，详见其头注。写 /home、非
# 关键、不碰启动关键路径、不可能变砖。
echo
echo "── OTA 登录触发恢复钩子：挂接 ~/.bashrc ──"
if [ -f "$HERE/ota-recover.sh" ]; then
    chmod +x "$HERE/ota-recover.sh"
    BRC="$ROOT/.bashrc"
    [ -f "$BRC" ] && cp "$BRC" "$BK/.bashrc.pre-ota" 2>/dev/null || true
    # 幂等：先去旧块（同时更新指向本次 bundle 路径），再追加新块
    [ -f "$BRC" ] && sed -i '/# >>> cangjie-ota-recover >>>/,/# <<< cangjie-ota-recover <<</d' "$BRC" 2>/dev/null || true
    {
        echo "# >>> cangjie-ota-recover >>> (install.sh 维护，勿手改)"
        echo "case \"\$-\" in *i*) [ -x \"$HERE/ota-recover.sh\" ] && \"$HERE/ota-recover.sh\" || true ;; esac"
        echo "# <<< cangjie-ota-recover <<<"
    } >> "$BRC"
    echo "-- 已挂接（仅交互登录触发；scp/非交互不动）。OTA 后 SSH 进设备即提示自动恢复。"
else
    echo "-- （包内无 ota-recover.sh，跳过 OTA 恢复钩子）"
fi

# ── 6. 启动与 xochitl 无关的常驻服务（本体已由第 1 层 xovi/start 注入，此处不碰它）──
# cangjie-xovi-reenable 会重启 xochitl，留给下次开机触发，不在装机时打断当前会话。
echo
echo "── 启动常驻服务（不含 reenable：xochitl 已注入）──"
systemctl start cj-stars.service 2>/dev/null && echo "-- cj-stars（★待办）已起" || echo "-- cj-stars 未起（查 journalctl -u cj-stars）"
systemctl start wr-serve.service 2>/dev/null && echo "-- wr-serve（墨香面板 127.0.0.1:8777）已起" || echo "-- wr-serve 未起（缺 credentials.json 属正常，扫码登录后自恢复）"
systemctl start wr-renew.timer 2>/dev/null || true

# ── 7. 健康检查 ──────────────────────────────────────────────────────────
echo
echo "═══════════════════════════════════════════════════"
NEW_PID="$(systemctl show xochitl -p MainPID --value 2>/dev/null || echo 0)"
STATE="$(systemctl is-active xochitl 2>/dev/null || true)"
CJ="$(grep -c cangjie-langhook /proc/"$NEW_PID"/maps 2>/dev/null || echo 0)"
QR="$(grep -c qt-resource-rebuilder /proc/"$NEW_PID"/maps 2>/dev/null || echo 0)"
echo "  xochitl   : is-active=$STATE  PID=$NEW_PID"
echo "  注入      : cangjie=$CJ 段  qrr=$QR 段  (都期望 >0)"
echo "  常驻服务  : cj-stars=$(systemctl is-active cj-stars 2>/dev/null || echo ?)  wr-serve=$(systemctl is-active wr-serve 2>/dev/null || echo ?)"
echo "═══════════════════════════════════════════════════"
if [ "$STATE" = "active" ] && [ "${CJ:-0}" -gt 0 ] && [ "${QR:-0}" -gt 0 ]; then
    echo "✅ 安装完成（幂等，可重复跑）。"
    echo "   · 中文输入：文本框弹键盘 → 地球键切 简/繁·全拼/双拼"
    echo "   · 系统增强：设置 →「系统增强」面板逐项开关"
    echo "   · 墨香微信读书：浏览器开 http://<设备IP>:8777 扫码登录（生成 credentials.json 后 wr-serve/wr-renew 自恢复）"
    if [ "$DO_SYSTEMD" = "1" ]; then
        echo "   · 重启：开机自恢复全部功能（无需手动）。"
    fi
    echo "   · 误 OTA：功能全丢但不变砖，重跑本安装器即恢复。"
else
    echo "⚠️  健康检查未达预期。查 journalctl -u xochitl 与 /tmp/cj-hashtab.log。"
    echo "    备份在 $BK。"
    exit 1
fi
