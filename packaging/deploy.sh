#!/bin/sh
# host 侧一键部署书架+网关+笔记线+两个 enhance 领域服务到设备。2026-09-11 从 shelf/deploy.sh
# 搬到这里——它编排的是跨 shelf/gateway/enhance/notes 四个目录的一整套安装，本质上是"全项目安装
# 编排"的一部分，该跟 packaging/ 放一起（shelf/build.sh、shelf/install.sh、shelf/uninstall.sh 没有跟着搬）。
#   组载荷（bin/ systemd/ lo-alias/ xovi/ install.sh uninstall.sh manifest.sh devlib.sh）→ tar → ssh 送到
#   设备暂存目录 → 校验完整后换位 → 设备端 install.sh。
# 用法：./deploy.sh [host] [install.sh 的参数…]      host 默认 10.11.99.1
#   例：./deploy.sh 10.11.99.1 --only font,wallpaper     ./deploy.sh 10.11.99.1 --password '新密码'
#   环境 SHELF_NO_BUILD=1 跳过交叉编译（直接用 target/ 里现成产物）
#
# 2026-09-20 改动（脚本审计 M9/M10）：
#  · 密码不再拼进远端命令行：--password 的值经 ssh 标准输入写进设备上 0600 的临时文件，install.sh 用
#    --password-file 读后即删——含空格/引号/分号的密码不会被远端 shell 解释，host 与设备的 ps 里也看不到；
#  · 其余参数逐个 shquote 后再传，不再 `$*` 裸拼；
#  · 载荷先在本地打成 tar 文件（打包失败当场退出），推到 shelf-pkg.new，校验有 install.sh 后才换掉 shelf-pkg——
#    传输中断不会把设备上现成的 shelf-pkg 清空又留下残缺载荷；
#  · HTTPS 探测只测无认证应 401，不再拿默认密码 shelf 试登录（会在网关上制造失败登录记录）。
set -eu
cd "$(dirname "$0")"
# shellcheck disable=SC1091
. ./lib.sh
HOST="${1:-10.11.99.1}"; [ $# -gt 0 ] && shift
TARGET=aarch64-unknown-linux-musl
BINS="book-serve koreader-serve"
GATEWAY_BINS="gateway"   # 网关（../gateway）2026-09-11 正名搬顶层，二进制与单元一并打进载荷
ENHANCE_BINS="wallpaper-serve font-serve"   # 2026-09-11 从 shelf 挪进 ../enhance/，单元跟着各自目录走
NOTES_BINS="ink-serve transcribe-serve mind-serve note-serve"   # 笔记线（../notes）二进制与单元一并打进载荷

# 拆出 --password（值走 stdin），其余参数原样保留（逐个 shquote）
PASSWORD=""; HAVE_PW=0; REMOTE_ARGS=""; _prev=""
for a in "$@"; do
    if [ "$_prev" = "--password" ]; then PASSWORD="$a"; HAVE_PW=1; _prev=""; continue; fi
    case "$a" in
        --password) _prev="--password" ;;
        --password=*) PASSWORD="${a#--password=}"; HAVE_PW=1 ;;
        *) REMOTE_ARGS="$REMOTE_ARGS $(shquote "$a")" ;;
    esac
done
[ -z "$_prev" ] || { echo "!! --password 缺参数"; exit 2; }

[ "${SHELF_NO_BUILD:-0}" = "1" ] || sh ../shelf/build.sh
STAGE="$(mktemp -d)"; trap 'rm -rf "$STAGE"' EXIT
P="$STAGE/pkg/shelf"
mkdir -p "$P/bin" "$P/systemd" "$P/lo-alias" "$P/xovi"
for b in $BINS; do cp "../shelf/target/$TARGET/release/$b" "$P/bin/"; done
cp ../shelf/systemd/* "$P/systemd/"
for b in $GATEWAY_BINS; do
    [ -f "../gateway/target/$TARGET/release/$b" ] && cp "../gateway/target/$TARGET/release/$b" "$P/bin/"
done
[ -d ../gateway/systemd ] && cp ../gateway/systemd/*.service "$P/systemd/"
for b in $ENHANCE_BINS; do
    [ -f "../enhance/$b/target/$TARGET/release/$b" ] && cp "../enhance/$b/target/$TARGET/release/$b" "$P/bin/"
    [ -f "../enhance/$b/$b.service" ] && cp "../enhance/$b/$b.service" "$P/systemd/"
done
for b in $NOTES_BINS; do
    [ -f "../notes/target/$TARGET/release/$b" ] && cp "../notes/target/$TARGET/release/$b" "$P/bin/"
done
[ -d ../notes/systemd ] && cp ../notes/systemd/*.service "$P/systemd/"
cp ../enhance/lo-alias/lo-alias.sh "$P/lo-alias/"
cp ../shelf/install.sh ../shelf/uninstall.sh ../shelf/manifest.sh devlib.sh "$P/"
cp ../shelf/xovi/*.qmd "$P/xovi/"
tar -C "$STAGE/pkg" -cf "$STAGE/shelf-pkg.tar" shelf

REMOTE=/home/root/shelf-pkg
echo "-- 推送到 root@$HOST:$REMOTE/ 并安装"
rssh_in "rm -rf $REMOTE.new && mkdir -p $REMOTE.new && tar -C $REMOTE.new -xf - && [ -f $REMOTE.new/shelf/install.sh ] && rm -rf $REMOTE && mv $REMOTE.new $REMOTE" < "$STAGE/shelf-pkg.tar"
if [ "$HAVE_PW" = "1" ]; then
    printf '%s' "$PASSWORD" | rssh_in "umask 077; cat > $REMOTE/.pw"
    REMOTE_ARGS="$REMOTE_ARGS --password-file $REMOTE/.pw"
fi
# REMOTE_ARGS 每个词已 shquote，要在远端展开
rssh "sh $REMOTE/shelf/install.sh$REMOTE_ARGS"
# host 侧 HTTPS 探测（设备 busybox wget 做不了自签）：无密码应 401
if command -v curl >/dev/null 2>&1; then
    code="$(curl -sk -o /dev/null -w '%{http_code}' --max-time 5 "https://$HOST/api/services" || echo 000)"
    echo "-- HTTPS 探测：无密码 $code（期望 401）；浏览器开 https://$HOST/ 登录（首次默认密码 shelf，登录后强制改）"
else
    echo "-- 本机没有 curl，跳过 HTTPS 探测；浏览器开 https://$HOST/"
fi
