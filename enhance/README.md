# enhance —— 系统增强线（第七条顶层项目线，2026-09-09 新开）

跟 `shelf/`、`notes/` 一样是独立的顶层目录/项目线，不嵌进已有的六分块目录里。起因见 `docs/reMarkable系统增强线白皮书.md` §00（用户三轮纠正后定案："CJK 精确吸附不动老项目，在新路径下工作，类似 shelf/" → "也不放在 cang-jie 路径下随便一个已有目录，应该是新路径，比如 enhance，里面不仅有精准吸附，还有笔锋和电池刺客等等" → "属于一条新线路，系统增强线"）。

⚠️ **命名跟 工程纪律 六分块的「④系统增强」（`xovi-extensions/`）撞了**——那边指设备端 QML/UI 增强全家桶（reading-qol/font-menu）；这边专指"底层、跨块、原来散落各处的单点增强工具"，两者暂时并存，还没有正式在 工程纪律 里理顺关系，各自独立记录，别混。

## 文档

| 文档 | 记什么 |
|---|---|
| `docs/reMarkable系统增强线白皮书.md` | 决策记录 + 真机验证轮次，读现状先看 §00b |
| `README.md`（本文件） | 目录一览、跟其它目录的关系 |
| `hl-snap/`、`battop/`、`handwriting-stroke/` 各自的 `README.md` | 组件自己的说明（怎么建怎么部署，`handwriting-stroke/` 另外还有完整研究过程记录） |

## 组件一览

| 组件 | 类型 | 状态 |
|---|---|---|
| `hl-snap/` | 独立最小 xovi 扩展（C，ARM64） | ✅ 真机通（荧光笔 CJK 精确吸附，划哪吸哪） |
| `battop/` | 独立 Rust 二进制（诊断采样器） | ✅ 真机通（历史更早，`git mv` 过来的，非本线首创） |
| `handwriting-stroke/` | 独立最小 xovi 扩展（C，ARM64） | ✅ 两个 hook 目标真机通（笔尖角度模型+提按速度代理，覆盖书法笔+钢笔/铅笔/马克笔等日常工具，见白皮书 §03e-§03f）；真实硬件压感接不上已放弃改用速度代理；最常用的钢笔/铅笔量级工具（`bVar16<4`）仍摸不到 |

## 跟其它目录的关系

- `chinese-ime/langhook/` 完全没动——`hl-snap/` 只是路径引用它的三个工具文件（`scan.c`/`pattern.c`/`trampoline_aarch64.c`，特征码扫描 + ARM64 trampoline），不修改、不移动。
- `misc/battery-audit/battop/` 已经不存在，`git mv` 到了这里；`misc/battery-audit/` 目录本身还在（剩下的诊断脚本历史记录：`bataudit*.sh`/`APP-DESIGN.md`）。
- `ghidra-project-328/`（3.28.0.172 固件的逆向工程产物）**不属于** `enhance/`，是共享的逆向基座产物存放处，跟 `ghidra-project/`（.169 固件）并列——`handwriting-stroke/` 的研究用它，但目录本身独立。
- `shelf/services/shelf-gateway/src/enhance/`（网页「管理→系统增强」面板的后端）是这条线的**消费方**——那边的 `battop.rs` 只是拿设备上已经装好的 `/home/root/battop/battop` 走 `systemctl`，不关心源码在仓库哪个位置。同一个"enhance"名字不是巧合——网页那边 2026-09-09 早先就用这个名字组织了那几个开关的面板代码，这次顶层目录顺着同一个名字延续，是有意保持一致。`hlSnapCjk` 开关目前网页那边写的还是 `~/.local/share/cangjie-ime/reading-qol.json`（跟 `hl-snap.so` 读的是同一份文件），两边没有直接代码依赖，只是约定用同一个配置文件当接口。
