# pkm-semantic —— reMarkable 设备端语义引擎（PKM 主线）

> 本目录是块5 PKM 的 **Python 算法原型 + 阈值标定**；完整设计 + 知识化方法论见《[PKM 白皮书](../pkm/docs/reMarkablePKM白皮书.md)》，生产 Rust 在 [`../pkm/`](../pkm/README.md)。

把手写笔迹异步解析成结构化知识。PKM 回归后 reMarkable 复位为 **PKM 阅读/笔记工作台**，
本目录是这条线的"语义引擎"：只读扫描 `.rm` 笔迹 → 识别约定符号 → 输出独立文档（绝不回写原件）。

首个能力：**★ 全局待办**——扫描所有文档的手绘五角星标记，汇总成一份全局待办清单。
（选它打头因为它①不依赖底层文本 ②颜色/形状能从 `.rm` 读出 ③输出独立文件天然绕开
`inplace 判死`+云同步冲突 ④架构上就是给现有 wr-serve daemon 加一个 inotify 消费者。）

**标记约定（真机对账后定案）**：`专用笔色 + 星形（自相交）` 双条件。用一种记笔记不用的
笔色画待办星（如红/蓝）；检测 = 该笔色内 + 紧致 + 自相交 ≥ 5。**自相交是核心不变量**——
星之为星就在笔画交叉：pentagram 恒 5 个、真机松散星 10~20 个，圆/方框/对勾/正常字母是 0。

## 现状：Python 原型（`proto/`，host 侧）

按项目纪律"有 Python 参照的算法先 Python 后 C（Rust）"：先在 host 用 rmscene 把算法 +
差分测试跑通并标定阈值，再移植到设备端 `device-rs`（生产解析用那份 byte-exact 的
`remarkable_lines`，非 rmscene）。

| 文件                  | 作用                                                                                                                   |
| --------------------- | ---------------------------------------------------------------------------------------------------------------------- |
| `geometry.py`         | 纯几何：弧长重采样 / 转角 / 角点 / **自相交计数** / 闭合度（无 rmscene 依赖，可直接对照移植 Rust）                     |
| `star_detect.py`      | ★ 检测器：`StarConfig` + `is_star()`。判据=颜色门控 + 紧致 + 尺寸 + **自相交≥5**                                       |
| `synth.py`            | 合成笔划：正例（一笔画/圆钝五角星）+ 负例（对勾/括号/圆/方框/下划线/字母/V/涂鸦）                                      |
| `rm_strokes.py`       | 只读 `.rm` → `Stroke(points,color,tool)`。含 `_enum_name`（PenColor 是 IntEnum，Py3.11+ str() 返数字，必须走 `.name`） |
| `star_scan.py`        | 扫描 xochitl 镜像 → `Global_Todo.md`(+JSON)。颜色门控 + **多笔星空间合并**（`--todo-color RED`）                       |
| `test_star_detect.py` | 差分测试（18 条）：正例≥90%、负例≤5%、自相交不变量、颜色门控、真机 fixture 回归、IntEnum 回归                          |
| `testdata/`           | **真机 fixture**：《缺失功能》（含红色手绘星 + 红干扰 + 黑色笔记），从设备拉回                                         |
| `tune.py`             | 阈值标定：打印各形状特征分布 + 命中率                                                                                  |

### 跑起来

```bash
python3 -m venv .venv && ./.venv/bin/pip install -r proto/requirements.txt
cd proto
../.venv/bin/python -m pytest -q          # 18 passed（含真机 fixture 回归）
../.venv/bin/python tune.py               # 特征标定表
# 真机 fixture 试跑（用户把星画成红色）：
../.venv/bin/python star_scan.py --src testdata --todo-color RED --out Global_Todo.md
../.venv/bin/python star_scan.py --src <xochitl镜像> --todo-color RED --out Global_Todo.md
```

### 标定结论（合成数据）

- 正例（clean/圆钝五角星）在 ≤2.5% 笔迹抖动下检出 **100%**。
- 全部负例（对勾/括号/圆/方框/下划线/字母/V/涂鸦）误检 **≤1%**——自相交≥5 干净分离。

### 真机对账（`testdata/`《缺失功能》第2页，2026-08-22）

关键发现，全部改写了原设计：

1. **真手绘星 ≠ 理想五角星**：是**多笔叠加、外廓圆钝、内部自相交**的松散手势。原"5 尖角 + 闭合"
   阈值全漏 → 改判据为 **自相交**（pentagram 不变量），并加**空间合并多笔星**。
2. **黑对黑纯几何不可用**：满页黑色手写里，草书汉字也有 4~11 个自相交 + 紧致 → 纯几何
   每页约 **8 个假阳**（笔序 9/98/107/110/155…）。→ 坐实必须**颜色门控**。
3. **颜色 + 形状缺一不可**：用户把星画成红色，但红笔里也有干扰（红对勾/红方框）→ 只筛颜色不够，
   仍需星形（自相交）区分。双条件下 RED 门控检出 6 个红星、正确忽略红对勾/红方框；GREEN/BLUE 门控→空。

## 已知边界 / 待办

- **松散星漏检**：真机 ~13 个红星里检出 6 个最清晰的（自相交高）；画得太圆钝（自相交 ≤4）的会漏。
  多收几页样本可下调 `self_int_min` / 调 `cluster_gap` 再标定（当前 gap=25 稳定；40 起过度合并）。
