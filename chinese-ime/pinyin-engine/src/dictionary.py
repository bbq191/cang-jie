"""
候选词典：把 RIME 词库文件加载成
    "拼音音节序列" -> [(词, 权重), ...]（按权重降序）
的查找表。9.2 节说的"起步用简单的词频表排序"就是这里。

默认数据源是雾凇拼音（iDvel/rime-ice）的 cn_dicts/base.dict.yaml（M5 阶段换的源，
见白皮书 9.2/9.5 节、data/PROVENANCE.rime-ice.md）：三列，词/拼音/整数词频，
tab 分隔，词频已经是跨字符可比的权重（rime-ice 自己在 frontmatter 声明
`sort: by_weight` 且合并时已经按"拼音顺序、权重逆序"排过），直接拿来排序，
不需要额外语料频次表兜底，也不用担心"同字读音占比不能跨字比较"的坑。

旧数据源（rime/rime-luna-pinyin + rime-essay-simp）留作对比/回退，用
`Dictionary(legacy_luna_pinyin=True)` 加载，两个此前踩过的坑仍然记在这里、
只在旧模式生效：

1. luna_pinyin.dict.yaml 这份"词库"本身只收了单字全集 + 一小部分手工
   维护的固定词组（成语、专名），像"你好"这种最基础的常用词反而不在
   里面——frontmatter 里的 `use_preset_vocabulary: true` 是关键：Rime
   真机运行时会另外加载一份"preset vocabulary"（词频表，只有"词 + 出现
   次数"，不带拼音），跟这份词库的单字拼音实时组合、拼出词组候选。
   对应做法：拉取 rime-essay-simp 仓库的 essay-zh-hans.txt（就是这份
   preset vocabulary），把每个词按"逐字取该字最高权重读音"拼出拼音 key，
   词典没有现成条目时用这份表补位。

2. 更容易踩的坑：词典里那一列百分比权重（比如 "的 de 99.97% / di 0.03%"）
   语义是"同一个字的多个读音里，这个读音占多大比例"——只在同一个字的
   内部有意义，不能跨字符比较！实测验证过一个反例：单独查 "guo" 这个
   音节，"掴"（97.33%，但那是"掴"自己 guai/guo 两个读音里 guo 占比，
   不代表"掴"这个字常见）排在了真正常用的"国"（没有标注百分比，因为
   "国"只有一个读音，没有歧义要标）前面——如果直接拿这一列当候选排序
   依据，会把生僻字排到常用字前面，是真实会导致候选体验完全不可用的
   一个数据语义误用。修正方法：候选跨字符/跨词的排序统一改用
   essay-zh-hans.txt 里的真实语料出现频次（这份表本身就同时覆盖单字
   和词组的频次，之前只用来补词组是没用满数据），词典里的百分比权重
   只保留在"同一个字选哪个读音"这一层内部使用，不参与跨字排序。

rime-ice 的 base.dict.yaml 天然不带这两个坑（实测扫过全部 55 万+行数据，
第三列没有出现过百分比格式，全是纯整数词频），换源本身顺带把这两处
特殊处理都省掉了，不是"换了个文件名、逻辑照旧"。

**一个动手之后才发现的数据缺口**：`base.dict.yaml` 只收词/短语，完全不含
单字（实测确认 "国"/"的" 这类最基础的单字一条都查不到）——雾凇拼音把
单字表拆到了独立的 `8105.dict.yaml`（8105 常用字表，115KB，格式跟
`base.dict.yaml` 完全一样：词/拼音/整数词频三列）。默认模式因此会把这
两个文件都加载并合并（单字和词/短语的音节序列长度天然不同，1 音节的
key 只会被单字表填充、多音节的 key 只会被词表填充，两份数据不会在同
一个 key 下混权重，量纲不同也不影响排序正确性）。见
`data/PROVENANCE.rime-ice.md`。

**扩展词库调研**（磁盘空间确认充裕之后专门核实过，见白皮书 9.5 节）：
默认加了 `41448.dict.yaml`（大字表，46019 个字，跟 8105 有 7888 个字重叠，
格式是"字+拼音"两列、**没有权重列**）——加载时按 `(syllables, word)` 去重，
8105 已有的字权重不受影响（排在前面的文件优先），41448 独有的生僻字权重
按 0 处理，自然排到候选末尾，不会挤占常用候选的位置。`ext.dict.yaml`
（权重全是常量 100，内容偏专有名词）、`tencent.dict.yaml`（没有拼音列，
要靠 Rime 自己的"单字读音自动拼词组拼音+多音字消歧"算法，我们没实现）、
`others.dict.yaml`（不是候选表，是配合 corrector 用的错音提示表，故意
收录错误读音）这三个明确不用，原因跟磁盘空间无关，见
`data/PROVENANCE.rime-ice.md`。
"""
from __future__ import annotations

