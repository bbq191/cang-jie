# wallpaper-serve —— 休眠壁纸上传即用

**一句话**：网页上传一张图，设备休眠时就满屏显示它；传多张会组成一个"池"，每次休眠后自动换下一张（下次休眠显示）。

- **怎么用**：网关网页「其他 → 壁纸」上传。第一次启用后要**整机重启一次**，之后上传、切换都不用重启。
- **是什么**：Web 服务，只听本机 `127.0.0.1:8793`，经网关 `/api/wallpapers` 访问；随 `install-all.sh` 的 shelf 步安装。
- **现状**：真机通（3.28.0.172，2026-09-06 定稿）。2026-10-09 第六轮审计的改动（见下文"最近的改动"）已部署，功能未手测。
- 代码：`src/main.rs` 路由与子命令，`store.rs` 壁纸池与缩放，`native.rs` 写 xochitl 配置键，`wake.rs` 轮换触发；服务单元 `wallpaper-serve.service`。host 测试 `cargo test`（12 项，2026-10-09 实跑）。

## 接口（经网关前缀 `/api/wallpapers`）

| 路由 | 做什么 |
|---|---|
| `GET /` | 池里的图、轮换模式 `mode`、当前图 `current` |
| `GET /status` | 服务状态，含 `native.restartPending`（写过配置键、还没整机重启） |
| `POST /` | 上传（multipart，可多张）；池里还没有当前图时自动激活第一张，`?activate=1` 强制激活 |
| `PUT /current {name}` | 换成池里的某一张 |
| `PUT /mode {mode}` | 轮换模式：`sequential`（按顺序，缺省）/ `random`（随机）/ `fixed`（固定不换） |
| `DELETE /{name}` · `GET /{name}` | 删 / 取池里的某一张 |
| `GET /events` | 事件流（池变化时发 `wallpapers/pool`） |

命令行子命令：`wallpaper-serve enable | disable | roll | activate <名字>`。

## 机制

xochitl 有一个隐藏配置键 `xochitl.conf` → `[General] SleepScreenPath=<png>`。它指向 `~/.local/share/shelf/wallpapers/current.png` 之后，休眠屏会原生满屏显示这张图、自动隐藏中间的插画卡，而且**每次休眠都重读文件**。所以：

| 动作 | 怎么做 |
|---|---|
| 写配置键 | **只写一次**：激活第一张壁纸时 `native.rs` 自动写，或手动 `wallpaper-serve enable` |
| 换图 | 永远是**原地覆盖 `current.png`**（保持同一个文件）；池里只有一张且 `current.png` 已是它时不重写 |
| 轮换 | 见下一段 |
| 入池 | 等比放大到盖满 954×1696 再居中裁掉多余部分（cover）。10-09 起**先在原图上裁出会落在屏幕里的那块，再一次缩放**（此前整张缩放后再裁，横图白做一半以上的重采样；开发机 12MP 横图 1.72s → 1.01s，中间图 15MB → 6.5MB）。源图先只读文件头，超过 1600 万像素或长宽比极端的直接拒收，避免解码吃光内存 |
| 卸载 | `wallpaper-serve disable` 删掉配置键，恢复原生休眠屏 |

**轮换**：inotify 监听壁纸目录的 `IN_CLOSE_NOWRITE`（有人只读地打开又关上了 `current.png`）。

- xochitl 每次进休眠都读一遍 `current.png`（2026-09-24 真机观察：读完约 120 ms 后它才写 `Normal to DeepSleep`），读完就按模式换下一张。
- 10 秒内重复读只算一次，按含休眠的开机时长 `/proc/uptime` 计（09-25 起；此前按单调时钟，休眠几小时后醒来、10 秒内又休眠会被误当成重复读）。
- 本服务自己写 `current.png` 是 `IN_CLOSE_WRITE`、池图在子目录，都不会触发。
- 空闲时零唤醒，每次休眠醒一次。09-24 真机验证：休眠那一刻即轮换、只轮换一次（充电状态下还没试）。
- 不用 systemd-sleep 钩子：充电时按电源键内核不 suspend，钩子不可靠（2026-09-03 真机）。09-24 前是跟 `journalctl -f -u xochitl` 找唤醒日志，代价是 xochitl 每写一行日志都醒一次。

**出错与自愈**：监听建不起来（如目录不在）时按 5 秒到 5 分钟指数退避重试；一次监听正常跑满 5 分钟后，退避从 5 秒重新算（09-25 起）。壁纸目录被删或被卸载时，内核撤掉监听、只发一条 `IN_IGNORED`：09-30 起收到它就当出错，走同一套退避重建目录和监听（已部署，这条路径没在真机上专门触发过）。

不写 `/usr`、不做 bind-mount、没有开机单元和 sleep 钩子，也不起 `journalctl` 子进程。

**首次写键后要整机重启一次** 才会读进这个键；网页壁纸页和 `GET /status` 的 `native.restartPending` 会提示。统一用整机重启（`reboot`）：单独 `systemctl restart xochitl` 有概率在它退出时崩溃，xovi 已生效时更**别**跑 `xovi/start`（见 [`../../docs/INSTALL.md`](../../docs/INSTALL.md)「常见问题」）。

**改 `xochitl.conf` 的纪律**：`rmsvc_core::xochitl_conf` 只动 `[General]` 下这一个键，先写临时文件再 rename、保留原文件权限，首次改前留 `xochitl.conf.shelf-bak`。这个文件里有 DeveloperPassword 和 UserToken，**任何地方都不打印它的行内容**。

## 最近的改动

| 日期 | 改动 | 状态 |
|---|---|---|
| 2026-10-09 | cover 先裁再缩放（见上表）；壁纸池只认普通文件（目录、半成品不再被轮换选中而报错）；找 xochitl 主进程改读 `/proc`，不再每次 fork `systemctl show`；测试改用 `Paths::sandbox` | 10-09 已部署（部署自检通过），功能未手测 |
| 2026-09-30 | 收到 `IN_IGNORED` 当出错重建监听 | 已部署，未专门触发 |
| 2026-09-25 | 去重改按 `/proc/uptime`；退避复位；上传暂存移到 `~/.local/state/shelf/upload/`（/home 分区，启动时清 `.part` 半成品；此前可能落到 tmpfs 占内存） | 已部署，未专门核 |

## 路径（XDG）

| 内容 | 位置 |
|---|---|
| 壁纸池 | `~/.local/share/shelf/wallpapers/pool/*.png` |
| 当前壁纸 | `~/.local/share/shelf/wallpapers/current.png` |
| 状态（模式、当前图） | `~/.local/state/shelf/wallpaper-state.json` |
| 上传暂存 | `~/.local/state/shelf/upload/`（与 font-serve 共用，启动时清 `.part`） |

## 历史（已退役）

2026-09-03～09-05 曾用 bind-mount 覆盖 `/usr/share/remarkable/suspended.png`，再用三张 776×776 透明图盖住插画卡（`shelf-wallpaper-bind.service` + `system-sleep/shelf-wallpaper.sh` + `blank776.png`）。发现 `SleepScreenPath` 后整套删掉，脚本存档在 [`legacy-bind-mount/`](legacy-bind-mount/)，**不要再用**；万一从更老的备份恢复出这套残留，按书架白皮书 §03x 手工清理。来龙去脉见书架白皮书 §03w / §03x / §03ab。
