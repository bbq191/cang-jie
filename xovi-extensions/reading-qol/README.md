# reading-qol —— 阅读增强（设置页「系统增强」控制面板 + 点击翻页/快速黑白/清残影/字体）

> **完整设计 + 真机调试记录见《[系统增强白皮书](../docs/reMarkable系统增强白皮书.md)》（块4）。** 本 README 侧重端点级操作/部署纪律。

xochitl 的 QMLDiff 增强，**纯 QML 层、不碰硬件、不改 waveform、不进 `xochitl_pdf_renderer`**。
**2026-08-17：整套接进设置页控制面板，4 项开关真机验证通过**（见下「系统增强菜单」）。

**「系统增强 → 阅读增强」整套（设备真二进制 md5 3356dde7 核对 + apply-diffs + 真机端到端）**：

- `settings-reading-enhance.qmd` —— 设置 App 左侧菜单**最下方**加「系统增强」入口 + 内联「阅读增强」内容页（4 项 `SettingsCheckBoxItem` 开关 + 每 N 页步进），XHR 读写 `reading-qol.json`。**本地化 en/简/繁**：内嵌三语表 + 用 `qsTranslate("SettingsModel",{Help,Cloud,Accessibility})` 判语言（这三串简繁不同：帮助/幫助、云端/雲端、无障碍/無障礙；未翻译=en），qsTranslate 响应式 → 切语言实时跟随。（`Qt.locale()` 本机卡 en_US 无效、UI 语言不落 conf，都用不了。）字体菜单 3 项显示名同样三语。
- `reading-qol-config.qmd` —— **共享配置**（挂 DeviceSceneView#root，被 tap/mono/refresh 共用）：从 JSON 读 `cjTapPageTurn/cjFastMono/cjRefresh/cjRefreshByChapter/cjRefreshEvery`，**1.5s Timer 轮询**让设置改动返回阅读器即生效（DeviceSceneView 不重建、`onCompleted` 只触发一次，故靠轮询）。
- `tap-page-turn.qmd` —— 窄边缘点击翻页（**左 ~10% 上一页、右 ~10% 下一页**，中间 ~80% 中性；纵向再排除**上 25%/下 15%**，仅 25%~85% 高度生效——避开顶部工具栏/底部进度条/持机拇指的四角误触，保留原生滑动/笔/缩放/菜单/选择/双击进文本），受 `view.cjTapPageTurn` 门控。**2026-08-24 真机验证**：单击单次触发（无多连发）、左右边缘方向正确、中性区不误翻。参数 `0.1/0.9` + `0.25/0.85` 可真机微调。
- `fast-mono-reading.qmd` —— 阅读时锁 `Epaper.ScreenModeItem.Mono` 加速刷新，绑 `cjFastMono`，4 指点击快捷切换。
- `page-refresh.qmd` —— **翻页清残影**（独立开关 `cjRefresh`，**彩屏 & 黑白都生效**）：按章（`tocModel`+currentPage 派生切章）或按 N 页（默认 15）`ghostBuster.forceClearNow`。
- `add-reading-fonts.qmd`（**物理在 `../font-menu/`**，功能上属本组、受 `fontEnhance` 门控）—— 字体菜单按 `fontEnhance` **追加**霞鹜文楷/霞鹜新致宋/KF Readerly 三项（原字体保留，**仅 EPUB**：`epub.setFontName`，PDF 无此菜单）。
- `keyboard-mono.qmd` —— 拼音**组词态**把键盘区（`KeyboardPanel` 自带的 `#screenMode`，objectName:"keyboard"）锁 `Mono` 单色快刷、非组词态恢复 `Animation`；绑定 `candidatebar.qmd` 注入的 `cjCandidateBar.visible`，零 C 改动。

**共享状态机制**：设置页 QML 用 `XMLHttpRequest` 写 `/home/root/.local/share/cangjie-ime/reading-qol.json`（/home 持久），阅读页 QML 读取。运行时 xochitl 带 `QML_XHR_ALLOW_FILE_{READ,WRITE}=1`。**踩坑：同步 PUT 到 `file://` 只截断不写体 → 写必须异步（open 不带 `false`）；读同步 GET 正常。**

**崩溃自愈 fail-safe**（qmd 无 `.so` 的签名守卫，补的一层）：`chinese-ime/langhook/deploy/cangjie-qrr-failsafe.sh` 装到 xovi `scripts/pre-start/`，每次 `xovi/start` 用 `journalctl -b -1`（设备 journald 持久）数上一个 boot 的 xochitl 崩溃签名，**≥3 次就把这 6 个阅读增强 qmd 移出 qrr 隔离**（只动自己的、不碰 candidatebar/moxiang/IME），下一个 boot 恢复到"输入法可用"。干净 `systemctl restart` 不产生崩溃签名、不误触发。恢复：重跑 install.sh 或从 `~/.local/share/cangjie-ime/qrr-failsafe.quarantine/` 移回。

