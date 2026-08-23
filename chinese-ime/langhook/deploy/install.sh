#!/bin/sh
# cangjie-ime 安装脚本（vellum-xovi 0.3.3 结构）—— reMarkable Paper Pro，root 运行。
#
# 【2026-08-16 重写：改用 vellum 的官方 xovi 引导机制，彻底弃掉 /usr drop-in】
# 旧版把 xovi 启动配置写 /usr/lib（rootfs），dm-verity 下"写 /usr + 完整重启 → root hash
# 变 → A/B 回滚"（真机踩实两次，正是设备被搞成裸机的原因）。新版不碰 /usr：
#   - cangjie 扩展/词典/qmd 全放 /home（非 verity，重启不丢）。
#   - 引导配置走 vellum-xovi 自己的机制：源在 /home/root/xovi/services/xochitl.service/
#     （持久），`xovi/start` 把它 cp 成 /etc/.../xochitl.service.d/ 的 tmpfs 挂载
#     （含 LD_PRELOAD=xovi.so + XOVI_ROOT + qt-resource-rebuilder 的 QML env）。
#     只写 /etc tmpfs、不碰 /usr → 无 verity 回滚。
#
# 前置（本脚本不装，vellum 官方包）：
#   vellum add xovi qt-resource-rebuilder
#
# 【重启后持久化】/etc tmpfs 重启即清、无开机自动服务 → 重启后需手动恢复：
#   /home/root/xovi/start        （或 vellum reenable）
# （真机多为休眠而非重启，休眠 tmpfs 不丢，此负担在实际使用中很小。）
#
# 【范围】本脚本只装"中文输入法"（键盘+候选+词典）。UI 界面汉化（zh_*.qm 需进 /usr/share，
#   是 verity 回滚雷）需另做 verity 安全方案，本脚本不含。
set -eu

HERE="$(cd "$(dirname "$0")" && pwd)"
PAYLOAD="$HERE/payload"
[ -d "$PAYLOAD" ] || PAYLOAD="$HERE"          # 松散布局兜底
ROOT=/home/root
XOVI="$ROOT/xovi"
EXTDIR="$XOVI/extensions.d"                              # 顶层（rebuild_hashtable 用这个）
SVCEXT="$XOVI/services/xochitl.service/extensions.d"     # XOVI_ROOT 运行时实际加载这个
QRR="$XOVI/exthome/qt-resource-rebuilder"               # qmd 注入目录
HASHTAB="$QRR/hashtab"
FONTS="$ROOT/.local/share/fonts"
DATADIR="$ROOT/.local/share/cangjie-ime"                # 词典 XDG（= src/hook_init.c 的 CJ_DATA_DIR）

echo "== cangjie-ime 安装（vellum-xovi 结构，不碰 /usr）=="

# ---- 前置检查 ----
[ "$(id -u)" = "0" ] || { echo "!! 需要 root 运行"; exit 1; }
[ -f "$XOVI/xovi.so" ] || { echo "!! 没找到 $XOVI/xovi.so —— 先跑：vellum add xovi"; exit 1; }
[ -f "$EXTDIR/qt-resource-rebuilder.so" ] || {
    echo "!! 没找到 qt-resource-rebuilder —— 先跑：vellum add qt-resource-rebuilder"; exit 1; }
[ -f "$XOVI/start" ] || { echo "!! 没找到 $XOVI/start —— xovi 安装异常"; exit 1; }

# ---- 1. cangjie 扩展 -> 两处 extensions.d（顶层 + services，保险）；词典 -> XDG ----
echo "-- 拷 cangjie-langhook.so -> extensions.d（顶层 + services）"
mkdir -p "$SVCEXT"
cp "$PAYLOAD/cangjie-langhook.so" "$EXTDIR/cangjie-langhook.so"
cp "$PAYLOAD/cangjie-langhook.so" "$SVCEXT/cangjie-langhook.so"
# 清旧崩溃标记（xovi 会把 extensions.d 里任意文件当扩展加载，重复注册是致命错）
rm -f "$EXTDIR/cangjie-langhook.so.crashed" "$SVCEXT/cangjie-langhook.so.crashed"
echo "-- 拷 5 个词典 blob -> $DATADIR/"
mkdir -p "$DATADIR"
for b in dict.bin dict.zh_tw.bin dict_jianpin.bin dict_jianpin.zh_tw.bin english.bin; do
    [ -f "$PAYLOAD/$b" ] && cp "$PAYLOAD/$b" "$DATADIR/$b"
done

