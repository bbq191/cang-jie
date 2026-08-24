# reMarkable Paper Pro Move 系统增强白皮书（块4）

> 系统增强线（阅读/显示/笔记 UX）设计与真机调试记录 · 2026-08
>
> 本白皮书是块4「系统增强」的设计单一事实来源。跨块优先级见《[功能路线图白皮书](../../docs/reMarkable功能路线图白皮书.md)》§09；
> 代码在 `xovi-extensions/reading-qol/`（+ `font-menu/`）；设置页面板 qmd 是 `settings-reading-enhance.qmd`。
>
> ⚠️ **两处跨块**：① 「笔记增强 → 荧光笔汉字吸附」逻辑是 `chinese-ime/langhook` 里的 C hook（块2 的 .so），完整设计见《[阅读白皮书](../../reading/docs/reMarkable阅读白皮书.md)》§03 组件3d；② 「笔记增强 → ★全局待办」开关只是门户，能力本体见《[PKM 白皮书](../../pkm/docs/reMarkablePKM白皮书.md)》；③ 「导入书籍自动优化」开关消费方是 wr-serve（Rust），见《[阅读白皮书](../../reading/docs/reMarkable阅读白皮书.md)》§07-D。本白皮书只讲这些开关的**门户/面板机制**，不重复能力本体。

## 00｜定位

