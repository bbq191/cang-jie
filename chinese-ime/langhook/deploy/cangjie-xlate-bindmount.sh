#!/bin/sh
# cangjie #2 UI 汉化（verity 安全）：bind-mount /home 的完整翻译目录（原版 en/de/es/fr +
# 我们的 zh_CN/zh_TW/zh_HK）覆盖 /usr 只读翻译目录，让 xochitl 原生 load 中文 .qm——
# 选简体中文→语言选择器正确 + 全界面中文 + 原生重译。运行时 VFS 挂载、不改 /usr 块、
# 无 dm-verity 回滚。xovi/start（含重启后手动 vellum reenable / xovi start）每次都跑
# 本 pre-start 脚本，在 xochitl 起来之前重新应用。幂等。
SRC=/usr/share/remarkable/xochitl/translations
OVL=/home/root/.local/share/cangjie-ime/xlate-overlay
ZH=/home/root/.local/share/cangjie-ime/translations
mkdir -p "$OVL"
# 组装 overlay（幂等）：先拿原版所有 .qm（$SRC 未挂载时是 /usr 原版；已挂载时是 OVL 自身，
# 拷回无害），再叠我们的 zh。
for f in "$SRC"/*.qm; do [ -e "$f" ] && cp -a "$f" "$OVL"/ 2>/dev/null; done
cp -a "$ZH"/reMarkable_zh_*.qm "$OVL"/ 2>/dev/null
# 未挂载才挂（避免重复叠加）
if ! grep -q " $SRC " /proc/mounts 2>/dev/null; then
    mount --bind "$OVL" "$SRC" && echo "[cangjie-xlate] bind-mount 已应用" || echo "[cangjie-xlate] bind-mount 失败"
else
    echo "[cangjie-xlate] 已挂载，跳过"
fi
