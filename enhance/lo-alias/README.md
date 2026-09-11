# lo-alias —— `lo-alias.sh` 来源说明

2026-09-11 **剥离移植**（copy，不是路径依赖）自 `chinese-ime/langhook/deploy/cangjie-lo-alias.sh`——
那次是全仓库归档整理，`chinese-ime/` 挪出了仓库，`shelf/deploy.sh` 原先路径引用它的这个脚本断了。
脚本内容本身其实跟中文输入法无关（治的是"不插 USB 时 xochitl :80 socket 不可达"这个网络问题，
`shelf-gateway.service` 的 `ExecStartPre` 靠它——脚本本身属于块4系统增强这类底层单点工具，只是历史上
先放在 `chinese-ime/langhook/deploy/` 下），这次落到 `enhance/` 独立成副本、改名 `lo-alias.sh`（去掉
`cangjie-` 前缀——往后新命名一律不带这个前缀），不再对接旧路径；`shelf/deploy.sh` 打包时原名从这里取
（跨目录路径引用，`shelf` 不复制维护第二份），设备端落点/`shelf-gateway.service` 的 `ExecStartPre`
也已同步改成 `lo-alias.sh`，不再是历史名 `cangjie-lo-alias.sh`。

后续这份副本独立维护，不再"改一处两边同步"。
