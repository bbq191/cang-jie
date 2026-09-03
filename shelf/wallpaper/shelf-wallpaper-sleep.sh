#!/bin/sh
# systemd-sleep 钩子（装到 /usr/lib/systemd/system-sleep/shelf-wallpaper.sh）。
# reMarkable 定制 systemd-sleep 用 before/after，旧约定 pre/post——两套都认。
# 休眠前补 bind（xochitl 每次休眠重读 suspended.png）；唤醒后轮换下一张（避开与休眠画图抢时序）。
# 逻辑全在 wallpaper-serve 子命令里，本脚本只做转发。
BIN=/home/root/.local/bin/wallpaper-serve
[ -x "$BIN" ] || exit 0
case "$1" in
    before|pre)  "$BIN" bind >/dev/null 2>&1 ;;
    after|post)  "$BIN" roll >/dev/null 2>&1 ;;
esac
exit 0
