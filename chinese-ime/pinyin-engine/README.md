# 拼音引擎核心逻辑

对应《[reMarkable 拼音输入法白皮书](../docs/reMarkable拼音输入法白皮书.md)》03 节
（拼音引擎核心设计）/ 06 节（候选词库）。`src/` 是先在 x86 开发机上写通、
写满测试的 Python 参照实现，`c/` 是逐字节差分对拍后移植的 C 版——C 版已由
`chinese-ime/langhook` 直接 include 源码、编进 `.so` 装到设备上，
拼音/双拼/繁体/简拼/中英混输（Phase C）全部真机验证通过。这个目录本身仍可
脱离设备独立开发/测试（`pytest` + `make test`/`make diff-check`）。

## 架构中的位置

本引擎是拼音输入法链路的中段——「按键处理 → 拼音缓冲状态机 → `cj_segment()` 音节切分 →
查 `dict.bin`」这一段（下图右侧链②中部）。完整链路与逆向依据见
《[拼音输入法白皮书](../docs/reMarkable拼音输入法白皮书.md)》§01：

![拼音输入法架构](../docs/architecture-ime.svg)

## 目录结构

```
pinyin-engine/
  src/
    syllables.py     音节表——从真实词库反推出全部合法拼音音节，而不是手写规则表
    segment.py        音节切分：连续字母 -> 音节序列（含隔音符号、"打到一半"处理）
    dictionary.py     候选词典：音节序列 -> 候选字/词，按权重降序（默认雾凇拼音数据源，
                       旧数据源仍可选加载，见下方"数据来源与许可"）
    shuangpin.py       双拼预处理层：双拼按键 -> 标准拼音（复用 RIME 官方 schema 数据）
    traditional.py     繁体转换层：to_traditional()（commit 时转换，旧设计）+
                       build_traditional_table()（离线批量生成繁体候选表，05 节
                       设计修订，供 C 端繁体双 blob 用，见下方说明）
    jianpin.py          简拼倒排索引：词条 -> 各音节首字母拼接的 key（拼音输入法
                        白皮书 07 节设计的落地，跟 query() 的全拼路径独立并存）
    mixed_input.py       中英混输的 Shift 直通状态机原型——⚠️ 设备端未采用这套
                        （见下方第 9 条），此文件为被取代的存档
    engine.py           把上面几层接起来，对外的主入口
  ui/                真跑通的 QML 候选栏原型（PySide6 离屏渲染），见下方说明
  tests/             71 个单元测试，覆盖白皮书 9.2–9.4 节和《拼音输入法白皮书》
                     05/07 节点名的边界情况
  data/              RIME 生态词库/双拼 schema 数据（见下方"数据来源与许可"）
  c/                 音节表/切分/候选词典/简拼索引/繁体词典的 C 移植版（见
                     c/src/*.h 设计说明），跟 src/ 的 Python 实现逐一有对应的
                     差分测试或专属单测；切分/词典/简拼/繁体双 blob 均已接入
                     chinese-ime/langhook 并真机验证（含双拼，见
                     《拼音输入法白皮书》Step Y）；候选逻辑做成真正的逐字输入法
                     （Phase A：前缀候选/分段提交/退格撤销，白皮书 02 节）。中英
                     混输已接入设备（Phase C，白皮书 07 节）：词典驱动的 iOS 式
                     前缀补全，english.bin（SymSpell，复用 dict.bin 格式）；仅
                     “连打 nihaohello 一次成句”的联合切分后置；mixed_input.py 的
                     Shift 状态机未采用（被取代的原型）
```

## 跑起来

```bash
cd pinyin-engine
/path/to/venv/bin/python -m pytest tests/ -v   # 71 passed

# 交互试一下
/path/to/venv/bin/python -c "
from src.engine import PinyinEngine
e = PinyinEngine()
print(e.query('nihao').candidates[:5])                       # ['你好', '拟好', '你', '尼', '泥']
print(e.query_shuangpin('vsgo', scheme='flypy').candidates[:2])  # 小鹤双拼 vsgo = zhong'guo -> ['中国', '种过']
print(e.commit('网络', region='tw'))                           # 網路（候选栏本身仍显示简体，commit 才转换）
print(e.query_jianpin('zg')[:3])                              # 简拼 z+g -> ['中国', '这个', '整个']
"
```

