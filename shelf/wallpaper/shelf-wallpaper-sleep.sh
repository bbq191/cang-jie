#!/bin/sh
# systemd-sleep 钩子（装到 /usr/lib/systemd/system-sleep/shelf-wallpaper.sh）。
# reMarkable 定制 systemd-sleep 用 before/after，旧约定 pre/post——两套都认。
# 休眠前补 bind（xochitl 每次休眠重读 suspended.png）；唤醒后轮换下一张（避开与休眠画图抢时序）。
# 逻辑全在 wallpaper-serve 子命令里，本脚本只做转发。
BIN=/home/root/.local/bin/wallpaper-serve
[ -x "$BIN" ] || exit 0
# 打点进 journal（`journalctl -t shelf-wallpaper`）：systemd-sleep 不记录同步钩子的执行，没这行看不出跑没跑。
case "$1" in
    before|pre)  logger -t shelf-wallpaper "hook $1 $2: bind"; "$BIN" bind 2>&1 | logger -t shelf-wallpaper ;;
    after|post)  logger -t shelf-wallpaper "hook $1 $2: roll"; "$BIN" roll 2>&1 | logger -t shelf-wallpaper ;;
esac
exit 0
