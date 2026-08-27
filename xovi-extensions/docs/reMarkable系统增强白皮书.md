# reMarkable Paper Pro Move 系统增强白皮书（块4）

> 系统增强线（阅读/显示/笔记 UX）设计与真机调试记录 · 2026-08
>
> 本白皮书是块4「系统增强」的设计单一事实来源。跨块优先级见《[功能路线图白皮书](../../docs/reMarkable功能路线图白皮书.md)》§09；
> 代码在 `xovi-extensions/reading-qol/`（+ `font-menu/`）；设置页面板 qmd 是 `settings-reading-enhance.qmd`。
>
> ⚠️ **跨块**：① 「笔记增强 → 荧光笔汉字吸附」逻辑是 `chinese-ime/langhook` 里的 C hook（块2 的 .so），完整设计见《[阅读白皮书](../../reading/docs/reMarkable阅读白皮书.md)》§03 组件3d；② 「笔记增强 → ★全局待办」开关只是门户，能力本体见《[PKM 白皮书](../../pkm/docs/reMarkablePKM白皮书.md)》；③ 「导入书籍自动优化」开关消费方是 wr-serve（Rust），见《[阅读白皮书](../../reading/docs/reMarkable阅读白皮书.md)》§07-D；④ **「设备端查词 · 生词本」代码骑在 pkm daemon 上（复用荧光笔读回+笔记本注入管线，不单拆二进制），但概念属块4系统增强，完整设计在本白皮书 §08**（区别于②③——查字词的**本体文档就在这里**，不是只放开关）。本白皮书对①②③只讲开关的**门户/面板机制**，对④讲**完整能力本体**。

## 00｜定位