本仓库用的是 `rmfw/.venv`（`uv pip install --python rmfw/.venv/bin/python pytest`
装好的），没有独立建 venv。

## 设计要点 / 踩过的坑

1. **音节表不是手写的**——现代汉语声韵母拼合规则例外多（j/q/x 只配部分
   韵母、b/p/m/f 各自能配的韵母都不同），手写错的代价是切分全错。改成
   直接从 RIME 官方 `luna_pinyin.dict.yaml` 里提取"实际出现过的全部音节"，
   权威且和候选词典用的是同一份真相来源。

2. **切分算法**：隔音符号 `'` 是硬边界（`xi'an` 强制切成 `xi`+`an`，不
   带符号的 `xian` 按正词法惯例整体算一个音节）；没有歧义时用贪心最长
   匹配（`xianggang` -> `xiang`+`gang`，不是 `xi`+`ang`+`gang`，是白皮书
   9.2 节点名的例子）；支持"最后一个音节还没打完"（比如打到 `zho`，
   识别成"确定前缀为空 + pending='zho'"，交给前缀匹配去处理，而不是
   直接报错切不出来）。

3. **候选词典源自 RIME luna_pinyin，但发现了两个用之前没预料到的坑**：
   - `luna_pinyin.dict.yaml` 本身只收单字全集 + 少量手工维护的固定
     词组，"你好"这种最基础的常用词反而不在里面——frontmatter 里的
     `use_preset_vocabulary: true` 是关键，真机上 Rime 靠另一份
     "preset vocabulary"（词频表，只有"词+次数"不带拼音）跟单字拼音
     实时组合拼出词组候选。对应做法是额外拉取 `rime-essay-simp` 仓库
     的 `essay-zh-hans.txt`（这就是这份 preset vocabulary），用"逐字
     取该字权重最高读音"拼出拼音 key，词典没有的词组从这里补。
   - **词典里的百分比权重列语义容易用错**：那一列（比如"的 de 99.97% /
     di 0.03%"）是"同一个字的多个读音里选哪个"，只在字符内部有意义，
     不能跨字符比较！实测踩到的坑：单独查 `guo` 这个音节，"掴"
     （guai/guo 两个读音里 guo 占 97.33%，但这只是"掴"自己内部的读音
     占比）如果直接拿这列排序会排到真正常用的"国"（因为只有一个读音，
     没有百分比可标）前面——生僻字排到常用字前面，候选体验直接不可用。
     修正方法：跨字符/跨词的排序统一换成 essay 语料的真实频次
     （`test_dictionary.py::test_common_char_outranks_rare_char_with_same_syllable`
     是这个坑的回归测试），词典百分比权重只保留在"同一个字选主读音"
     这一层内部用。

4. **简繁混杂的坑**：`luna_pinyin.dict.yaml` 的单字/词条源头（萌典等）
   本身以繁体字形为主，同一个字的不同繁简变体常常都收在词典里（比如
   "麼"权重 99.93% 远高于"么"的 1%）——如果只按（修正前的）权重列取
   候选，会出现"今天天气怎麼樣"这种简繁混排的输出。改用语料频次排序后
   这个问题连带解决了（因为 essay 语料本身是简体语料，"怎么样"的频次
   远高于繁体"怎麼樣"），但如果以后要覆盖更偏僻的字/词，简繁混杂这个
   风险没有被系统性排除，只是被现在这批高频词"顺带"绕开了——白皮书
   9.4 节设计的 OpenCC 转换层需要覆盖到候选生成这一层，不能只当成
   "翻译 UI 字符串"和"打字上屏后再转换"两处用，是本次实现补充出来的
   认知，已同步更新进白皮书。