# ---- 2. 字体 + fontconfig + CJK 覆盖预检（借鉴 rmtool has_cjk_font，防满屏豆腐块）----
echo "-- 拷字体 -> $FONTS/"
mkdir -p "$FONTS"
# 输入法/UI CJK 字体 + 阅读字体增强的 3 个可选字体（家族名见 add-reading-fonts.qmd）：
#   LXGWWenKaiMonoGBScreen=霞鹜文楷、LXGWNeoZhiSongScreenFull=霞鹜新致宋、KF_Readerly=拉丁阅读体。
for f in "$PAYLOAD"/LXGWNeoXiHeiScreenFull.ttf "$PAYLOAD"/LXGWWenKai-Regular.ttf \
         "$PAYLOAD"/LXGWWenKaiMonoGBScreen.ttf "$PAYLOAD"/LXGWNeoZhiSongScreenFull.ttf \
         "$PAYLOAD"/KF_Readerly-Regular.ttf "$PAYLOAD"/KF_Readerly-Bold.ttf \
         "$PAYLOAD"/KF_Readerly-Italic.ttf "$PAYLOAD"/KF_Readerly-BoldItalic.ttf; do
    [ -f "$f" ] && cp "$f" "$FONTS/"
done
if [ -f "$PAYLOAD/fontconfig-cangjie.conf" ]; then
    mkdir -p "$ROOT/.config/fontconfig"
    cp "$PAYLOAD/fontconfig-cangjie.conf" "$ROOT/.config/fontconfig/fonts.conf"
fi
command -v fc-cache >/dev/null 2>&1 && fc-cache -f >/dev/null 2>&1 || true
if command -v fc-list >/dev/null 2>&1; then
    CJK_COUNT="$(fc-list --format='%{file}\n' ':lang=zh-cn' 2>/dev/null | grep -c . || true)"
    [ "${CJK_COUNT:-0}" -eq 0 ] && { echo "!! 无覆盖 zh-cn 的字体，会满屏豆腐块，中止。"; exit 1; }
    echo "   CJK 字体预检通过：$CJK_COUNT 个"
fi

# ---- 3. qmd -> qt-resource-rebuilder exthome ----
# 候选栏（必需）+ 设置页中文键盘入口 + 键盘 Mono +「系统增强 → 阅读增强」整套：
#   reading-qol-config = 共享配置(读 reading-qol.json，1.5s 轮询传播)；tap-page-turn=点击翻页；
#   fast-mono-reading=快速黑白；page-refresh=按章/按页清残影(彩屏&黑白)；add-reading-fonts=字体菜单+3；
#   settings-reading-enhance=设置页「系统增强」控制面板。规范源在 xovi-extensions/reading-qol|font-menu。
echo "-- 拷 candidatebar.qmd（候选栏，必需）-> $QRR/"
cp "$PAYLOAD/candidatebar.qmd" "$QRR/candidatebar.qmd"
for q in reading-qol-config.qmd tap-page-turn.qmd fast-mono-reading.qmd page-refresh.qmd \
         keyboard-mono.qmd add-reading-fonts.qmd settings-keyboard-zh.qmd settings-reading-enhance.qmd; do
    [ -f "$PAYLOAD/$q" ] && cp "$PAYLOAD/$q" "$QRR/$q"
done
# 清掉可能残留的旧字体菜单 qmd（已被 add-reading-fonts.qmd 取代）
rm -f "$QRR/add-lxgw-font.qmd"

# ---- 3a. 阅读增强初始配置（首次装才建，不覆盖用户已有设置）----
RQOL="$DATADIR/reading-qol.json"
if [ ! -s "$RQOL" ]; then
    echo "-- 建阅读增强初始配置（全 OFF）-> $RQOL"
    printf '%s' '{"tapPageTurn":false,"fastMono":false,"refresh":false,"refreshByChapter":false,"refreshEvery":15,"fontEnhance":false}' > "$RQOL"
fi

# ---- 3c. qrr 崩溃自愈 fail-safe（xovi pre-start：崩溃循环时自动隔离阅读增强 qmd，防砖）----
# 每次 xovi/start 检查上一个 boot 的 xochitl 崩溃签名(journald 持久)，>=3 次就把 6 个阅读增强 qmd
# 移出 qrr（只动我们自己的），下一个 boot 恢复到"输入法可用"健康态。详见脚本头注释。
PRESTART="$XOVI/scripts/pre-start"
if [ -f "$PAYLOAD/cangjie-qrr-failsafe.sh" ]; then
    echo "-- 装 qrr 崩溃自愈 fail-safe -> $PRESTART/"
    mkdir -p "$PRESTART"
    cp "$PAYLOAD/cangjie-qrr-failsafe.sh" "$PRESTART/cangjie-qrr-failsafe.sh"
    chmod +x "$PRESTART/cangjie-qrr-failsafe.sh"
fi
# 重装即视为"已修好"：清 fail-safe 触发标记 + 隔离区（上面 qmd 已重新铺好）
rm -f "$DATADIR/qrr-failsafe.TRIGGERED"
rm -rf "$DATADIR/qrr-failsafe.quarantine"

