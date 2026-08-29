#!/bin/sh
# ═══════════════════════════════════════════════════════════════════════════
# cang-jie 全项目一键打包器（宿主机侧） —— 组装自包含安装 tar 包。
#
# 产物：dist/cangjie-full-<固件标签>-<日期>.tar.gz，解开是：
#   cangjie/
#     install.sh              设备端编排器（packaging/install-on-device.sh）
#     uninstall.sh            设备端卸载器（packaging/uninstall-on-device.sh）
#     firmware-allowlist.txt  固件门白名单（sha256 → 版本标签）
#     MANIFEST.txt            清单：每文件 sha256 + 构建信息
#     ime/                    第 1 层：真机验证过的中文化安装器 + 全部载荷
#       install.sh  uninstall.sh  payload/{.so,词典,字体,fontconfig,qmd,.qm,pre-start}
#     bin/                    第 2 层：reading + pkm 的 aarch64 全静态二进制
#     systemd/                第 3 层：开机自恢复单元（写 /usr rootfs）
#
# 载荷清单**对齐 chinese-ime/langhook/deploy/install.sh 的拷贝段**（权威源，勿手抄漂移）。
#
# 用法：
#   ./package.sh            缺 Rust 二进制则自动 build，再打包
#   ./package.sh --rebuild  强制重编 Rust 二进制
#   ./package.sh --no-build 不编，缺二进制直接报错（纯重打包既有产物）
# ═══════════════════════════════════════════════════════════════════════════
set -eu

HERE="$(cd "$(dirname "$0")" && pwd)"
REPO="$(cd "$HERE/.." && pwd)"
cd "$REPO"

TARGET=aarch64-unknown-linux-musl
BUILD=auto
for a in "$@"; do
    case "$a" in
        --rebuild) BUILD=force ;;
        --no-build) BUILD=none ;;
        *) echo "!! 未知参数：$a"; exit 2 ;;
    esac
done

# ── 源路径 ────────────────────────────────────────────────────────────────
IME_DEPLOY="$REPO/chinese-ime/langhook/deploy"
FONTS_DIR="$REPO/chinese-ime/fonts"
DICT_BUILD="$REPO/chinese-ime/pinyin-engine/c/build"
QOL="$REPO/xovi-extensions/reading-qol"
FONTMENU="$REPO/xovi-extensions/font-menu"
TRANS="$REPO/chinese-ime/translations"
READING_REL="$REPO/reading/device-rs/target/$TARGET/release"
PKM_REL="$REPO/knowledge/pkm/target/$TARGET/release"
SYSD_SRC="$REPO/reading/device-rs/systemd"

READING_BINS="wr-serve wr-download wr-renew wr-fetch"
PKM_BINS="cj-stars-daemon cj-stars"
UNITS="cangjie-xovi-reenable.service wr-serve.service wr-renew.service wr-renew.timer cj-stars.service"

# ── 助手 ──────────────────────────────────────────────────────────────────
MISSING_REQ=0
copy_req() { # 必需：缺则记错
    if [ -f "$1" ]; then cp "$1" "$2/"; else echo "  ✗ 缺（必需）：$1"; MISSING_REQ=1; fi
}
copy_opt() { # 可选：缺则提示（对齐 install.sh 的 [ -f ] && cp 容错）
    if [ -f "$1" ]; then cp "$1" "$2/"; else echo "  · 跳过（可选缺）：$(basename "$1")"; fi
}
find_font() { find "$FONTS_DIR" -iname "$1" -type f 2>/dev/null | head -n1; }

echo "═══ cang-jie 全项目打包 ═══"

# ── 1. Rust 二进制：按需交叉编译 ──────────────────────────────────────────
need_build=0
for b in $READING_BINS; do [ -f "$READING_REL/$b" ] || need_build=1; done
for b in $PKM_BINS;     do [ -f "$PKM_REL/$b" ]     || need_build=1; done
if [ "$BUILD" = "force" ] || { [ "$BUILD" = "auto" ] && [ "$need_build" = "1" ]; }; then
    echo "-- 交叉编译 reading + pkm（$TARGET 全静态）…"
    ( cd "$REPO/reading/device-rs" && sh ./build.sh )
    ( cd "$REPO/pkm" && sh ./build.sh )
elif [ "$BUILD" = "none" ] && [ "$need_build" = "1" ]; then
    echo "!! 缺 Rust 二进制且 --no-build。先跑 reading/device-rs/build.sh 与 knowledge/pkm/build.sh"
    exit 1
else
    echo "-- Rust 二进制已就绪，复用（--rebuild 强制重编）"
fi

# ── 2. 组暂存树 ───────────────────────────────────────────────────────────
STAGE="$(mktemp -d)"
trap 'rm -rf "$STAGE"' EXIT
PKG="$STAGE/cangjie"
PAY="$PKG/ime/payload"
mkdir -p "$PKG/ime" "$PAY" "$PKG/bin" "$PKG/systemd"

echo "-- 组 ime/（中文化层，载荷对齐 install.sh 拷贝段）"
copy_req "$IME_DEPLOY/install.sh"   "$PKG/ime"
copy_opt "$IME_DEPLOY/uninstall.sh" "$PKG/ime"

# 2a .so + 5 词典 blob（必需）
copy_req "$REPO/chinese-ime/langhook/cangjie-langhook.so" "$PAY"
for b in dict.bin dict.zh_tw.bin dict_jianpin.bin dict_jianpin.zh_tw.bin english.bin; do
    copy_req "$DICT_BUILD/$b" "$PAY"
