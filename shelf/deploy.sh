#!/bin/sh
# host 侧一键部署书架到设备（独立于整包 packaging/，Phase 验证期用）：
#   组载荷（bin/ systemd/ lo-alias/ xovi/ install.sh uninstall.sh）→ tar-over-ssh → 设备端 install.sh。
# 用法：./deploy.sh [host] [install.sh 的参数…]      host 默认 10.11.99.1
#   环境 SHELF_NO_BUILD=1 跳过交叉编译（直接用 target/ 里现成产物）
set -eu
cd "$(dirname "$0")"
HOST="${1:-10.11.99.1}"; [ $# -gt 0 ] && shift
TARGET=aarch64-unknown-linux-musl
BINS="shelf-gateway book-serve koreader-serve font-serve wallpaper-serve"
NOTES_BINS="ink-serve transcribe-serve note-serve"   # 笔记线（../notes）二进制与单元一并打进载荷

[ "${SHELF_NO_BUILD:-0}" = "1" ] || sh ./build.sh
STAGE="$(mktemp -d)"; trap 'rm -rf "$STAGE"' EXIT
mkdir -p "$STAGE/shelf/bin" "$STAGE/shelf/systemd" "$STAGE/shelf/lo-alias" "$STAGE/shelf/xovi"
for b in $BINS; do cp "target/$TARGET/release/$b" "$STAGE/shelf/bin/"; done
cp systemd/* "$STAGE/shelf/systemd/"
for b in $NOTES_BINS; do
    [ -f "../notes/target/$TARGET/release/$b" ] && cp "../notes/target/$TARGET/release/$b" "$STAGE/shelf/bin/"
done
[ -d ../notes/systemd ] && cp ../notes/systemd/*.service "$STAGE/shelf/systemd/"
cp ../chinese-ime/langhook/deploy/cangjie-lo-alias.sh "$STAGE/shelf/lo-alias/"
cp install.sh uninstall.sh "$STAGE/shelf/"
cp xovi/*.qmd "$STAGE/shelf/xovi/"
echo "-- 推送到 root@$HOST:/home/root/shelf-pkg/ 并安装"
tar -C "$STAGE" -cf - shelf | ssh "root@$HOST" 'rm -rf /home/root/shelf-pkg && mkdir -p /home/root/shelf-pkg && tar -C /home/root/shelf-pkg -xf -'
# shellcheck disable=SC2029  # 参数就是要在远端展开
ssh "root@$HOST" "sh /home/root/shelf-pkg/shelf/install.sh $*"
# host 侧 HTTPS 探测（设备 busybox wget 做不了自签）：无密码应 401；默认密码 shelf 若仍必改应 403（首登必改），否则 200
code="$(curl -sk -o /dev/null -w '%{http_code}' --max-time 5 "https://$HOST:8778/api/services" || echo 000)"
ok="$(curl -sk -o /dev/null -w '%{http_code}' --max-time 5 -u "shelf:shelf" "https://$HOST:8778/api/services" || echo 000)"
case "$ok" in
    403) echo "-- HTTPS 探测：无密码 $code（期望 401），默认密码 shelf → 403（首登必改）；浏览器开 https://$HOST:8778/ 用 shelf 登录后设新密码" ;;
    200) echo "-- HTTPS 探测：无密码 $code（期望 401），默认密码仍可用但未强制改（异常，检查 gateway.json）" ;;
    *) echo "-- HTTPS 探测：无密码 $code（期望 401），默认密码 $ok（已自定义密码则为 401 正常）；浏览器开 https://$HOST:8778/" ;;
esac