# ---- 3b. UI 界面汉化（verity 安全，bind-mount 覆盖翻译目录，让 xochitl 原生加载 zh）----
# 股票固件 /usr 翻译目录只有 de/en/es/fr、无 zh 且 /usr 只读加不进。用 bind-mount 把 /home
# 的完整目录（原版 + 我们的 zh）覆盖上去——运行时 VFS 挂载、不改 /usr 块、无 verity 回滚。
# 挂载放 xovi pre-start 脚本，每次 xovi/start（含重启后手动 reenable）自动重新应用。
# 这样 xochitl 自己 load(zh_CN) 成功→原生提交语言+全界面重译+语言选择器正确（比 hook 干净）。
TRANS_HOME="$DATADIR/translations"
echo "-- 拷 zh .qm -> $TRANS_HOME/"
mkdir -p "$TRANS_HOME"
for q in reMarkable_zh_CN.qm reMarkable_zh_TW.qm reMarkable_zh_HK.qm; do
    [ -f "$PAYLOAD/$q" ] && cp "$PAYLOAD/$q" "$TRANS_HOME/$q"
done
if [ -f "$TRANS_HOME/reMarkable_zh_CN.qm" ]; then
    echo "-- 装 UI 汉化 bind-mount pre-start 脚本"
    PRESTART="$XOVI/scripts/pre-start"
    mkdir -p "$PRESTART"
    [ -f "$PAYLOAD/cangjie-xlate-bindmount.sh" ] && cp "$PAYLOAD/cangjie-xlate-bindmount.sh" "$PRESTART/"
    chmod +x "$PRESTART/cangjie-xlate-bindmount.sh" 2>/dev/null || true
else
    echo "-- （无 zh .qm，跳过 UI 汉化 bind-mount）"
fi

# ---- 4. rebuild_hashtable（qrr QMLDiff 定位靠它；官方脚本有交互提示，这里非交互复刻）----
if [ -s "$HASHTAB" ] && [ "${1:-}" != "--rehash" ]; then
    echo "-- hashtab 已存在，跳过重建（--rehash 强制）"
else
    echo "-- 重建 qrr hashtab（停 xochitl，特殊启动 dump QML，约 60-90s）"
    XR=/tmp/xovi-hashtab
    systemctl stop xochitl; sleep 1
    for p in $(pidof xochitl); do kill -15 "$p"; done; sleep 2
    rm -rf "$XR"; mkdir -p "$XR/extensions.d"
    ln -sf "$EXTDIR/qt-resource-rebuilder.so" "$XR/extensions.d/"
    rm -f "$HASHTAB"
    QMLDIFF_HASHTAB_CREATE="$HASHTAB" QML_DISABLE_DISK_CACHE=1 \
        LD_PRELOAD="$XOVI/xovi.so" XOVI_ROOT="$XR" \
        setsid /usr/bin/xochitl </dev/null >/tmp/cj-hashtab.log 2>&1 &
    i=0
    while [ "$i" -lt 60 ]; do
        sleep 2; i=$((i+1))
        grep -q "Hashtab saved to" /tmp/cj-hashtab.log 2>/dev/null && break
    done
    for p in $(pidof xochitl); do kill -15 "$p"; done; sleep 2
    rm -rf "$XR"
    [ -s "$HASHTAB" ] || { echo "!! hashtab 重建失败，查 /tmp/cj-hashtab.log"; exit 1; }
    echo "   hashtab 就绪（$(wc -c < "$HASHTAB") 字节）"
fi

# ---- 5. 应用引导（vellum-xovi start：写 /etc tmpfs + daemon-reload + 重启 xochitl）----
echo "-- 应用 xovi/start（/etc tmpfs 引导，不碰 /usr）"
OLD_PID="$(systemctl show xochitl -p MainPID --value 2>/dev/null || echo 0)"
"$XOVI/start"
sleep 5

# ---- 6. 健康检查 ----
STATE="$(systemctl is-active xochitl 2>/dev/null || true)"
NEW_PID="$(systemctl show xochitl -p MainPID --value 2>/dev/null || echo 0)"
NREST="$(systemctl show xochitl -p NRestarts --value 2>/dev/null || echo '?')"
CJ="$(grep -c cangjie-langhook /proc/"$NEW_PID"/maps 2>/dev/null || echo 0)"
QR="$(grep -c qt-resource-rebuilder /proc/"$NEW_PID"/maps 2>/dev/null || echo 0)"
echo "=================================================="
echo "  is-active : $STATE   (期望 active)"
echo "  MainPID   : $OLD_PID -> $NEW_PID   (期望有变化)"
echo "  NRestarts : $NREST   (期望 0/不增)"
echo "  cangjie 加载 : $CJ 段   qrr 加载 : $QR 段   (都期望 >0)"
echo "=================================================="
if [ "$STATE" = "active" ] && [ "$NEW_PID" != "0" ] && [ "${CJ:-0}" -gt 0 ] && [ "${QR:-0}" -gt 0 ]; then
    echo "✅ 安装完成。文本框弹键盘 → 地球切 简体全拼/繁體全拼/简体双拼/繁體双拼。"
    echo "⚠️  重启后需手动恢复：/home/root/xovi/start（或 vellum reenable）"
else
    echo "⚠️  健康检查未达预期。查 journalctl -u xochitl 与 /tmp/cj-hashtab.log。"
    exit 1
fi
