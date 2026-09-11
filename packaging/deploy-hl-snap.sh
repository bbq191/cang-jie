#!/bin/sh
# host 侧一键构建+推送+安装 enhance/hl-snap（荧光笔 CJK 精确吸附，独立最小 xovi 扩展）。
# 之前这三步（make aarch64 → scp → ssh 跑 deploy/install.sh）一直是手动做的，这里只是
# 脚本化，不改任何构建/安装逻辑本身；enhance/hl-snap/deploy/install.sh 本身仍然可以脱离
# 本脚本独立跑（先手动 make + scp 再跑它）。
#
# 前置：设备已 vellum add xovi（本脚本不装，装不装由设备端 install.sh 检查，缺失会清楚报错）。
# 前置（host 侧构建）：需要 asivery/xovi 的 clone 供 xovigen 生成扩展元数据胶水，缺省找
# ../../../xovi（即 cang-jie 仓库外层再上一级）；不在默认位置就 `XOVI_DIR=<clone路径>
# sh deploy-hl-snap.sh <host>`。
#
# 用法：./deploy-hl-snap.sh [host]      host 默认 10.11.99.1
set -eu
cd "$(dirname "$0")"
HOST="${1:-10.11.99.1}"
DIR=../enhance/hl-snap
DEST=/home/root/hl-snap

echo "== 构建 hl-snap.so =="
# ⚠️ 不跑 `make clean`——产物已提交进仓库（README："不用每次现建才能部署"），缺外部 xovi
# clone 时重编会失败；不清一遍现有 .so/xovi_glue.{c,h} 才能在那种情况下退回用仓库里已提交
# 的版本，而不是先删了才发现编不出新的（这个坑真机实测过一次，教训写进这条注释）。
if ! make -C "$DIR" aarch64; then
    if [ -f "$DIR/hl-snap.so" ]; then
        echo "⚠️  重新构建失败（大概率是本机缺 asivery/xovi clone，见上面 XOVI_DIR 提示）——"
        echo "    改用仓库里已提交的 $DIR/hl-snap.so（可能不是最新源码对应的版本）"
    else
        echo "!! 构建失败，且仓库里也没有已提交的 $DIR/hl-snap.so，无法继续"
        exit 1
    fi
fi

echo "== 推送到 root@$HOST:$DEST =="
ssh "root@$HOST" "mkdir -p $DEST/deploy"
scp "$DIR/hl-snap.so" "root@$HOST:$DEST/hl-snap.so"
scp "$DIR/deploy/install.sh" "root@$HOST:$DEST/deploy/install.sh"

echo "== 设备端安装 =="
# shellcheck disable=SC2029  # 远端路径就是要在本地展开（固定字面量，无用户输入拼接风险）
ssh "root@$HOST" "sh $DEST/deploy/install.sh"
