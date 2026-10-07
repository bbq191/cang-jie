#!/bin/sh
# ui-font 安装脚本（vellum-xovi 结构，只碰 /home，不碰 /usr）——xochitl 界面字体，独立最小 xovi 扩展。
# 流程在 packaging/xovi-ext-install.sh（与 hl-snap 共用，数据驱动），本文件只有这个扩展的数据。运行需要同目录有
# xovi-ext-install.sh 与 devlib.sh——由 `packaging/deploy-ui-font.sh` 一起推送。
#
# 前置：vellum add xovi。界面字体本身（上传、选择）由 shelf 的 font-serve 管，这个扩展只读它写的
# ~/.local/share/shelf/ui-font.json；没选界面字体时扩展原样放行，装了也不改变任何东西。
# 用法：./install.sh [--no-restart]（含义同 hl-snap 的 install.sh）
# shellcheck disable=SC2034  # 下面这些变量由 source 进来的 xovi-ext-install.sh 使用
set -eu
HERE="$(cd "$(dirname "$0")" && pwd)"
EXT_NAME=ui-font
EXT_SO=ui-font.so
EXT_MAPTAG=ui-font
RQOL_INIT=''   # 不用 reading-qol.json
RQOL_MSG=''
OK_MSG='网页「其他 → xochitl → 界面字体」选字体，整机重启后生效。'
NEXT_MSG=''
[ -f "$HERE/xovi-ext-install.sh" ] || { echo "!! 缺 $HERE/xovi-ext-install.sh（用 packaging/deploy-ui-font.sh 部署，它会一起推送）"; exit 1; }
# shellcheck disable=SC1091
. "$HERE/xovi-ext-install.sh"