from collections import defaultdict
from dataclasses import dataclass
from pathlib import Path

from .syllables import DICT_PATH as LUNA_DICT_PATH, DATA_DIR

BASE_DICT_PATH = DATA_DIR / "base.dict.yaml"  # 雾凇拼音 base.dict.yaml：词/短语，默认数据源
CHAR_DICT_PATH = DATA_DIR / "8105.dict.yaml"  # 雾凇拼音 8105 常用字表：单字，base.dict.yaml 不含单字
CHAR_EXT_DICT_PATH = DATA_DIR / "41448.dict.yaml"  # 雾凇拼音大字表：单字，跟 8105 有重叠，排最后、去重后兜底覆盖生僻字
IOREST_DICT_PATH = DATA_DIR / "iorest.dict.yaml"   # Iorest/rime-dict 合并注音词库（pypinyin 生成，见 tools/annotate_iorest.py）
# 顺序有意义：8105 排在 41448 前面，跨文件去重时 8105 的真实权重优先生效
# （见 _load_dict 的 (syllables, word) 去重逻辑），不能颠倒。
DEFAULT_DICT_PATHS: tuple[Path, ...] = (CHAR_DICT_PATH, BASE_DICT_PATH, CHAR_EXT_DICT_PATH)
# FULL_DICT_PATHS = 默认 + iorest（~180 万注音词），只给简体全拼 dict.bin 用
# （gen_dict_blob + 其差分测试显式传入）。iorest 排最后、统一低权重（见
# annotate_iorest.py），只做覆盖兜底、不挤占前面数据源的真实词频排序。
# **刻意不进 DEFAULT**：简拼索引（gen_jianpin_blob 用 Dictionary() 默认）
# 保持精简、不把 180 万长尾编进简拼快速输入索引（blob 更小、候选不变噪）；
# 繁体全拼 dict.zh_tw 本轮也不含 iorest（需 OpenCC 转换，留作后续）。
FULL_DICT_PATHS: tuple[Path, ...] = (*DEFAULT_DICT_PATHS, IOREST_DICT_PATH)
ESSAY_PATH = DATA_DIR / "essay-zh-hans.txt"   # 仅 legacy_luna_pinyin=True 时使用


@dataclass(frozen=True)
class Candidate:
    word: str
    weight: float  # 排序用的最终分值，语义随数据源不同（见类文档字符串）


def _parse_weight_column(raw: str | None) -> float | None:
    """解析词典第三列。base.dict.yaml：直接是跨字符可比的整数词频。
    旧的 luna_pinyin.dict.yaml（legacy 模式）：同字内部的读音占比，百分比或裸数字，
    返回 None 表示没标注（该字/词只有一种读音，不需要标）。"""
    if not raw:
        return None
    raw = raw.strip()
    if raw.endswith("%"):
        try:
            return float(raw[:-1]) / 100.0
        except ValueError:
            return None
    try:
        return float(raw)
    except ValueError:
        return None


