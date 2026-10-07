#!/bin/sh
# host 侧一键构建+推送+安装 enhance/ui-font（xochitl 界面字体，独立最小 xovi 扩展）。
# deploy-xovi-ext.sh 的薄包装（构建/推送/md5/安装流程与 hl-snap 共用一份）。
# 用法：./deploy-ui-font.sh [host]      host 默认 10.11.99.1；环境 DEFER_XOVI_START=1 / CJ_SKIP_BUILD=1 同 deploy-xovi-ext.sh 头注。
set -eu
cd "$(dirname "$0")"
[ $# -le 1 ] || { echo "!! 用法：deploy-ui-font.sh [host]"; exit 2; }
exec sh ./deploy-xovi-ext.sh ui-font "$@"
