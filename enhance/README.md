# enhance —— 系统增强线

> **读者与用途**：想知道"除了书架和笔记，设备上还有哪些底层增强工具、各自是什么、怎么建怎么装"的人。
> 这些工具互相独立，大多不需要网页；有网页开关的由 [`../gateway/`](../gateway/README.md) 提供控制面。
> 整体位置见 [`../docs/OVERVIEW.md`](../docs/OVERVIEW.md)；决策记录与真机验证轮次见 [`docs/reMarkable系统增强线白皮书.md`](docs/reMarkable系统增强线白皮书.md)（读现状先看其 §00b）。

## 它是什么

这条线收拢一组**单点增强工具**：划中文时精确吸附、手写笔锋渲染、电池诊断、字体/壁纸上传即用。它们都不修改 xochitl 本体：

- 两个是 **xovi 扩展**（xovi 是第三方扩展加载框架；扩展是它在 xochitl 启动时加载的 `.so` 插件，"hook"若干函数改变行为）；
- 两个是 **Web 服务**（挂在网关下，网页上传即用）；
- 一个是独立的**诊断采样器**。

![enhance 的五个工具怎么接到设备上](docs/diagrams/enhance-overview.svg)

## 组件一览

| 组件 | 类型 | 状态 | 怎么用 |
|---|---|---|---|
| [`hl-snap/`](hl-snap/README.md) | xovi 扩展（C，ARM64） | 真机通：荧光笔 CJK 精确吸附（划哪吸哪，不再吸整行） | 网页「管理 → 系统增强」开关 `hlSnapCjk`，默认开 |
| [`handwriting-stroke/`](handwriting-stroke/README.md) | xovi 扩展（C，ARM64） | 两个 hook 目标真机通（笔尖角度模型 + 运笔速度代理，覆盖书法笔与钢笔/铅笔/马克笔等日常工具）；真实压感接不上已放弃；最常用的钢笔/铅笔量级工具（`bVar16<4`）仍摸不到 | 网页「管理 → 实验室」开关，默认关 |
| [`battop/`](battop/README.md) | 独立 Rust 二进制（采样诊断服务） | 真机通；**不随开机自启** | 网页「管理 → 系统增强」开关；开了出现「电池刺客」数据页（4 个时间窗 × 应用/进程/唤醒源） |
| [`wallpaper-serve/`](wallpaper-serve/README.md) | Web 服务（8793，挂网关） | 真机通：壁纸上传即用 + 池化轮换 | 网页「其他 → 壁纸」 |
| [`font-serve/`](font-serve/font-serve.service) | Web 服务（8792，挂网关） | 真机通：xochitl 字体上传即装 + 中文回退链 | 网页「其他 → xochitl」（字体） |
| [`shared/`](shared/PROVENANCE.md) | 公共 C 源文件 | `hl-snap` / `handwriting-stroke` 共用的特征码扫描 + trampoline 安装工具，带 host 单测（`make test`） | 不单独部署 |
| [`lo-alias/`](lo-alias/README.md) | 独立脚本 | 网关 `ExecStartPre` 用，让 `10.11.99.1` 在不插 USB 时也可达 | 不单独部署 |

> `font-serve` 没有自己的 README：它的行为在 `src/main.rs` 头注（路由、字体目录、`fonts.json` 索引）和 [`../shelf/docs/reMarkable书架白皮书.md`](../shelf/docs/reMarkable书架白皮书.md) 的字体相关章节里。

## 怎么构建与部署

三个"工具"（hl-snap / handwriting-stroke / battop）的 host 侧一键部署都在 [`../packaging/`](../packaging/README.md)：

```sh
cd packaging
sh deploy-hl-snap.sh <host>            # 构建 .so → 推送（md5 校验）→ 设备端安装
sh deploy-handwriting-stroke.sh <host>
sh deploy-battop.sh <host>
```

一般不用单独跑——`sh install-all.sh <host>` 会按顺序装好它们（xovi 扩展只落盘，最后统一重启 xochitl 一次），见 [`../docs/INSTALL.md`](../docs/INSTALL.md)。`wallpaper-serve` / `font-serve` 随 `install-all.sh` 的 `shelf` 步装。各工具自己的设备端 `install.sh` 仍可脱离编排单独跑（需要同目录的 `devlib.sh` 等文件，见各自 README）。

## 跟其它目录的关系

- [`../gateway/src/enhance/`](../gateway/src/enhance/) 是本线**开关的网页控制面**：只走 `systemctl` 和读写 `~/.local/share/cangjie-ime/reading-qol.json`（`hl-snap.so` / `hw-stroke.so` 读的是同一份文件），不关心工具源码在哪。
- `wallpaper-serve/`、`font-serve/` 是挂网关的领域服务（依赖 [`../rmsvc-core`](../rmsvc-core/README.md)），跟前三个诊断/扩展工具不是一类；2026-09-11 从 `shelf/services/` 挪进来纯属概念归类，运行时行为没变。
- [`../defw/`](../defw/README.md)（xochitl 3.28.0.172 逆向产物）**不属于**本线，是共享的逆向基座；`handwriting-stroke/` 的研究用它。
- 迁移、改名、旧路径去向等历史沿革见白皮书末尾的「迁移沿革」。