class Dictionary:
    def __init__(
        self,
        path: Path | tuple[Path, ...] | None = None,
        essay_path: Path | None = None,
        *,
        legacy_luna_pinyin: bool = False,
        normalize_simplified: bool = False,
    ):
        """默认（legacy_luna_pinyin=False）加载 DEFAULT_DICT_PATHS
        （8105.dict.yaml 单字表 + base.dict.yaml 词/短语表），第三列整数词频
        直接当权重用，essay_path 参数被忽略（新格式不需要语料频次表兜底）。
        `path` 传单个 Path 也可以（比如只想单独测某一个文件），会当成
        只有一个元素的元组处理。

        legacy_luna_pinyin=True 时切回旧数据源（path 默认 luna_pinyin.dict.yaml，
        essay_path 默认 essay-zh-hans.txt），走原来的"essay 主排序 + reading_share
        tie-break + essay 词组补位"逻辑，供对比/回退用，不是默认路径。
        """
        self._legacy = legacy_luna_pinyin
        if legacy_luna_pinyin:
            path = path if path is not None else LUNA_DICT_PATH
            essay_path = essay_path if essay_path is not None else ESSAY_PATH
        else:
            path = path if path is not None else DEFAULT_DICT_PATHS
            essay_path = None  # 新格式的第三列本身已经跨字符可比，不需要
        paths: tuple[Path, ...] = (path,) if isinstance(path, Path) else tuple(path)

        self._freq: dict[str, int] = {}
        if essay_path is not None and essay_path.exists():
            self._freq = self._load_freq(essay_path)

        self._table: dict[tuple[str, ...], list[Candidate]] = defaultdict(list)
        self._char_pinyin: dict[str, str] = {}  # 单字 -> 主读音，仅 legacy 模式的词组补位用
        self._load_dict(paths)
        if essay_path is not None and essay_path.exists():
            self._augment_with_essay_phrases(essay_path)
        if normalize_simplified:
            self._normalize_to_simplified()

    # ---------- 繁体污染归一化（简体侧 blob 专用，构造时 opt-in） ----------
    def _normalize_to_simplified(self) -> None:
        """把候选词里的繁体字形转成简体——真机报的"简体模式出繁体候选"根因：
        iorest 专题词库大量繁体（劍網三/絕對領域），base 少量鱼类专名异体
        （鰕），41448 大字表也带繁体字。

        安全守卫（关键）：OpenCC t2s 对已简体的输入会误转（坏→坯、夥→伙，把
        "暗中破坏"改成"暗中破坯"），因为它假设输入是纯繁体。所以这里**字符级**
        转换，且只转"不在 8105 通用规范汉字表里的字"——8105 里的字（坏/夥/車…）
        一律保留原样，只对真正的繁体独有字（嬌/劍/網/鰕…）调 t2s。这样绝不
        改坏合法简体词。8105 自带的极少数繁体条目（車 等，约 27 个）落在白名单
        里、保留不动，是可接受的残留。

        繁体 dict.zh_tw.bin 走各自的 s2twp 生成、不经过这里（对已归一化的简体
        再 s2twp 仍得正确繁体，不受影响）。host 单测默认不开这个 flag、不受影响。
        """
        import opencc

        conv = opencc.OpenCC("t2s")
        simp_std: set[str] = set()
        started = False
        with CHAR_DICT_PATH.open("r", encoding="utf-8") as f:
            for line in f:
                line = line.rstrip("\n")
                if not started:
                    if line == "...":
                        started = True
                    continue
                if not line or line.startswith("#"):
                    continue
                w = line.split("\t")[0]
                if len(w) == 1:
                    simp_std.add(w)

        char_cache: dict[str, str] = {}

        def to_simp(word: str) -> str:
            out_chars = []
            for ch in word:
                if ch in simp_std:
                    out_chars.append(ch)
                    continue
                mapped = char_cache.get(ch)
                if mapped is None:
                    mapped = conv.convert(ch)
                    char_cache[ch] = mapped
                out_chars.append(mapped)
            return "".join(out_chars)

        for syllables, cands in self._table.items():
            merged: dict[str, float] = {}
            order: list[str] = []
            for c in cands:
                w = to_simp(c.word)
                if w in merged:
                    if c.weight > merged[w]:
                        merged[w] = c.weight
                else:
                    merged[w] = c.weight
                    order.append(w)
            self._table[syllables] = [Candidate(word=w, weight=merged[w]) for w in order]

    # ---------- essay-zh-hans.txt：真实语料频次表（仅 legacy 模式加载） ----------
    @staticmethod
    def _load_freq(path: Path) -> dict[str, int]:
        freq: dict[str, int] = {}
        with path.open("r", encoding="utf-8") as f:
            for line in f:
                line = line.rstrip("\n")
                if not line:
                    continue
                cols = line.split("\t")
                if len(cols) != 2:
                    continue
                word, count_raw = cols
                try:
                    freq[word] = int(count_raw)
                except ValueError:
                    continue
        return freq

    # ---------- 词库主文件：权威的字/词 -> 拼音映射（可以是多个文件合并） ----------
    def _load_dict(self, paths: tuple[Path, ...]) -> None:
        raw_rows: list[tuple[str, tuple[str, ...], float | None]] = []
        # 跨文件去重：同一个 (音节序列, 词) 组合只保留第一次出现的那条——
        # 41448 大字表跟 8105 常用字表有 7888 个字重叠，DEFAULT_DICT_PATHS
        # 里 8105 排在前面，先出现先占位，41448 里的重复字会被跳过，不会
        # 出现同一个字在同一个候选列表里出现两次（一次有真实权重、一次权重
        # 兜底为 0）。同一个字的不同读音（不同音节序列）不受影响，仍然全部
        # 保留——这不是去重错读音，是去重"完全相同的候选条目"。
        seen: set[tuple[tuple[str, ...], str]] = set()
        for path in paths:
            started = False
            with path.open("r", encoding="utf-8") as f:
                for line in f:
                    line = line.rstrip("\n")
                    if not started:
                        if line.strip() == "...":
                            started = True
                        continue
                    if not line or line.startswith("#"):
                        continue
                    cols = line.split("\t")
                    if len(cols) < 2:
                        continue
                    word = cols[0]
                    syllables = tuple(cols[1].split(" "))
                    dedup_key = (syllables, word)
                    if dedup_key in seen:
                        continue
                    seen.add(dedup_key)
                    weight_col = _parse_weight_column(cols[2] if len(cols) > 2 else None)
                    raw_rows.append((word, syllables, weight_col))

        if self._legacy:
            # 主排序键：essay 语料频次（跨字符可比）。
            # tie-break：语料查不到频次时（essay 没收录的生僻字），退而求其次用
            # "reading_share"（只在极少数同词同音节多次出现、频次又相同时才会用到，
            # 不会影响绝大多数排序结果，纯粹是让排序稳定、不依赖字典遍历顺序）。
            for word, syllables, weight_col in raw_rows:
                freq = self._freq.get(word, 0)
                tie_break = weight_col if weight_col is not None else 0.0
                weight = freq + tie_break  # freq 是整数频次，tie_break < 1，不会互相越界影响排序
                self._table[syllables].append(Candidate(word=word, weight=weight))
        else:
            # base.dict.yaml：第三列本身就是跨字符可比的整数词频，直接用；
            # 没有第三列（理论上不会出现，源数据已核实每行都带词频）时兜底为 0。
            for word, syllables, weight_col in raw_rows:
                weight = weight_col if weight_col is not None else 0.0
                self._table[syllables].append(Candidate(word=word, weight=weight))

        for cands in self._table.values():
            cands.sort(key=lambda c: c.weight, reverse=True)

        if self._legacy:
            # 单字主读音表：字符内部按"reading_share"选主读音（这一列的本意就是干这个用的），
            # 仅供下面 _augment_with_essay_phrases 用，非 legacy 模式不需要这张表。
            char_best_share: dict[str, tuple[float, str]] = {}
            for word, syllables, weight_col in raw_rows:
                if len(syllables) != 1:
                    continue
                share = weight_col if weight_col is not None else 1.0
                prev = char_best_share.get(word)
                if prev is None or share > prev[0]:
                    char_best_share[word] = (share, syllables[0])
            self._char_pinyin = {w: syl for w, (share, syl) in char_best_share.items()}

    # ---------- 用 essay 词组频次表补充词典里没有的常用词组（仅 legacy 模式） ----------
    def _augment_with_essay_phrases(self, path: Path) -> None:
        essay_rows: list[tuple[tuple[str, ...], str, int]] = []
        with path.open("r", encoding="utf-8") as f:
            for line in f:
                line = line.rstrip("\n")
                if not line:
                    continue
                cols = line.split("\t")
                if len(cols) != 2:
                    continue
                word, count_raw = cols
                if len(word) < 2:
                    continue  # 单字已经由词典本身覆盖
                try:
                    count = int(count_raw)
                except ValueError:
                    continue
                syllables = []
                ok = True
                for ch in word:
                    syl = self._char_pinyin.get(ch)
                    if syl is None:
                        ok = False
                        break
                    syllables.append(syl)
                if not ok:
                    continue
                essay_rows.append((tuple(syllables), word, count))

        touched: set[tuple[str, ...]] = set()
        for syllables, word, count in essay_rows:
            if any(c.word == word for c in self._table.get(syllables, ())):
                continue  # 词典已经手工收了这个词，不重复添加
            self._table[syllables].append(Candidate(word=word, weight=float(count)))
            touched.add(syllables)

        for syllables in touched:
            self._table[syllables].sort(key=lambda c: c.weight, reverse=True)

    def lookup(self, syllables: tuple[str, ...]) -> list[Candidate]:
        return self._table.get(syllables, [])

    def lookup_prefix(self, syllables_prefix: tuple[str, ...], limit: int = 50) -> list[Candidate]:
        """音节序列前缀匹配——用于"最后一个音节还没打完"时按前缀过滤候选。"""
        out: list[Candidate] = []
        n = len(syllables_prefix)
        last = syllables_prefix[-1] if syllables_prefix else ""
        head = syllables_prefix[:-1]
        for key, cands in self._table.items():
            if len(key) != n:
                continue
            if key[:-1] != head:
                continue
            if not key[-1].startswith(last):
                continue
            out.extend(cands)
        # 权重降序；同权重按候选词 UTF-8 字节序（对齐 C 版 cj_dict_cand_ranks_before）。
        # iorest 灌入后前缀池常超过 limit，截断落在同权重并列区，两边 tie-break
        # 必须一致，否则截断到的成员集合不同（差分测试 240 万规模下会暴露）。
        out.sort(key=lambda c: (-c.weight, c.word.encode("utf-8")))
        return out[:limit]

    def __len__(self) -> int:
        return sum(len(v) for v in self._table.values())

    def items(self):
        """遍历全部 (音节序列, 候选列表) —— 供 07 节简拼索引（jianpin.py）
        反推整份词典用，不直接暴露 self._table，保留改内部存储结构的自由。"""
        return self._table.items()