均参照竞品 [pretenderlu/rmtool](https://github.com/pretenderlu/rmtool)（GPL-3.0）揭示的**机制**净室重写，**不复制其 QMD 文本或代码**。

## 状态（Move 3.28.0.166，2026-08-14）

- **tap-page-turn**：已按 .166 真实 QML 重写 + qmldiff 离线 apply-diffs 实跑 + **真机验证通过**（用户确认点击翻页手感、`READING-QOL-TAP: loaded/next/previous` 日志命中）。
- **fast-mono-reading**：已按 .166 真实 QML 重写（3 文件）+ 离线 apply-diffs 实跑通过 + 真机部署（2026-08-15 完整 install.sh 重跑，qmldiff 四文件 `Loading file` 无解析错误）；`readingQolMonoEnabled` **默认已改 false**（打开文档不自动进单色，4 指点击手势快捷开/关）；**2026-08-15 用户真机确认点击翻页 / 4 指切单色 / 字体菜单霞鹜文楷均正常**。
  - **TODO[集成]**：把「开关」+「清残影页数」接进系统阅读设置菜单（"系统功能采集"，用户可自定义刷新页数），替代当前"4 指手势 + 硬编码 50 页"。4 指手势保留作快捷切换。
- **keyboard-mono**：`.166` KeyboardPanel.qml 与 .164 逐行一致（`root>keyboardContainer>screenMode[objectName:"keyboard",mode:Animation]`），选择器直接适用 + qmldiff 离线 apply-diffs（与 candidatebar.qmd **一起** apply，2 diff applied，emit 里 `cjCandidateBar` id 与 `#screenMode.mode` 引用共存、无注释污染）+ **2026-08-15 真机验证通过**：qmldiff `Loading`/`Processing KeyboardPanel` 无解析错误、健康检查绿；`KBD-MONO` 日志硬证 **mode 跟随组词态精确切换**（组词 candbarVisible=true→mode=1 Mono、收起 false→mode=2 Animation，两个完整来回）。可观测性提醒：候选栏本就纯黑白，Mono 化不改静态观感，"快多少"需 A/B 慢动作对比，收益可能微妙——日志证明机制在工作。

## 离线验证管线（本项目建立，务必复用）

host 侧无法"运行" xochitl，但能用官方 qmldiff 工具**离线实跑补丁**，验证解析 + 选择器命中 + emit 合法 QML——比"结构自洽"强得多：

1. `git clone --recurse-submodules https://github.com/asivery/rm-xovi-extensions`（qmldiff 子模块 = `asivery/qmldiff`，Rust）；`cargo build --release` 得 `qmldiff` CLI。
2. 从**设备** scp `.166` 的 `/usr/bin/xochitl`（md5 `5215ef7ab…`；`rmfw/` 里那份是旧 .164，别用）。
3. `tools/extract_qml.py <xochitl> <outdir>`（zstd magic `28 b5 2f fd` 全局扫 + 逐块解压）解出真实 QML（.166 = 556 个）；文件名未还原，靠内容 grep 认领目标。
4. 设备 `hashtab`（`.../qt-resource-rebuilder/hashtab`）解析出全部真实资源路径 + 标识符明文，是 `AFFECT` 路径的权威来源。
5. 把目标 QML 放到真实资源路径下：`qmldiff apply-diffs <root> <dest> patch.qmd -f -c`。这条与设备端 qt-resource-rebuilder 走**同一份 qmldiff 代码**（明文直配、不经 hashtab），设备高度一致。

**踩坑**：`INSERT { … }` 块内是 QML，注释必须用 `//`（emit 成 `/* */`）；误用 diff 注释 `;` 会被当 QML token 输出成非法 JS（离线 apply-diffs 抓出过）。无 id 的点号类型选择器要写全（`Epaper.ScreenModeItem`，裸 `ScreenModeItem` 不匹配）。

## 部署（一步一确认，tap 与 fast-mono 分两次上机）

前置：设备已装 xovi + qt-resource-rebuilder，`hashtab` 已建（candidatebar 正常工作即证明），**无需**再跑 `rebuild_hashtable`。

1. 备份：`cp` 当前 `/home/root/xovi/exthome/qt-resource-rebuilder/*.qmd` 留底。
2. scp 单个 qmd 到该目录（与 `candidatebar.qmd` 同路径），两端 md5 校验。
3. `systemctl restart xochitl` → 健康检查（`is-active`=active、`MainPID` 变化、`NRestarts` 不增）。
4. `journalctl -u xochitl | grep -E 'qmldiff|READING-QOL'`：确认 `Loading file <qmd>` + `Processing file <目标.qml>` 无 `Error while parsing`；打开文档看命中日志。
5. 真机功能验证。失败：删该 qmd + 重启回退（备份 `qrr-qmd.bak.pre-*`）。
6. 两者都通过后并入 `deploy/install.sh`（拷 qmd 到 exthome，与 `candidatebar.qmd` 同段）。

## .166 阅读器关键结构（真实 QML 核对）

- `SceneViewGestures.qml`：根 `TouchArea{id:touchArea}`；单指 `TouchAreaClickFilter{id:touchClick;objectName:"click-1"}` 的 `onClick`；`property var view` 由 DeviceSceneView 以 `view: root` 注入；原生占用 2/3/5 指手势（4 指空闲）；`ghostBuster` 裸引用可达。
- `DeviceSceneView.qml`：根 `FocusScope{id:root}`（= `view` / DocumentView 里的 `sceneView`），有 `zoomedIn/zoomedOut/textMode/notePage/page/clickGestureEnabled/document`。fast-mono 状态挂这里（两边都够得着）。
- `DocumentView.qml`：根 `FocusScope{id:root}`；`import xofm.libs.epaper as Epaper`；`root.ghostBuster`（`GhostBuster.forceClearNow`）；`currentPage` 有 `onCurrentPageChanged`；屏幕模式实例 = root 直接子 `Epaper.ScreenModeItem`（无 id），`Epaper.ScreenModeItem.Mono` 枚举存在。

## 许可证

参照机制自写、未复制 rmtool 的 GPL-3.0 QMD/代码；QMD 是描述"往哪个 QML 插什么"的独立文本补丁，运行时由 qt-resource-rebuilder 加载，不编译进 `cangjie-langhook.so`。clone `rm-xovi-extensions`/`qmldiff` 仅离线读/编译工具，不并入本项目。不代替法律意见，只记录事实与架构取舍。
