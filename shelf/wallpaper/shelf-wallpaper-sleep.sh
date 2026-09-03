#!/bin/sh
# systemd-sleep 钩子（装到 /usr/lib/systemd/system-sleep/shelf-wallpaper.sh）。
# reMarkable 定制 systemd-sleep 用 before/after，旧约定 pre/post——两套都认。
# 只在休眠前补 bind（xochitl 每次休眠重读 suspended.png）。**轮换不在这里**：充电/USB 连着时按电源键
# 内核不 suspend、本钩子不跑（2026-09-03 真机），轮换由 wallpaper-serve 监听 xochitl 唤醒日志触发（wake.rs）。
BIN=/home/root/.local/bin/wallpaper-serve
[ -x "$BIN" ] || exit 0
# 打点进 journal（`journalctl -t shelf-wallpaper`）：systemd-sleep 不记录同步钩子的执行，没这行看不出跑没跑。
case "$1" in
    before|pre)  logger -t shelf-wallpaper "hook $1 $2: bind"; "$BIN" bind 2>&1 | logger -t shelf-wallpaper ;;
    after|post)  logger -t shelf-wallpaper "hook $1 $2: (轮换由 wallpaper-serve 唤醒监听负责)" ;;
esac
exit 0