5. **双拼按键映射不能凭记忆手写**——同一个按键在不同上下文代表的韵母
   会变（比如小鹤双拼里 `k` 键在 zh/ch/sh/r/z/c/s 后面代表 `uai`，其它
   声母后面代表 `ing`），手写错一条的代价是那一批音节全解码错。改成直
   接复用 RIME 官方 `rime-double-pinyin` 仓库的 schema 文件——这些
   schema 里 `translator.preedit_format` 那段正好是 RIME 自己用来把
   "双拼按键"实时转成"完整拼音"的正则替换规则链（RIME 的 algebra DSL），
   照搬这条链跑一遍，不重新发明。**顺带纠正了白皮书 9.3 节一处凭记忆
   写错的例子**："小鹤双拼里 `vg` 代表 `ang`"是错的，实测 `vg` 解码出来
   是 `zheng`，"昂"（零声母 ang）实际按键是 `ah`。

6. **繁体转换层按 9.4 节设计的"只在 commit 时转换"接了一层**：候选栏
   本身（`query()` 返回值）保持简体不变，只有 `engine.commit(word, region)`
   这个转换点才生效，复用 8.2 节验证过的同一套 OpenCC 词汇级配置
   （`s2twp`/`s2hk`），单元测试直接对拍验证转换结果和 8.2 节实测表一致
   （"网络" tw→網路，hk→網絡）。

7. **简拼字母不需要一张手写的声母映射表**——《拼音输入法白皮书》07 节
   设计的"zh/ch/sh 简拼字母取首字母"，落地时发现根本不需要真的编码
   一张查表：`jianpin_letter(syllable)` 永远等于 `syllable[0]`，因为
   `zh`/`ch`/`sh` 这三个双字母声母本身的第一个字符就是 `z`/`c`/`s`，
   跟单字母声母、零声母音节的处理方式在字符层面天然统一（见
   `jianpin.py` 模块文档字符串）。索引只收多音节词——单音节词的简拼
   字母跟它全拼的第一个字母是同一个东西，查询效果被 `segment.c`/
   `segment.py` 的 pending 前缀匹配天然覆盖，重复收录没有额外收益。

   **`PinyinEngine.query_jianpin()` 接入时被自己的单元测试当场打脸过
   一次，值得记一笔**：最初实现是"精确匹配（key 长度正好等于输入长度）
   优先，不够 limit 个再用前缀匹配补位"，跟 `query()` 全拼路径的分层
   逻辑抄了同一个模式。真的用 `test_query_jianpin_prefix_fills_in_when_exact_is_sparse`
   测了才发现：`"py"` 这个 key 精确匹配的 2 音节词本身就有 213 个
   （"朋友"/"培养"/"便宜"……权重都不低），随便一个 `limit`（哪怕设到
   200）都会被精确匹配自己占满，"拼音输入法"（`pin yin shu ru fa`
   -> key `pysrf`，只有前缀匹配才能覆盖）永远排不到前缀匹配补位的
   机会——精确匹配的结果集本来就是前缀匹配结果集的子集（`"py".startswith("py")`
   为真），拆成两段查询完全是画蛇添足，还引入了这个真实 bug。改成直接
   用 `JianpinIndex.lookup_prefix()` 一次查询、纯按权重排序，问题
   消失。这个坑跟白皮书拼音分册 06 节 `dictionary.c` 那次"攒够 512 个
   候选就不再看后面的 key、导致权重更高的候选被挤丢"是同一类型的
   错误，区别只是这次是在设计/单测阶段被抓到，不是等接了真实数据规模
   跑到真机上才暴露——本身就是"先写差分/边界测试，再信任实现"这条
   项目纪律起作用的例子。

   **C 移植（`c/src/jianpin.c` + `c/src/jianpin.h`）已完成**，跟
   `dictionary.c` 同一套模式（排序 key 数组 + 二分查找 + mmap 只读、
   流式 top-k 前缀匹配），差分测试（`c/tests/diff_check_jianpin.py`）
   11/11 条完全一致，11 个 C 单元测试全过，ARM64 交叉编译零警告。
   **跟白皮书 07 节最初设计有一处偏离，明确记录**：07 节原话是"候选
   文字/权重复用主 `dict.bin` 已有的候选池，用偏移量引用，不重复存储
   文字"，真正实现时改成了自成一体的独立 blob（`dict_jianpin.bin`，
   15.0MB，自己存一份候选词文字+权重）——偏移量引用要求这份 blob 和
   `dict.bin` 的候选表生成顺序/下标严格对齐，一旦两个生成脚本以后各自
   演化（换数据源、加扩展词库），下标会悄悄失效却不报错，是 mmap 场景
   下最不想要的一类 bug；改成两份数据各自独立生成、独立校验、运行时
   互不依赖，代价只是多占约 15MB 磁盘（`/home` 分区 45.8GB 可用，不是
   紧张资源）。详见 `c/src/jianpin.h` 顶部说明。