done
# 2b 字体：核心 CJK 必需（否则设备端 CJK 预检 abort），其余可选
for f in LXGWNeoZhiSongScreenFull.ttf HanaMinB.ttf; do
    p="$(find_font "$f")"
    if [ -n "$p" ]; then cp "$p" "$PAY/"; else echo "  ✗ 缺（必需CJK）：$f"; MISSING_REQ=1; fi
done
for f in LXGWNeoXiHeiScreenFull.ttf LXGWWenKai-Regular.ttf LXGWWenKaiMonoGBScreen.ttf \
         KF_Readerly-Regular.ttf KF_Readerly-Bold.ttf KF_Readerly-Italic.ttf KF_Readerly-BoldItalic.ttf; do
    p="$(find_font "$f")"; [ -n "$p" ] && cp "$p" "$PAY/" || echo "  · 跳过（可选字体缺）：$f"
done
copy_opt "$IME_DEPLOY/fontconfig-cangjie.conf" "$PAY"
# 2c qmd（候选栏必需；其余系统增强/字体菜单/设置门户可选）
copy_req "$IME_DEPLOY/candidatebar.qmd" "$PAY"
copy_opt "$IME_DEPLOY/settings-keyboard-zh.qmd" "$PAY"
for q in reading-qol-config tap-page-turn fast-mono-reading page-refresh keyboard-mono settings-reading-enhance; do
    copy_opt "$QOL/$q.qmd" "$PAY"
done
copy_opt "$FONTMENU/add-reading-fonts.qmd" "$PAY"
# 2d pre-start 脚本（fail-safe / lo 别名 / 汉化 bind-mount）
for s in cangjie-qrr-failsafe.sh cangjie-lo-alias.sh cangjie-xlate-bindmount.sh; do
    copy_opt "$IME_DEPLOY/$s" "$PAY"
done
# 2e UI 汉化 .qm
for q in reMarkable_zh_CN.qm reMarkable_zh_TW.qm reMarkable_zh_HK.qm; do
    copy_opt "$TRANS/$q" "$PAY"
done

echo "-- 组 bin/（reading + pkm 二进制）"
for b in $READING_BINS; do copy_opt "$READING_REL/$b" "$PKG/bin"; done
for b in $PKM_BINS;     do copy_opt "$PKM_REL/$b"     "$PKG/bin"; done

echo "-- 组 systemd/（开机自恢复单元）"
for u in $UNITS; do copy_req "$SYSD_SRC/$u" "$PKG/systemd"; done

echo "-- 组包根（编排器 + 固件白名单）"
copy_req "$HERE/install-on-device.sh"   "$PKG" && mv "$PKG/install-on-device.sh"   "$PKG/install.sh"
copy_req "$HERE/uninstall-on-device.sh" "$PKG" && mv "$PKG/uninstall-on-device.sh" "$PKG/uninstall.sh"
copy_req "$HERE/ota-recover.sh"         "$PKG"   # OTA 登录触发恢复（install.sh 挂进 ~/.bashrc）
copy_req "$HERE/firmware-allowlist.txt" "$PKG"
chmod +x "$PKG/install.sh" "$PKG/uninstall.sh" "$PKG/ota-recover.sh" "$PKG/ime/install.sh" 2>/dev/null || true
[ -f "$PKG/ime/uninstall.sh" ] && chmod +x "$PKG/ime/uninstall.sh" || true

[ "$MISSING_REQ" = "0" ] || { echo "!! 有必需文件缺失，中止打包（见上 ✗）。"; exit 1; }

# ── 3. 生成 MANIFEST（provenance）───────────────────────────────────────────
FW_LABEL="$(grep -v '^#' "$PKG/firmware-allowlist.txt" | grep -m1 . | awk '{print $2}' 2>/dev/null || echo unknown)"
{
    echo "# cang-jie 全项目安装包清单"
    echo "# 构建时间：$(date '+%Y-%m-%d %H:%M:%S')"
    echo "# 目标固件：$FW_LABEL（固件门以 firmware-allowlist.txt sha256 为准）"
    echo "# git：$(git -C "$REPO" describe --always --dirty 2>/dev/null || echo n/a)"
    echo "#"
    echo "# sha256                                                            相对路径"
    ( cd "$PKG" && find . -type f ! -name MANIFEST.txt | sort | while read -r f; do
        printf '%s  %s\n' "$(sha256sum "$f" | cut -d' ' -f1)" "${f#./}"
    done )
} > "$PKG/MANIFEST.txt"

# ── 4. 打 tar ─────────────────────────────────────────────────────────────
DIST="$REPO/dist"
mkdir -p "$DIST"
OUT="$DIST/cangjie-full-${FW_LABEL}-$(date +%Y%m%d).tar.gz"
tar -czf "$OUT" -C "$STAGE" cangjie
SZ="$(du -h "$OUT" | cut -f1)"
SUM="$(sha256sum "$OUT" | cut -d' ' -f1)"

echo "═══════════════════════════════════════════════════"
echo "✅ 打包完成：$OUT"
echo "   体积 $SZ   sha256 $SUM"
echo "   目标固件：$FW_LABEL"
echo
echo "部署（设备 SSH 后）："
echo "  scp $OUT root@10.11.99.1:/home/root/"
echo "  ssh root@10.11.99.1 'cd /home/root && tar -xzf $(basename "$OUT") && cangjie/install.sh'"
echo "═══════════════════════════════════════════════════"
