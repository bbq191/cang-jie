# enhance —— 系统增强线

> **读者与用途**：想知道"除了书架和笔记，设备上还有哪些底层增强工具、各自是什么、怎么建怎么装"的人。
> 这些工具互相独立，大多不需要网页；有网页开关的由 [`../gateway/`](../gateway/README.md) 提供控制面。
> 整体位置见 [`../docs/OVERVIEW.md`](../docs/OVERVIEW.md)；决策记录与真机验证轮次见 [`docs/reMarkable系统增强线白皮书.md`](docs/reMarkable系统增强线白皮书.md)（读现状先看 §00b）。

## 它是什么

这条线收拢"原来散落各处的**单点增强工具**"：划中文时精确吸附、手写笔锋渲染、电池诊断、字体/壁纸上传即用。它们都不修改 xochitl 本体——两个是 **xovi 扩展**（第三方扩展加载框架加载的 `.so` 插件，在 xochitl 启动时 hook 少量函数），两个是**Web 服务**（挂在网关下），一个是独立的诊断采样器。

> 命名说明：工程纪律 历史上的"六分块④系统增强"（`xovi-extensions/` 那套 QML/UI 增强）已随 2026-09-11 大整理搬出仓库；本目录是现行的"系统增强线"，两者不要混。

## 组件一览

| 组件 | 类型 | 状态 |
|---|---|---|
| `hl-snap/` | 独立最小 xovi 扩展（C，ARM64） | 真机通：荧光笔 CJK 精确吸附（划哪吸哪） |
| `handwriting-stroke/` | 独立最小 xovi 扩展（C，ARM64） | 两个 hook 目标真机通（笔尖角度模型 + 提按速度代理，覆盖书法笔与钢笔/铅笔/马克笔等日常工具，见白皮书 §03e-§03f）；真实压感接不上已放弃改用速度代理；最常用的钢笔/铅笔量级工具（`bVar16<4`）仍摸不到 |
| `battop/` | 独立 Rust 二进制（诊断采样器） | 真机通并常驻；网页「管理 → 电池刺客」展示 4 个时间窗 × 应用/进程/唤醒源数据（历史脚本见 `battop/history/`） |
| `wallpaper-serve/` | Web 服务（8793，挂网关） | 真机通：壁纸上传即用 + 池化轮换（历史 bind-mount 方案见 `wallpaper-serve/legacy-bind-mount/`） |
| `font-serve/` | Web 服务（8792，挂网关） | 真机通：xochitl 字体上传即装 + 中文回退链 |
| `shared/` | 剥离移植的公共 C 源文件 | `hl-snap` / `handwriting-stroke` 共用的特征码扫描 + trampoline 安装工具，带 host 单测（`make test`），见 `PROVENANCE.md` |
| `lo-alias/` | 剥离移植的独立脚本 | 网关 `ExecStartPre` 用，治网络可达性（让 10.11.99.1 常驻），见其 README |

## 跟其它目录的关系（要点）

- `../gateway/src/enhance/` 是本线**开关的网页控制面**（消费方）：只走 `systemctl` 和读写 `~/.local/share/cangjie-ime/reading-qol.json`（跟 `hl-snap.so`/`hw-stroke.so` 读的是同一份文件），不关心源码放哪。
- `wallpaper-serve/`、`font-serve/` 是挂网关的领域服务（依赖 `../rmsvc-core`），与三个诊断/扩展工具不是一类；2026-09-11 从 `shelf/services/` 挪进来纯属概念归类。
- `../packaging/` 是本线三个工具（hl-snap / handwriting-stroke / battop）的 host 侧编排方，各有 `deploy-<name>.sh`；工具自己的设备端 `install.sh` 仍可脱离它单独跑。
- `../defw/`（3.28.0.172 固件逆向产物）**不属于**本线，是共享的逆向基座，`handwriting-stroke/` 研究用它。
- 逐条来龙去脉（迁移、改名、旧路径去向）见白皮书末尾附录。