8. **繁体双 blob——05 节设计修订，离线生成，C 端零改动**：最初的
   `to_traditional()`/`engine.commit()` 是"候选栏保持简体、选中那一刻
   才转换"，能跑但不满足"繁体模式下候选栏本身要显示繁体字、跟简体模式
   功能完全对等"这个要求。改成新增 `build_traditional_table()`——把一份
   `Dictionary` 候选表整体转换成繁体（默认 `region="tw"`，跟设备语言
   切换器目前只有"简体中文"/"繁体中文"两项对应，不生成 `hk` 版本），
   同一个音节 key 下如果多个候选转换后撞成同一个词（真实碰撞用例：
   "谙"和词库自带的繁体异体"諳"，见 `test_build_traditional_table_merges_real_collision`），
   权重相加、重新按权重降序排序——不能留到运行时处理，C 端只做只读
   查询。生成工具 `c/tools/gen_dict_zh_tw_blob.py` 用它产出
   `dict.zh_tw.bin`：跟 `dict.bin` 完全同一个 `CJDICT01` 格式（打包
   逻辑抽成 `c/tools/blob_format.py`，两个生成脚本共用，不重复维护），
   `c/src/dictionary.c` **不需要改一行代码**就能打开——这正是 05 节
   "C 引擎完全不变，只是 mmap 哪个文件"这个设计主张的意义所在，专门
   写了 `c/tests/test_dictionary_zh_tw.c` 验证这个主张成立（打开
   `dict.zh_tw.bin`、查到"網路"而不是"网络"、合并权重正确），不是
   重新测一遍 `dictionary.c` 本身。转换用 OpenCC 的 Python 绑定，
   58.9 万条候选实测约 4.4 秒转换完，不是性能瓶颈。

9. **中英混输——Python 原型是 Shift 直通状态机；设备端未采用（原型保留存档）**：
   ⚠️ 这份 Python 状态机没有移植进设备。设备端中英混输的现状是：整串
   英文靠回车原样上屏（拉丁字母提交）、首字母大写英文靠 Shift 直接透传；
   曾短暂做过"把用户敲的字母作为可点候选"（白皮书 07 节 Step FF），但做
   逐字输入法时按 iOS 惯例移除了（候选栏只留词候选，Phase A A4）。iOS 那种
   `你好hello` 候选**已由 Phase C 完成并真机验证**（白皮书 07 节）：设备端
   英文词典 `english.bin`（SymSpell 82k 词频表，复用 `dict.bin` 同格式），
   打 `nihao`→选"你好"→打 `hello`→点英文候选得"你好hello"，全拼/简拼/双拼
   三模式统一；仅"连打 `nihaohello` 一次成句"的联合切分后置。`mixed_input.py`
   作为被取代的原型保留（下面这段描述的是原型逻辑，不是设备现状）。
   `mixed_input.py` 的 `MixedInputState` 只处理"要不要进入西文直通
   模式、缓冲区怎么变、什么时候提交"这几件事，不依赖 `segment.py`/
   `dictionary.py` 的任何一行——这是设计本身决定的（Shift 触发的西文
   直通是纯粹的按键状态机，不需要拼音引擎参与判断），不是偷懒省事。
   触发规则：缓冲区为空时敲的字符键是 Shift（大写）态就进入西文直通、
   小写态就返回 `PASSTHROUGH_KEY`（交给拼音模式，这个类不管）；已经
   在西文直通模式后字符原样追加、大小写不做任何规范化；空格/回车/
   退格/其它按键（CapsLock 等）都有明确规则（分别对应
   `handle_space_or_enter`/`handle_backspace`/`handle_other_key`，
   跟 `hook_init.c` Step T 对拼音缓冲区的"先提交/清空、再正常处理"
   安全网是同一个模式，方法命名和行为特意保持对称，以后翻译成 C 的
   时候少一层"这两处到底是不是同一个逻辑"的判断）。15 个单元测试
   覆盖全部状态转移，包括"提交完一个词后紧接着敲小写字母，应该重新
   判定为交给拼音模式，不是残留状态"这种容易漏掉的边界。

