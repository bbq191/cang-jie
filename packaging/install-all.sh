#!/bin/sh
# ═══════════════════════════════════════════════════════════════════════════
# cang-jie 全新设备统一安装器（host 侧编排，2026-09-11 新写）。
#
# 只编排、不重新实现任何构建/传输逻辑——依次调用四个已经各自独立可用的部署脚本：
#   packaging/deploy-battop.sh              电池刺客（跟 xovi/vellum 无关）
#   packaging/deploy-hl-snap.sh             荧光笔 CJK 精确吸附（需要 vellum add xovi）
#   packaging/deploy-handwriting-stroke.sh  CJK 手写笔迹渲染优化（需要 vellum add xovi）
#   packaging/deploy.sh                     shelf 本体+网关+笔记线+两个领域服务（不需要 xovi）
# 装前先过固件安全门（sha256(/usr/bin/xochitl) 比对 firmware-allowlist.txt），避免在没验证
# 过注入定位的固件上装错。
#
# 明确不做的事（范围外，见 packaging/README.md「已知缺口」）：
#   · 不装 vellum/xovi 本体——这是所有脚本共同的手动前置条件，本脚本只在缺失时把报错原样
#     透出，不代为安装。
#   · 不装中文化（输入法/候选栏/UI 汉化）——那条链路还在 oldbak/chinese-ime/，没有回到 git
#     版本控制，需要单独手动跑。
#   · 不重建 xovi 开机持久化恢复链（xovi-reenable.service 那套）。
#   · 没有对称的 uninstall-all.sh。
#
# 用法：./install-all.sh [host] [--force] [--skip battop,hl-snap,handwriting-stroke,shelf]
#   host    默认 10.11.99.1（USB）
#   --force 固件不在白名单也强装（会自动把当前哈希追加进 firmware-allowlist.txt）
#   --skip  逗号分隔，跳过指定的安装步骤
# ═══════════════════════════════════════════════════════════════════════════
set -eu
cd "$(dirname "$0")"

HOST="${1:-10.11.99.1}"; [ $# -gt 0 ] && shift
FORCE=0
SKIP=""
for a in "$@"; do
    case "$a" in
        --force) FORCE=1 ;;
        --skip=*) SKIP="${a#--skip=}" ;;
        --skip) ;;
        *) if [ "${_prev:-}" = "--skip" ]; then SKIP="$a"; else echo "!! 未知参数：$a"; exit 2; fi ;;
    esac
    case "$a" in --skip) _prev="$a" ;; *) _prev="" ;; esac
done

skip_has() { case ",$SKIP," in *",$1,"*) return 0 ;; *) return 1 ;; esac; }

echo "═══ 固件安全门（root@$HOST）═══"
REMOTE_HASH="$(ssh "root@$HOST" 'sha256sum /usr/bin/xochitl' | awk '{print $1}')"
if [ -z "$REMOTE_HASH" ]; then
    echo "!! 没拿到 /usr/bin/xochitl 的 sha256（ssh 连不上，或设备上没有这个文件？）"
    exit 1
fi
if grep -q "^${REMOTE_HASH}[[:space:]]" firmware-allowlist.txt; then
    LABEL="$(awk -v h="$REMOTE_HASH" '$1==h{$1=""; print; exit}' firmware-allowlist.txt)"
    echo "-- 固件命中白名单：$LABEL"
elif [ "$FORCE" = "1" ]; then
    echo "⚠️  固件不在白名单（sha256=$REMOTE_HASH），--force 强装——追加进 firmware-allowlist.txt"
    echo "$REMOTE_HASH  (--force 追加，未验证，$(date +%Y-%m-%d))" >> firmware-allowlist.txt
else
    echo "!! 固件不在白名单（sha256=$REMOTE_HASH）。"
    echo "   这台设备的 xochitl 没有在这套安装脚本上验证过注入定位，qmd/hook 偏移可能对不上。"
    echo "   确认这台设备的固件确实跟已验证过的版本一致，要强装就加 --force（会自动记录这个哈希）。"
    exit 1
fi

INSTALLED=""
FAILED=""

run_step() {
    name="$1"; script="$2"
    if skip_has "$name"; then
        echo; echo "-- 跳过 $name（--skip）"
        return 0
    fi
    echo; echo "═══ $name ═══"
    if sh "$script" "$HOST"; then
        INSTALLED="$INSTALLED $name"
    else
        echo "!! $name 失败（见上面这一步的原始报错）"
        FAILED="$FAILED $name"
    fi
}

run_step battop ./deploy-battop.sh
run_step hl-snap ./deploy-hl-snap.sh
run_step handwriting-stroke ./deploy-handwriting-stroke.sh
run_step shelf ./deploy.sh

echo
echo "═══════════════════════════════════════════════════════════"
echo "已安装：${INSTALLED:-（无）}"
if [ -n "$FAILED" ]; then
    echo "❌ 失败：$FAILED —— 看对应步骤上面的原始报错，不会自动重试"
fi
echo "─── 不在本脚本范围内，需要手动处理 ───"
echo "· vellum/xovi 引导（若 hl-snap/handwriting-stroke 因缺 xovi.so 失败）：设备上先跑"
echo "    vellum add xovi qt-resource-rebuilder"
echo "· 中文化（输入法/候选栏/UI 汉化）：这条链路目前只在 /home/afu/Projects/oldbak/chinese-ime/，"
echo "    没有回到 git 版本控制，需要去那边手动编译 + 跑 deploy/install.sh"
echo "· xovi 开机持久化恢复链、chrony 国内 NTP、wifi-watch：这次没有一并恢复，见 README「已知缺口」"
echo "═══════════════════════════════════════════════════════════"
[ -z "$FAILED" ]