给 xochitl 原生阅读器/系统加一层 UX 增强，**纯 QMLDiff 注入、不碰硬件、不改 waveform、不进 `xochitl_pdf_renderer`**，全部默认关、用户在设置页逐项开。缘起：竞品 [pretenderlu/rmtool](https://github.com/pretenderlu/rmtool)（GPL-3.0）的 `tap-page-turn` 与 `fast-mono-reading`——**参照其揭示的机制净室重写，不复制其 QMD 文本或代码**。许可证隔离沿用项目纪律：QMD 是"往哪个 QML 插什么"的独立文本补丁，运行时由 qt-resource-rebuilder 加载，不编译进 `cangjie-langhook.so`。

## 01｜设置页「系统增强」门户（2026-08-17 接入，4+1 项开关真机验证）

**入口不走阅读器 FormatMenu，而是设置 App**：`settings-reading-enhance.qmd` 往 `Settings.qml` 左侧菜单**最下方**插「系统增强」`ArkControls.SidebarItem`（设置页用 `onTriggered`）+ 内联「阅读增强」内容页（`SettingsCheckBoxItem` 开关，`selected`+`clicked` 手动翻转）。内容切换 = `_selectedPage`(int) 哨兵 990001 → `payloadLoader.sourceComponent` 用 `REBUILD`+`LOCATE AFTER STREAM /{/` 注入早返回。菜单是 `SettingsModel` 驱动的 `Repeater`（项不在 QML 里），故往 `ColumnLayout#settingsColumn` 插静态项。

**开关清单**（写 `reading-qol.json`）：① 点击翻页 `cjTapPageTurn` ② 快速黑白 `cjFastMono` ③ 清残影 `cjRefresh`（+按章 `cjRefreshByChapter`/每 N 页 `cjRefreshEvery`）④ 阅读字体增强 `fontEnhance` ⑤ 导入书籍自动优化 `autoOptimize`（默认关，消费方 wr-serve）。另有「笔记增强」二级分类（荧光笔吸附、★全局待办）。

**跨 QML 树共享状态 = `reading-qol.json`**（`/home/root/.local/share/cangjie-ime/reading-qol.json`，/home 持久）：设置页 QML 用 `XMLHttpRequest` 写、阅读页 QML 读。运行时 xochitl 带 `QML_XHR_ALLOW_FILE_{READ,WRITE}=1`。
- **大坑：同步 PUT 到 `file://` 只截断不写体 → 写必须异步**（open 不带 `false`）；读同步 GET 正常。
- **传播靠 `reading-qol-config.qmd` 在 `DeviceSceneView#root` 的 1.5s 轮询 Timer**（`onCompleted` 只触发一次、返回阅读器不重建，"改了不生效"就是缺这个轮询）；字体菜单例外（构建那刻读一次、退出重开生效）。
- **★全量防覆盖铁律**：每个写 `reading-qol.json` 的二级页都必须**读写全量键**（翻页页/书籍页原本只写 7 键，加 starTodo 三键，否则切翻页设置会抹掉 `starTodoEnabled` → daemon 读成 false 功能被意外关）。

**本地化 en/简/繁（三语真机验证）**：面板/菜单/字体名随 UI 语言实时跟随。检测踩坑——本机 `Qt.locale().name` 卡 `en_US`、UI 语言不落 `xochitl.conf`、`languageSettings` 是下传 property 拿不到；正解用**响应式 `qsTranslate("SettingsModel",{Help,Cloud,Accessibility})`** 判简繁（帮助/幫助、云端/雲端、无障碍/無障礙，未翻译=en），判别串靠 `lconvert` 反编译 `reMarkable_zh_{CN,TW}.qm` diff 得到。**QML 硬坑**：`property var T` 大写开头→设置页加载失败 `Property names cannot begin with an upper case letter`（改小写 `i18n`）。

## 02｜阅读增强（点击翻页 / 快速黑白 / 清残影 / 字体）

**1. 点击翻页（`tap-page-turn.qmd`）**：阅读视图窄边缘点击——**左 ~10% 上一页、右 ~10% 下一页、中间 ~80% 中性**；纵向再排除**上 25%/下 15%**（仅 25%~85% 高度生效），保留原生滑动/笔/缩放/菜单/选择/双击进文本。机制：REBUILD `SceneViewGestures.qml` 的 `touchClick.onClick`（`TouchAreaClickFilter#click-1`），按归一化坐标 `pos.x/width`、`pos.y/height` 判方向 → `view.moveForward()`/`view.moveBackward()`，受 `view.cjTapPageTurn` 门控，一串守卫（`zoomedIn`/`notePage`/`textMode`/`linkPressed`/文件类型）避免误触。Move 屏小、滑动翻页别扭，点击翻页是刚需级改善，与 P0 微信读书注入的 EPUB 阅读体验直接叠加。**演进·分区**：初版"纵向中段 + 底部通栏"→ 左下角误翻（底部通栏抢判定）→ 改左右三分（33%）全高 → **2026-08-24 收窄为左右 10% 边缘 + 纵向上 25%/下 15% 排除**（三分太宽、边角持机误触），真机验证单击单次触发、方向正确、中性区不误翻。横向 `0.1/0.9` + 纵向 `0.25/0.85` 均可真机微调。

**2. 快速黑白（`fast-mono-reading.qmd`）**：阅读时锁 `Epaper.ScreenModeItem.Mono` 加速刷新，绑 `cjFastMono`，4 指点击快捷切换。3 文件 AFFECT——状态注入 `DeviceSceneView#root`、4 指手势（`SceneViewGestures`）、周期 `ghostBuster.forceClearNow`（`DocumentView`）。**踩坑·改对对象**：最初 REPLACE `DocumentView` 的 ScreenModeItem，但它阅读时 `visible: globalScreenMode != undefined` 为 false、不控屏，改了没反应；真正控屏的是 `DeviceSceneView` 的 `Epaper.ScreenModeItem{id:content;visible:!screenDriver.globalMode}`——**apply-diffs 能验"选择器命中"、验不了"是不是真正控屏的那个"，靠真机才暴露**（"形状像不等于对"典型）。

**3. 清残影（`page-refresh.qmd`，独立开关 `cjRefresh`）**：从 fast-mono 拆出的**独立能力，彩屏 & 黑白都生效**（`forceClearNow` 是与屏幕模式无关的硬件全刷）。按章（`tocModel`+`currentPage` 派生切章，`DocumentView.onCurrentPageChanged`）或按 N 页（默认 15）触发。

**4. 阅读字体增强（`font-menu/add-reading-fonts.qmd`，`fontEnhance`）**：字体菜单**追加不替换**——按 `fontEnhance` 往 `FormatFont` 的 `fontModel.append` 加霞鹜文楷/霞鹜新致宋/KF Readerly 三项，原字体保留。**仅 EPUB**（`epub.setFontName`，PDF 固定版式无字体菜单）；ttf 装 `/home/root/.local/share/fonts/`。**冗余项已删**：旧的 `reader-font-lxgw.qmd`（单字体硬编码中文，英文系统显中文碍眼）与本文件楷体重复，已移除、楷体在 add-reading-fonts 保留。

**默认全关**：部署后阅读行为零变化。fast-mono/清残影页数不再硬编码，4 指手势保留作快捷切换。

## 03｜键盘 Mono（`keyboard-mono.qmd`）

拼音**组词态**把键盘区（`KeyboardPanel` 自带的 `#screenMode`，objectName:"keyboard"）锁 `Mono` 单色快刷、非组词态恢复 `Animation`；绑定 `candidatebar.qmd` 注入的 `cjCandidateBar.visible`，**零 C 改动**。`.166`/`.169` KeyboardPanel.qml 与 .164 逐行一致（`root>keyboardContainer>screenMode[mode:Animation]`），选择器直接适用。真机 `KBD-MONO` 日志硬证 mode 跟随组词态精确切换（组词 true→Mono、收起 false→Animation）。**可观测性提醒**：候选栏本就纯黑白，Mono 化不改静态观感，"快多少"需 A/B 慢动作对比、收益微妙——日志证明机制在工作。（此项与块2 输入法耦合，因是"显示快刷增强"归块4。）

## 04｜笔记增强（跨块，只在此设开关）

- **荧光笔汉字精确吸附**：中文"对齐到文本"划一小段却吸整行/吸不上的根治（clean-room 复现镇纸）。逻辑是 `langhook` 的 C hook（hook 扩张层 `FUN_00f05ad0`，CJK 首字跳过词扩张、保命中精确边界），设置页此处只给开关。**完整反编译/修复记录见《[阅读白皮书](../../reading/docs/reMarkable阅读白皮书.md)》§03 组件3d。**
- **★全局待办**：红笔画星 → 后台 daemon 自动汇总总结卡片。开关在「系统增强 → 笔记增强」，**能力本体见《[PKM 白皮书](../../pkm/docs/reMarkablePKM白皮书.md)》**。

## 05｜离线 qmldiff 验证管线（方法论资产，务必复用）

host 无法运行 xochitl，但能用官方 qmldiff 工具**离线实跑补丁**，验解析 + 选择器命中 + emit 合法 QML——比"结构自洽"强得多：

1. `git clone --recurse-submodules https://github.com/asivery/rm-xovi-extensions`（qmldiff 子模块 = `asivery/qmldiff`，Rust）；`cargo build --release` 得 `qmldiff` CLI。
2. 从**设备** scp 实跑固件的 `/usr/bin/xochitl`（`.166` md5 `5215ef7ab…`；`rmfw/` 里的二进制按需对齐，别用过期版本）。
3. `tools/extract_qml.py <xochitl> <outdir>`（zstd magic `28 b5 2f fd` 全局扫 + 逐块解压）解出真实 QML（.166 = 556 个）；文件名未还原，靠内容 grep 认领目标。
4. 设备 `hashtab`（`.../qt-resource-rebuilder/hashtab`）解析出全部真实资源路径 + 标识符明文，是 `AFFECT` 路径的权威来源。
5. 目标 QML 放真实路径下 `qmldiff apply-diffs <root> <dest> patch.qmd -f -c`——与设备端 qt-resource-rebuilder 走**同一份 qmldiff 代码**，高度一致。

**qmldiff 纪律（血泪）**：
- `INSERT { … }` 块内是 QML，注释必须用 `//`（emit 成 `/* */`）；误用 diff 注释 `;` 会被当 QML token 输出成非法 JS（离线 apply-diffs 抓出过）。
- 无 id 的点号类型选择器要写全（`Epaper.ScreenModeItem`，裸 `ScreenModeItem` 不匹配）。
- 嵌套深埋节点 `TRAVERSE` 必须用通配 `?#id`（非通配只匹配直接子节点，直配 `ColumnLayout#settingsColumn` panic "Cannot locate"）。
- **改 `DeviceSceneView.qml` 只有 root 直接子安全**（fast-mono 的 `FocusScope[#root] > Epaper.ScreenModeItem[#content]` REPLACE mode）；**深层节点属性 REPLACE 会拖垮整文件、连累同文件其它 qmd**（返回浮标延长实验坐实，见 §06 判死）；且 `REPLACE` **只能改属性绑定、不能改函数/信号处理器**（REPLACE 函数报 `Cannot LOCATE Type`）。

## 06｜崩溃自愈 fail-safe + 判死清单

**崩溃自愈 fail-safe**（qmd 无 `.so` 的签名守卫）：`chinese-ime/langhook/deploy/cangjie-qrr-failsafe.sh` 装到 xovi `scripts/pre-start/`，每次 `xovi/start` 用 `journalctl -b -1`（设备 journald 持久）数上一个 boot 的 xochitl 崩溃签名，**≥3 次就把这批阅读增强 qmd 移出 qrr 隔离**（只动自己的、不碰 candidatebar/moxiang/IME），下一个 boot 恢复到"输入法可用"。干净 `systemctl restart` 不产生崩溃签名、不误触发。**一次真实隔离与恢复（2026-08-21）**：墨香 Navigator 整页化 Popup→Item 编译失败致 xochitl 崩溃 4 次，fail-safe 把 6 个阅读增强 qmd 移进隔离区 → 系统增强菜单全掉。**真凶（坏的 moxiang-navigator）修好后**，隔离区 qmd 移回 + 清 `qrr-failsafe.TRIGGERED` + 重启即恢复。**教训：fail-safe 隔离的是"阅读增强这批"、不管真凶是谁——排查看崩溃真凶，别错怪被隔离的无辜 qmd。**

**判死清单**：
- **返回浮标常驻/延长——判死**：脚注跳转后的"Back to page X"浮标默认 8 秒消失（`showNotification(...,8000)`）。逆向定位到位（有未用的 `showWithoutTimeout` 参），但两次实测（改 `messageTimer.interval`）都连累同 `DeviceSceneView.qml` 的阅读增强全失效——是 **REPLACE 深层 Timer 属性机制本身**拖垮整文件，非表达式/大数。**彻底放弃、返回靠原生 8 秒**。
- **改 waveform / 屏幕高刷**——判死（残影/烧屏永久损伤、无回退），见路线图 §4.1。

## 07｜关键 QML 结构参考（.166/.169 真实核对）

- `SceneViewGestures.qml`：根 `TouchArea{id:touchArea}`；单指 `TouchAreaClickFilter{id:touchClick;objectName:"click-1"}` 的 `onClick`；`property var view` 由 DeviceSceneView 以 `view: root` 注入；原生占用 2/3/5 指手势（4 指空闲）；`ghostBuster` 裸引用可达。
- `DeviceSceneView.qml`：根 `FocusScope{id:root}`（= `view`），有 `zoomedIn/zoomedOut/textMode/notePage/page/clickGestureEnabled/document`；控屏实例 = root 直接子 `Epaper.ScreenModeItem{id:content;visible:!screenDriver.globalMode}`。fast-mono 状态挂这里。
- `DocumentView.qml`：根 `FocusScope{id:root}`；`import xofm.libs.epaper as Epaper`；`root.ghostBuster`（`GhostBuster.forceClearNow`）；`currentPage` 有 `onCurrentPageChanged`。
- `Settings.qml`：左侧菜单 `SettingsModel` 驱动 `Repeater`；静态项插 `ColumnLayout#settingsColumn`；内容页 `payloadLoader.sourceComponent`。
- `KeyboardPanel.qml`：`root>keyboardContainer>screenMode[objectName:"keyboard",mode:Animation]`。

## 交叉引用

- 优先级/立项：《[功能路线图白皮书](../../docs/reMarkable功能路线图白皮书.md)》§09。
- 代码 + 操作：[reading-qol/README.md](../reading-qol/README.md)（端点级操作/部署纪律）。
- 荧光笔吸附本体：《[阅读白皮书](../../reading/docs/reMarkable阅读白皮书.md)》§03-3d；★待办本体：《[PKM 白皮书](../../pkm/docs/reMarkablePKM白皮书.md)》；自动优化：阅读白皮书 §07-D。