## 已知局限（MVP 范围内没做的事）

- 候选排序默认数据源（`base.dict.yaml`）自带的整数词频已经是跨字符
  可比的权重，不再需要额外语料表兜底；但仍然没做 9.2 节提到的"二元
  词频（结合上一个已上屏的词调整排序）"，纯 unigram。
- Python 版 `Dictionary.lookup_prefix` 仍然是线性扫描——这一层没有
  换掉，是刻意的：真正对响应延迟敏感的是接进设备的版本，`c/` 目录下
  已经有对应的 C 查询层（`c/src/dictionary.c`，排序数组 + 二分查找，
  不是 trie，见该文件头部设计说明），差分测试跟这里的 Python 实现
  逐字节比对过，且已经接入 `chinese-ime/langhook` 并真机
  验证通过（白皮书 02 节 Step V）。
- 双拼只接了小鹤双拼（flypy）和自然码（natural）两套 schema，`data/`
  里还下了另外三套（abc/mspy/pyjj）但没写对应的解码测试，理论上
  `shuangpin.py` 的通用解析逻辑对它们也适用（同样是 `preedit_format`
  规则链格式），只是没有实测验证过，不确定就不下结论。
- 双拼对"最后一个键打到一半"的处理比较粗糙——原样透传，不像 9.2 节
  全拼那样有专门的前缀匹配（`is_prefix_of_some_syllable`），真要做
  精细化用户体验需要专门处理。
- 没有做语言模型级别的整句排序，`engine.py` 的整句候选是贪心最长匹配
  分词逐段拼字，不追求全局最优。
- `query_jianpin()` 是独立入口，没有跟 `query()` 的全拼路径合并——
  拼音输入法白皮书 07 节设计的"全拼/简拼两条路径并行、简拼候选降权
  合并展示"需要先有真机试验数据才能定下具体的降权系数，这一步留到
  真机试验阶段，现在只保证简拼查询本身是对的。
- `jianpin.c` 已经接入 `chinese-ime/langhook` 并真机验证
  通过——`segment.c`/`dictionary.c` 当初"先 Python 原型 + 差分测试，
  再移植 C，最后接真机"的三步全部走完。落地方式比 07 节最初设想的
  "两条路径加权合并"更简单：全拼路径先填充候选缓存，缓存槽位还有
  空间时简拼补位追加在后面（去重），天然满足"全拼优先"，真正的加权
  合并仍然留到以后（07 节已注明）。真机测试中发现并修复了两个真实
  bug：① 繁体模式下简拼补充候选一直是简体字（已经生成配套的
  `dict_jianpin.zh_tw.bin`，见下一条）；② "m"/"n" 两个字母查不到任何
  简拼候选（最初写的"缓冲区长度 &lt; 2 就跳过"对前缀匹配是错误的，
  单字母本来就可以是多字母简拼 key 的合法前缀；且这两个字母本身恰好
  也是合法的完整拼音音节，全拼路径会先入为主判定"已经打完"，两条路径
  同时失效）。详见白皮书 07 节新增 callout。
- 繁体双 blob 现在有完整的两份：`dict.zh_tw.bin`（全拼）+
  `dict_jianpin.zh_tw.bin`（简拼，`gen_jianpin_zh_tw_blob.py` 生成，
  复用 `build_traditional_table()` 转换 + `JianpinIndex` 鸭子类型接受
  普通 dict 作为数据源，没有改 `jianpin.py` 一行代码）——都已接入设备
  并真机验证过简体/繁体候选各自正确。也没有生成 `dict.zh_hk.bin`（香港
  繁体）——设备语言切换器目前只有
  简体/繁体两项，没有单独的香港繁体选项，生成一份用不上的数据没有
  意义，`build_traditional_table()` 的 `region` 参数本身已经支持
  `"hk"`，以后如果设备侧真的要加香港繁体，不需要改这一层代码。