- **多笔星合并靠空间聚类**：`cluster_gap` 太大%会把相邻星并成一个（计数偏少）。生产环境里专用色只有星、
  页面稀疏，聚类更干净。
- **rmscene 版本 < 当前固件**：`read_blocks` 刷 "newer format" 警告（已静默）、可能跳尾部数据。
  笔划点数据完整度须与 `device-rs` 的 `remarkable_lines`(byte-exact) 交叉核对——生产解析用那份。

## Rust 移植（已完成，逐字节对拍 Python）

`pkm/src/stardetect.rs` + `pkm/src/bin/wr_stars.rs`（块5 独立 crate，2026-08-23 从 reading 抽出）——纯 aarch64 静态二进制。

- **对拍验证**：同一真机 fixture，Rust `wr-stars` 与 Python `star_scan` 在 5 个 `cluster_gap`（6/6/6/6/4）
  与各色门控**逐一全等**；**200 笔逐字段全等**（color / self_int / size / aspect）。几何全用 f64
  （Point 是 f32，上采 f64 与 rmscene/Python 一致）。回归测试 `pkm/tests/stars_fixture.rs`。
- **vendored `remarkable_lines` 新格式补丁**（当前固件比 crate 新）：① `ParagraphStyle::Unknown(u8)`
  容忍新样式码 6（原来遇到就整文件解析失败）；② 块少读时跳到块尾而非报错（rmscene 也只是
  "some data not read"）。补丁后 Rust 读到全部 200 笔，与 rmscene 一致。
- aarch64 musl 全静态产物 ~509KB。用法同 Python：`wr-stars <镜像> --todo-color RED [--cluster-gap 25]`。

## 设备 daemon（✅ 已交付，全真机端到端）

> **注**：下面几条是 daemon 初版（A 模型：直写 EPUB 全局索引）的设计。**最终交付是 B 模型**（一书一份可编辑
> 打字卡片 + `/upload` 活注入 + 事件驱动去重），与初版差别很大——**完整最终形态见 [PKM 白皮书](../pkm/docs/reMarkablePKM白皮书.md) §04-05 与记忆
> `pkm-star-todo-proto`**。关键更正：①注入走 `/upload` 不是直写（xochitl 无视磁盘直写）；②去重走 `trash-agent.qmd`
> 挂文档模型 `onRowsInserted` 纯事件驱动（不是 Timer）；③输出是每书一份「总结卡片」笔记本（一星一页 + 章名 +
> 总结卡片模板 + 打字批注保留），不是单本全局 EPUB；④设置页「系统增强 → 笔记增强」已加开关。

`pkm/src/bin/wr_stars_daemon.rs`（+ 复用 `reading/device-rs/src/fswatch.rs`）——**独立后台服务**（不并进 wr-serve）。

- **事件驱动 + 防抖，省电**：复用 `fswatch`（inotify + 防抖），**空闲阻塞睡死、零周期唤醒**
  （周期扫描是电池刺客，弃用）；systemd 侧 `CPUQuota=30%`+`MemoryMax=64M`+`Nice=10`——
  纯后台无交互，硬帽放对了地方（不像 wr-serve 会误伤交互式下书/优化）。**同一改法也把 wr-serve
  原来那个 60s 自动优化轮询改成了事件驱动**（同病同治）。
- **全设备自足、无网络依赖**：直写 `XOCHITL_DIR`（`write_document_files` 建 /`update_epub_inplace` 更新，
  删派生件逼 xochitl 重渲），**不走 10.11.99.1 上传端点**（设备不插 USB 时那个未必可达）。
- **变更检测省 churn**：结果指纹（哪本书哪页几颗星）不变则跳过重出文档，绝大多数非星区 .rm 写入零 churn。
- **输出**：一份「★ 全局待办」EPUB 注入书库（原生不支持跨文件超链接 → 列"书名·第N页"，靠全局搜索软跳转）
  - `stars.json`/`Global_Todo.md` 落 `/home/root/weread/`。
- **配置**（`reading-qol.json`，与设置页共享）：`starTodoEnabled`(默认关)/`starTodoColor`(默认 RED)/`starTodoGap`(25)。
- **observe 模式**（`CANGJIE_FSWATCH_OBSERVE=1`）：只打日志不写文档，真机首轮观察 xochitl 落 .rm 的真实事件序列。

host 验证：全量测试 19 + fswatch 防抖 1 + stars fixture 5 全绿；daemon observe 冒烟对 fixture 见 6 处星；
全部 aarch64 musl 静态二进制编成。

## 已交付（全真机端到端，2026-08-22）

部署（`wr-stars.service` 装 `/usr`、`CPUQuota=30%`、开机自启）、检测、章名映射、卡片生成/merge、`/upload` 注入、
事件驱动去重、设置页开关——**全部真机验证通过**（用户逐项确认）。三服务 active、NRestarts 0、每书恒 1 张卡。

**已知边界 / 后续**：

1. **松散星漏检**：真机 ~13 星检出最清晰的几个；画太圆钝（自相交 ≤4）的会漏 → 多收样本下调 `self_int_min` / 调 `cluster_gap`。
2. **节名**：toc 平铺无子节，现只填章名（`★ 章名·第 N 页`）；遇到带子节的书再扩展成 `章名 - 节名`。
3. **擦星语义**：擦掉一颗星 → 该卡片页连同上面打的字一起消失（star = 待办本身，擦星即完成）。