给 xochitl 原生阅读器/系统加一层 UX 增强，**纯 QMLDiff 注入、不碰硬件、不改 waveform、不进 `xochitl_pdf_renderer`**，全部默认关、用户在设置页逐项开。缘起：竞品 [pretenderlu/rmtool](https://github.com/pretenderlu/rmtool)（GPL-3.0）的 `tap-page-turn` 与 `fast-mono-reading`——**参照其揭示的机制净室重写，不复制其 QMD 文本或代码**。许可证隔离沿用项目纪律：QMD 是"往哪个 QML 插什么"的独立文本补丁，运行时由 qt-resource-rebuilder 加载，不编译进 `cangjie-langhook.so`。

## 01｜设置页「系统增强」门户（2026-08-17 接入，hub 中枢 + 四分类真机验证）

**入口不走阅读器 FormatMenu，而是设置 App**：`settings-reading-enhance.qmd` 往 `Settings.qml` 左侧菜单**最下方**插「系统增强」`ArkControls.SidebarItem`（设置页用 `onTriggered`）+ 一个 **hub 中枢页**（`objectName:"cjEnhanceHub"`，哨兵 990001）。中枢页不直接堆开关，而是列**四个并列子分类**行（大标题 + 说明 + `›` + 发丝线，`MouseArea` 点击切 `_selectedPage`）；每个子分类是一个**独立 `Component`**（各自 `INSERT ... LOCATE AFTER Component#help`），靠 `_selectedPage` 哨兵切换 `payloadLoader.sourceComponent`（`REBUILD`+`LOCATE AFTER STREAM /{/` 注入早返回）。菜单是 `SettingsModel` 驱动的 `Repeater`（项不在 QML 里），故往 `ColumnLayout#settingsColumn` 插静态项。四分类各带独立哨兵：**990001 中枢、990002 快捷输入、990003 翻页与刷新、990004 书籍与字体、990005 笔记增强**；子页顶部「‹ 系统增强」返回即置回 990001。

**四个子分类 + 开关清单**（除快捷输入用独立文件外，其余写 `reading-qol.json`）：

1. **翻页与刷新**（990003）：① 点击翻页 `cjTapPageTurn`（键 `tapPageTurn`）② 快速黑白 `cjFastMono`（`fastMono`）③ 清残影 `cjRefresh`（`refresh`，+按章 `cjRefreshByChapter`/每 N 页 `cjRefreshEvery` 默认 15）。
2. **书籍与字体**（990004）：④ 阅读字体增强 `fontEnhance` ⑤ 导入书籍自动优化 `autoOptimize`（默认关，消费方 wr-serve）。
3. **快捷输入**（990002，snippets）：缩写→短语的**文本替换**管理页，UI 支持增删改，持久化到 `snippets.tsv`（详见 §01a）。
4. **笔记增强**（990005，**5 开关**，详见 §08 开关块）：⑥ ★全局待办 `starTodoEnabled`（主·默认关，+颜色 `starTodoColor`/间距 `starTodoGap`，能力本体在 PKM 白皮书）→ 下挂两子开关 `cardHighlights`（划线摘录〔=用户口语「荧光笔」〕·高亮是否入卡·**默认关**〔2026-08-27 规则a：星代办单开=空白模板，需再开此开关才摄取，QML 取缺省用 `=== true`〕）/`cardAggregates`（跨书汇总·4本汇总本·默认开），随主开关灰化失效；⑦ `vocabEnabled`（单词笔记·生词本·**独立顶层**·默认关，脱离画星）；⑧ 荧光笔精确吸附汉字 `hlSnapCjk`（**默认开**，C hook 消费，缺省即视为开——`c.hlSnapCjk !== false`）。

**跨 QML 树共享状态 = `reading-qol.json`**（`/home/root/.local/share/cangjie-ime/reading-qol.json`，/home 持久）：设置页 QML 用 `XMLHttpRequest` 写、阅读页 QML 读。运行时 xochitl 带 `QML_XHR_ALLOW_FILE_{READ,WRITE}=1`。
- **大坑：同步 PUT 到 `file://` 只截断不写体 → 写必须异步**（open 不带 `false`）；读同步 GET 正常。
- **传播靠 `reading-qol-config.qmd` 在 `DeviceSceneView#root` 的 1.5s 轮询 Timer**（`onCompleted` 只触发一次、返回阅读器不重建，"改了不生效"就是缺这个轮询）；字体菜单例外（构建那刻读一次、退出重开生效）。
- **★全量防覆盖铁律**：每个写 `reading-qol.json` 的子页都必须**读写全量键**——翻页页/书籍页除自身开关外，也要读进并写回笔记增强页的全部键 `starTodoEnabled/starTodoColor/starTodoGap` + `cardHighlights/cardAggregates/vocabEnabled`（默认**关/开/关**，取缺省：`cardHighlights` 用 `=== true`〔默认关〕、`cardAggregates` 用 `!== false`〔默认开〕、`vocabEnabled` 用 `=== true`〔默认关〕）+ `hlSnapCjk`，否则在翻页/书籍页保存会抹掉这些值（`starTodoEnabled` → daemon 读成 false 功能被意外关；子开关/单词笔记同理）。三个二级页均已带上（笔记页渲染、另两页透传）。

## 01a｜快捷输入 snippets（文本替换，真机通）

「系统增强 → 快捷输入」（哨兵 990002，`cjSnippetsPage`）是**缩写自动展开为常用短语**的文本替换管理页：用户在任意输入框敲缩写（如 `bqq`），输入法把它替换成预设短语。设置页此处是**增删改 UI**——列表页列出所有「缩写→短语」条目（每行带编辑/删除按钮）+ 顶部「+」新增，编辑页两栏（缩写 / 短语）+ 保存/取消，`cjByteLen` 用 `encodeURIComponent` 算 UTF-8 字节做长度校验。

- **持久化**：条目存 `/home/root/.local/share/cangjie-ime/snippets.tsv`（TSV，一行一条），设置页用 `XMLHttpRequest` 同步 GET 读、异步 PUT 写（同 `reading-qol.json` 的 `file://` 写坑：写必须异步）。输入法侧 `langhook` 热重载该表匹配展开。
- **能力本体**：展开逻辑 + 逻辑时钟半衰调频等在块2 输入法，真机端到端已部署（：snippets.tsv 热重载 + 跨输入框泄漏修复）；本白皮书只讲设置页这一侧的增删改门户。

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

- **荧光笔汉字精确吸附**（开关键 `hlSnapCjk`，**默认开**）：中文"对齐到文本"划一小段却吸整行/吸不上的根治（clean-room 复现镇纸）。逻辑是 `langhook` 的 C hook（hook 扩张层 `FUN_00f05ad0`，CJK 首字跳过词扩张、保命中精确边界），设置页「笔记增强」页此处只给开关，写 `reading-qol.json` 供 C hook 读（缺省即视为开，故其余子页保存时也要全量写回、勿抹）。**完整反编译/修复记录见《[阅读白皮书](../../reading/docs/reMarkable阅读白皮书.md)》§03 组件3d。**
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

## 08｜设备端查词 · 生词本「📕 生词本」（跨块：代码在 pkm daemon，2026-08-25 真机端到端验证通过）

查字词是**系统增强线的阅读辅助**（读书时划生词自动查词），但**代码本体骑在 pkm daemon（`wr-stars-daemon`）上**
——与 §04 荧光笔吸附（代码在块2 langhook .so）同构的第 4 处跨块：能力概念属块4，实现复用了 pkm 的荧光笔读回
（`cardhl`）+ EPUB 章映射（`epubindex`）+ 笔记本注入（`sync_auto_notebook`）管线，**不为它单拆一个二进制**。
本节是查字词设计的单一事实来源；实现在 `pkm/src/{dict,cardvocab,locate,vocabscan}.rs`，daemon 只触发+注入。

**可行性核查结论**（见《[设备端查词可行性](../../docs/reMarkable设备端查词可行性.md)》）：Move 上 KOReader 判死、
原生阅读器无逐词选中事件可 hook、注入弹窗够不进 SceneView tile 层——实时划词弹窗走不通；而**荧光笔读回文字**
（GlyphRange 自带文字+颜色，PKM 白皮书 §06 已生产）成立，故查词的可行形态是**异步**：荧光笔划词 → daemon 查本地
词典 → 写回一本《📕 生词本》。

**设计**（三个岔口与用户敲定）：
- **触发=复用 ⚪Gray 槽**：灰色高亮=生词。灰高亮**照常进卡片灰槽**（PKM §06 不变），查词是**完全解耦的并行附加
  扫描**，另出《生词本》；查不到的（人物名/专名）自动不列。
- **脱离画星**：任意已标注页的灰高亮都查（不必画★）。daemon 对源书**全部已标注页**枚举灰高亮
  （`enumerate_pages` 复用 `stardetect::page_order`，不再只取星页）。
- **带原句**：`locate::locate_range`（部首 `canon` 规整 + 章内 `char_find`）在 `epubindex::page_fulltext` 取的
  EPUB 章全文里定位词 → 向两侧扩到句界得整句。
- **词典双向**：英文词 → **牛津高阶英汉双解**（一部即英释+中译+例句）；中文词 → **现代汉语词典**（中文释义）。
- **划一句/短语怎么办**（`dict::lookup_phrase`）：**整体精确命中优先**（单词/成语/词典里的短语，如"踌躇满志"直接
  命中）；miss 才拆——**中文词级最长匹配**（只取 ≥2 字的词，单字连接词天然跳过，不是拆成单字：现汉验证"他一直很
  踌躇"→「一直」「踌躇」）、**英文按词拆并跳停用词**（the/a/of…）；**整句不拆**（长度门控，划整句更像"金句"非查词）。封顶防刷屏。

**实现**：
- `dict.rs`：本地词典 **mmap + 行首二分**（`memmap2`，RAM 与词典大小无关，适配设备内存）；`detect_lang`
  判中/英选词典、`normalize_key` 与建表侧对齐。数据 = `build_dict.py` 离线把用户自备 MOBI 经 calibre 转 HTML
  再解析（`<span class="bold">词</span>…释义<hr/>`）出的**排序 TSV**。
- `cardvocab.rs`：`sentence_of`（句界扩展）/ `cap_body`（牛津长释义截断）/ `render_notebook_pages`（纯逻辑可测，**首页=封面〔标题+计数〕，其后每本书各占一页**，避免全堆一页过长）。
- `vocabscan.rs`：**扫描编排**（全库源书 → ⚪灰词 → 查词 → 生词条），从 daemon 抽出成自足模块，**词典路径作
  参数传入**（路径无关、可测）；只依赖共享基础设施（`cardhl` 读高亮/`epubindex` 页→章+章全文/`stardetect::page_order`
  页序/`dict`/`cardvocab`），**不碰** daemon 的星/卡片路径。daemon 只剩「触发 `need_vocab` + `sync_auto_notebook`
  注入」两件事——最大程度解耦，短于另拆一个二进制（查字词与★待办共用同一 daemon 的 fswatch/注入/去重基础设施）。
- `locate.rs`：`canon`/`locate_range` **搬自原 `reading/device-rs/src/reverse.rs`**——那份是微信读书双向同步的
  逆映射孤儿死代码（无 `mod` 声明、缠着 `rmread`/`notebook`）；只把与死码无关的**纯定位原语**逐字搬进 pkm
  （真机对拍差=0），原 reverse.rs 连同死子系统已 2026-08-25 `git rm` 删除（见 git 历史）。
- daemon `collect_vocab`/`rebuild_vocab`：全库全量扫（`sync_auto_notebook` 每轮整本重生），**独立触发**
  `need_vocab`（冷启动 or dirty 里有**源书**变更，区别于笔记本触发的 `need_index`）。《生词本》列入
  `collect_notebook_texts` 排除名单（防自摄取）。

**版权红线**：牛津/现汉是用户**正版商业词典**，只做**个人自用**：MOBI 与派生 TSV 全部 `.gitignore`、绝不入库/分发，
仓库只留不含词典内容的 `pkm/tools/build_dict.py`；缺词典文件 daemon 该向自动降级不查。

**验证**：
- host 全绿（`dict`/`locate`/`cardvocab`/`vocabscan`/`epubindex::page_fulltext`）。
- 两部词典实测抽取 + 双典 Rust 探针端到端：**牛津 en.tsv 164,493 条 · 现汉 zh.tsv 62,642 条**，字节序正确；
  `cachet`/`abandon`→音标+英汉双解、`踌躇满志`→整体命中、`他一直很踌躇`→词级拆「一直」「踌躇」。
  （牛津 MOBI 经 calibre 完整转换在 CSS-flatten 阶段报错，改用 `--debug-pipeline` 取**输入阶段** HTML 抽词——
  那在报错阶段之前落盘，正是解析所需，2 秒抽完。）
- **真机已部署**（2026-08-25）：daemon 二进制 + `en.tsv`/`zh.tsv` scp 到设备 `/home/root/weread/dict/`，
  备份 `cangjie-backups/wr-stars-daemon.bak.pre-vocab`、md5 本地=设备一致、`systemctl restart wr-stars` 后
  is-active=active/MainPID 变/NRestarts=0/ExecMainStatus=0；冷启动日志 `[stars] 生词本已创建（0 个生词）`
  = **查词管线跑通、生词本已建**（暂无灰词故 0 条）。
- **真机端到端验证通过（2026-08-25）**：用户在《13 67》某页用 ⚪灰色荧光笔划「刑事」「情报」两词（**未画星**）→
  存盘 → daemon 日志 `[stars] 生词本已更新（2 个生词）` → 《📕 生词本》实际生成：
  「刑事 xíngshì {形}…有关刑法的」「情报 qíngbào {名}…消息和报告」+ **原句**（两词同句"…新上任的刑事情报科B组
  主管关振铎警司"）+ **章·页**（泰美斯的天秤 · P369）。每一环坐实：灰词检测（脱离画星）/ 现汉拼音+释义 /
  `locate_range` 原句定位 / `epubindex` 章页 / 词级精确命中。查字词至此**真机端到端验证完成**。

> **开关（2026-08-26 真机验证通过）**：设置页「系统增强 → 笔记增强」页现有 5 开关（用户真机确认 5 开关渲染正常）：
> `starTodoEnabled`(主·默认关) 下挂 `cardHighlights`(划线摘录〔口语"荧光笔"〕·高亮入卡·**默认关**〔规则a·2026-08-27〕)/`cardAggregates`(跨书汇总·4本汇总本·默认开)
> 两个子开关（主关则灰化失效）；`vocabEnabled`(**单词笔记·生词本·独立顶层·默认关**，脱离画星、与★互不依赖)；`hlSnapCjk`(荧光笔吸附)。
>
> **daemon 开关不生效根治（2026-08-27 真机验证）**：`wr-stars-daemon` 原来 fswatch 只 watch xochitl 文档树、不 watch config → 拨开关只写 `reading-qol.json`、唤不醒休眠 daemon，存量已画星书也不回扫（表现"开关像没用"）。修复：`device_core::fswatch::watch_debounced` 加可选 config 文件监听参数 + `CONFIG_SIGNAL` 哨兵，daemon 见到即 `settle(dirty=None)` **全库重扫**（真实 open-write-close 写 config 后 daemon 被唤醒跑全库重扫，真机坐实）。生效的 tap-page/hlSnapCjk 本就在进程内动作那刻现读，故一直正常。
> daemon `wr-stars-daemon` 读这 5 键分别门控三条产出，功能关闭时对应只读自动本(4汇总+生词本)入回收站清残留、星卡片含批注绝不 trash。
> **单词笔记基线（"开前灰词不补"）**：启用时记基线时刻(`vocab-since.txt`)，生词本只纳入 `.rm mtime >= 基线` 的页。
> 首次激活若已有生词本(之前就在用)→基线 0 grandfather 全保留；否则基线=配置 mtime(≈开关打开时刻，避开 fswatch 防抖竞态)。
> 停用删基线，下次干净 off→on 重设 → 只纳入此后画的灰词。
> **归档**：所有生成物上传前 `GET /documents/<zettelkasten uuid>` → 落卡片盒(真机验证 GET-then-upload，见设计建议 §四)。
> 三个二级页都全量写回同一 `reading-qol.json`，故 3 新键在三页 load+save 都带上（本页渲染、另两页透传）。
> **离线门槛**：`qmldiff apply-diffs`（真本 Settings.qml=固件 .169 的 `qml_00db3818`）1 diff applied、5 开关接线正确 emit、括号平衡、emit 可再 parse；daemon `cargo test`(60+5) 绿、aarch64-musl 静态链接。

## 交叉引用

- 优先级/立项：《[功能路线图白皮书](../../docs/reMarkable功能路线图白皮书.md)》§09。
- 代码 + 操作：[reading-qol/README.md](../reading-qol/README.md)（端点级操作/部署纪律）。
- 荧光笔吸附本体：《[阅读白皮书](../../reading/docs/reMarkable阅读白皮书.md)》§03-3d；★待办本体：《[PKM 白皮书](../../pkm/docs/reMarkablePKM白皮书.md)》；自动优化：阅读白皮书 §07-D。
