#!/bin/sh
# cangjie-precheck.sh —— xochitl 启动前指纹核验（fail-safe 外层防线）。
#
# 借鉴 rmkit-cn 的 precheck+symlink 范式（《reMarkable中文化白皮书》4.3 节 📌 借鉴块）：
# systemd drop-in 的 LD_PRELOAD 指向本脚本管理的 active/ 下两个 symlink；本脚本由
# ExecStartPre=- 在每次 xochitl 启动前运行，核验通过才建链、任一不过就摘链——
# symlink 缺失时 glibc 对 LD_PRELOAD 条目是 warn+skip，xochitl 纯原生启动，
# 最坏结果是"中文没了但机器能正常用"，把 crash-loop 堵在注入之前。
#
# 与 .so 内部的特征码自定位/safe mode 是互补的两层：本脚本防"固件大改后连
# 扫描都可能出问题"的启动崩溃，.so 内部 safe mode 防"注入后个别目标定位失败"。
#
# 本脚本永远 exit 0：ExecStartPre 带 - 前缀双保险，任何情况都不阻塞 xochitl 启动。
set -u

DATADIR=/home/root/.local/share/cangjie-ime
ACTIVE="$DATADIR/active"
XOVI_SO=/home/root/xovi/xovi.so
HOOK_SO="$DATADIR/cangjie-langhook.so"
KNOWN="$DATADIR/xochitl.sha256.known"
XOCHITL=/usr/bin/xochitl

log() { echo "cangjie-precheck: $*" >&2; }

# 预检不过：摘除注入 symlink（缺省安全），本次裸启原生 xochitl。
disarm() {
    log "预检不过（$1）—— 摘除注入 symlink，本次裸启原生 xochitl"
    rm -f "$ACTIVE/xovi.so" "$ACTIVE/cangjie-langhook.so"
    exit 0
}

[ "$(uname -m)" = "aarch64" ]        || disarm "架构 $(uname -m) != aarch64"
[ -f "$XOVI_SO" ]                     || disarm "缺 $XOVI_SO"
[ -f "$HOOK_SO" ]                     || disarm "缺 $HOOK_SO"
[ -f "$KNOWN" ]                       || disarm "缺基线文件 $KNOWN"
command -v sha256sum >/dev/null 2>&1  || disarm "设备无 sha256sum"

CUR="$(sha256sum "$XOCHITL" 2>/dev/null | cut -d' ' -f1)"
[ -n "$CUR" ]                         || disarm "sha256 计算失败（$XOCHITL）"
# 基线文件每行一个已验证的 xochitl sha256；OTA 后哈希变化即不匹配 → 裸启，
# 重跑 install.sh 会把新固件哈希追加进基线（装机即认证当前固件）。
grep -qx "$CUR" "$KNOWN"              || disarm "xochitl sha256 $CUR 不在已验证基线（疑似固件 OTA）"

mkdir -p "$ACTIVE"
ln -sf "$XOVI_SO" "$ACTIVE/xovi.so"
ln -sf "$HOOK_SO" "$ACTIVE/cangjie-langhook.so"
log "指纹核验通过（xochitl $CUR）—— 注入链就绪"
exit 0