- `mixed_input.py` 的 Shift 直通状态机**没有**移植进设备，是被取代的
  原型存档。设备端中英混输 **Phase C 已完成并真机验证**：词典驱动的
  iOS 式前缀补全，英文词典 `english.bin`（SymSpell 82k 词频表，复用
  `dict.bin` 同格式、运行时 mmap），打 `nihao`→选"你好"→打 `hello`→点
  英文候选得"你好hello"，全拼/简拼/双拼三模式统一；仅"连打 `nihaohello`
  一次成句"的联合切分后置。详见白皮书 07 节 Phase C callout。

## 数据来源与许可

- [`iDvel/rime-ice`](https://github.com/iDvel/rime-ice)（雾凇拼音，
  `data/base.dict.yaml` + `data/8105.dict.yaml` + `data/41448.dict.yaml`）
  —— **候选词典当前默认数据源**（M5 阶段换源，理由和细节见白皮书 9.5 节、
  `data/PROVENANCE.rime-ice.md`），钉版本 commit `569ff3bc`。**GNU GPL v3**
  （实测下载的 LICENSE 文件是标准 FSF GPLv3 全文）。`base.dict.yaml`
  只收词/短语不含单字，`8105.dict.yaml`（8105 常用字表）是必须一起
  加载的补充数据，不是可选扩展；`41448.dict.yaml`（大字表，46019 字，
  跟 8105 有 7888 字重叠）是磁盘空间确认充裕后额外加入的，加载时按
  `(音节, 词)` 去重，8105 的真实权重优先。`ext.dict.yaml`/
  `tencent.dict.yaml`/`others.dict.yaml` 各自有独立障碍（常量权重、
  缺拼音列需要额外实现自动注音、语义是错音提示表不是候选表），明确
  不用，见 `data/PROVENANCE.rime-ice.md`。上游没有独立 `AUTHORS` 文件，
  归属信息保留在数据文件自身的注释头里。
- [`rime/rime-luna-pinyin`](https://github.com/rime/rime-luna-pinyin)
  （`luna_pinyin.dict.yaml`）—— **旧数据源，仍保留可选加载**
  （`Dictionary(legacy_luna_pinyin=True)`，供对比/回退），**GNU LGPL v3**。
- [`rime/rime-essay-simp`](https://github.com/rime/rime-essay-simp)
  （`essay-zh-hans.txt`，preset vocabulary 词频表）—— 仅
  `legacy_luna_pinyin=True` 模式下配合上面的旧数据源使用，同样是
  **LGPL v3**。
- [`rime/rime-double-pinyin`](https://github.com/rime/rime-double-pinyin)
  （`double_pinyin_flypy.schema.yaml` / `double_pinyin_natural.schema.yaml`）
  —— **GNU GPL v3**（跟 luna_pinyin/essay 的 LGPL v3 不是同一个协议，
  实测下载的 LICENSE 文件确认过区别，不要混用同一份协议声明）。
- 这几份数据都完整保留了原始 `LICENSE`/`AUTHORS`（或等效归属信息）在
  `data/` 目录下，按各自协议要求署名和许可条款都在，如果以后要把
  `pinyin-engine/` 单独发布或者和其他协议的代码混合分发，GPL v3 比
  LGPL v3 对"整体作品许可"的要求更严格（GPL v3 的 copyleft 会扩散到
  链接它的整个程序，LGPL v3 只要求库本身可获取源码）——`rime-ice` 词典
  数据因此在 C 移植版里设计成独立文件运行时 mmap、不编译进
  `cangjie-langhook.so` 本体（见 `c/src/dictionary.h`），届时建议重新
  查一遍条款细则，这里只记录"用的是什么协议、架构上怎么取舍"这些
  事实，不代替法律意见。
