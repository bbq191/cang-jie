#!/bin/sh
# 把 PKM（块5）的 aarch64 二进制部署到设备。用法：./deploy.sh [host]
#   host 默认 10.11.99.1（USB）；untethered 用 wifi IP
# 落点与块3阅读同一目录 /home/root/weread（cj-stars-daemon 的 systemd 单元路径不变，
# 抽 crate 只改仓库侧、不动设备布局）。
set -e
cd "$(dirname "$0")"

HOST="${1:-10.11.99.1}"
TARGET=aarch64-unknown-linux-musl
DEST=/home/root/weread
BINS="cj-stars-daemon cj-stars"   # 守护进程 + 手动扫描 CLI；cj-nbtest 是测试件不部署

for b in $BINS; do
    [ -f "target/$TARGET/release/$b" ] || { echo "缺 $b，先跑 ./build.sh"; exit 1; }
done

# shellcheck disable=SC2029  # $DEST 是固定字面量路径，客户端展开正是所需
ssh "root@$HOST" "mkdir -p $DEST"
# shellcheck disable=SC2086,SC2046  # 有意按空格分词：多个二进制路径作独立参数传给 scp
scp $(for b in $BINS; do echo "target/$TARGET/release/$b"; done) "root@$HOST:$DEST/"
echo "✓ PKM 二进制已部署到 root@$HOST:$DEST"
echo "  cj-stars-daemon 挂 systemd 常驻（画星→自动汇总总结卡片）；systemd 单元在设备上，路径未变。"
