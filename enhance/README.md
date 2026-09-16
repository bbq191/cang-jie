# enhance —— 系统增强线（第七条顶层项目线，2026-09-09 新开）

跟 `shelf/`、`notes/` 一样是独立的顶层目录/项目线，不嵌进已有的六分块目录里。起因见 `docs/reMarkable系统增强线白皮书.md` §00（用户三轮纠正后定案："CJK 精确吸附不动老项目，在新路径下工作，类似 shelf/" → "也不放在 cang-jie 路径下随便一个已有目录，应该是新路径，比如 enhance，里面不仅有精准吸附，还有笔锋和电池刺客等等" → "属于一条新线路，系统增强线"）。

⚠️ **命名跟 工程纪律 六分块的「④系统增强」（`xovi-extensions/`）撞了**——那边指设备端 QML/UI 增强全家桶（reading-qol/font-menu）；这边专指"底层、跨块、原来散落各处的单点增强工具"，两者暂时并存，还没有正式在 工程纪律 里理顺关系，各自独立记录，别混。

## 文档

| 文档 | 记什么 |
|---|---|
| `docs/reMarkable系统增强线白皮书.md` | 决策记录 + 真机验证轮次，读现状先看 §00b |
| `README.md`（本文件） | 目录一览、跟其它目录的关系 |
| 各组件自己的 `README.md` | 组件自己的说明（怎么建怎么部署，`handwriting-stroke/` 另外还有完整研究过程记录） |

## 组件一览

| 组件 | 类型 | 状态 |
|---|---|---|
| `hl-snap/` | 独立最小 xovi 扩展（C，ARM64） | ✅ 真机通（荧光笔 CJK 精确吸附，划哪吸哪） |
| `battop/` | 独立 Rust 二进制（诊断采样器） | ✅ 真机通（历史更早，`git mv` 过来的，非本线首创）；2026-09-10 真机实装+常驻运行，`shelf` 网页「管理→电池刺客」二级 tab 接入 4 个时间窗×应用/进程/唤醒源的真实数据展示 |
| `handwriting-stroke/` | 独立最小 xovi 扩展（C，ARM64） | ✅ 两个 hook 目标真机通（笔尖角度模型+提按速度代理，覆盖书法笔+钢笔/铅笔/马克笔等日常工具，见白皮书 §03e-§03f）；真实硬件压感接不上已放弃改用速度代理；最常用的钢笔/铅笔量级工具（`bVar16<4`）仍摸不到 |
| `wallpaper-serve/` | 独立 Rust 二进制（网页服务，挂 `gateway/`） | ✅ 真机通；2026-09-11 从 `shelf/services/wallpaper-serve` 挪进来（概念上更贴近系统增强），壁纸上传即用+池化轮换，`gateway/` 网页「其他→壁纸」二级子标签 |
| `font-serve/` | 独立 Rust 二进制（网页服务，挂 `gateway/`） | ✅ 真机通；2026-09-11 从 `shelf/services/font-serve` 挪进来（概念上更贴近系统增强），xochitl 字体上传即装+中文回退链，`gateway/` 网页「其他→xochitl 字体」二级子标签 |
| `shared/` | 剥离移植的公共源文件（C） | `hl-snap`/`handwriting-stroke` 共用的特征码扫描+trampoline 安装工具文件（2026-09-15 全量代码审查后从各自逐字节重复的独立拷贝收进这里，新增 `trampoline_patch.c`），见 `PROVENANCE.md`；2026-09-16 补 `tests/test_shared.c`（`make test`，host gcc，不需要真机/交叉工具链，随 CI 跑）——此前这两个"涉及 xovi/mprotect 高危代码路径"共用的工具文件完全零自动化覆盖 |
| `lo-alias/` | 剥离移植的独立脚本 | `gateway.service` 的 `ExecStartPre` 用，治的是网络可达性问题，见 `README.md` |

## 跟其它目录的关系

- `chinese-ime/langhook/` 2026-09-11 挪出了仓库（本身仍是现役，`cangjie-langhook.so` 还在设备上跑，只是不再是仓库里的活跃开发目标）——`hl-snap/`/`handwriting-stroke/` 原本路径引用它的三个工具文件（`scan.c`/`pattern.c`/`trampoline_aarch64.c`），现已剥离移植成本目录下的独立副本 `shared/`，不再对接旧路径，见 `shared/PROVENANCE.md`。
- `misc/battery-audit/` 整个目录已经不存在——`battop/` 本体 2026-09-09 `git mv` 到了这里；剩下的诊断脚本历史记录（`bataudit*.sh`/`APP-DESIGN.md`）2026-09-11 也 `git mv` 进了 `battop/history/`，理由跟原因见 `battop/README.md`。
- `defw/`（3.28.0.172 固件的逆向工程产物，原名 `ghidra-project-328`，2026-09-10 改名）**不属于** `enhance/`，是共享的逆向基座产物存放处，跟 `ghidra-project/`（.169 固件）并列——`handwriting-stroke/` 的研究用它，但目录本身独立。
- `wallpaper-serve/`、`font-serve/` 跟 `hl-snap`/`battop`/`handwriting-stroke` 不是一回事——它们是**挂 `gateway/` 网页托管的领域服务**（依赖顶层 `../rmsvc-core` 共享基座、有自己的上传/配置 HTTP API），不是零依赖独立诊断工具或 xovi 扩展。2026-09-11 从 `shelf/services/` 挪进来纯粹是概念分类调整（"壁纸"/"字体"更像系统增强而不是"书架内容管理"业务），运行时行为、依赖 `gateway/` 代理托管的方式都没变，各自 `README.md` 有历史沿革说明。
- `packaging/`（2026-09-11 新增的全新设备统一安装器）是 `hl-snap`/`handwriting-stroke`/`battop` 三个工具的**host 侧编排方**——各自新增了一个 `packaging/deploy-<name>.sh`（构建+推送+跑设备端 `install.sh` 的自动化，不改任何构建/安装逻辑本身），`packaging/install-all.sh` 把三个连同 `shelf` 一起串起来，见 `../packaging/README.md`。三个工具原有的设备端 `install.sh`/`deploy/install.sh` 仍然可以脱离这层独立跑。
- `gateway/src/enhance/`（网页「管理」页系统增强/实验室/电池刺客几个二级 tab 的后端；2026-09-11 前叫 `shelf/services/shelf-gateway/src/enhance/`，网关正名搬顶层后路径跟着变）是这条线的**消费方**——那边的 `battop.rs` 只是拿设备上已经装好的 `/home/root/battop/battop` 走 `systemctl`+读 `summary.json`，不关心源码在仓库哪个位置。同一个"enhance"名字不是巧合——网页那边 2026-09-09 早先就用这个名字组织了那几个开关的面板代码，这次顶层目录顺着同一个名字延续，是有意保持一致。`hlSnapCjk`/`hwStrokeNibMinRatio` 等开关目前网页那边写的还是 `~/.local/share/cangjie-ime/reading-qol.json`（跟 `hl-snap.so`/`hw-stroke.so` 读的是同一份文件），两边没有直接代码依赖，只是约定用同一个配置文件当接口。**这个消费方 2026-09-10 又演进了两轮**（本仓库代码零变化，只是网页那边的呈现变了）：电池刺客从「系统增强」卡片拆成「实验室」开关+独立「电池刺客」二级 tab（含耗电情况/唤醒源两个三级 tab，真机接了 `summary.json` 的时间窗数据），以及网页正文全量支持中英文切换——细节都在 `shelf/docs/reMarkable书架白皮书.md` §03ak-§03an，本仓库文档不重复记。
