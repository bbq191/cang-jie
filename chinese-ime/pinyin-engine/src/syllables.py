"""
合法拼音音节表 —— 数据来源与设计说明见 README。

没有手工默写一份"声母表 x 韵母表"的组合规则表：现代汉语普通话声韵母
拼合规则例外很多（比如 b/p/m/f 只挑一部分韵母、j/q/x 只配 i/ü 系韵母），
手写这份规则表出错的代价很高（切分错就意味着候选生成全错），而
RIME 的 luna_pinyin 词库本身就是一份经过社区多年校对、真实覆盖全部
普通话合法音节的权威数据源（见 data/luna_pinyin.dict.yaml）——直接从
里面提取"实际出现过的音节全集"，比自己归纳规则更可靠，也让"音节表"
和"候选词典"用的是同一份真相来源，不会出现两边不一致的情况。

补的 EXTRA_SYLLABLES 是几个语气词/拟声词音节，标准声韵母组合规则覆盖
不到但确实是合法普通话音节，词库里如果已经包含就不会重复。
"""
from __future__ import annotations

from pathlib import Path

DATA_DIR = Path(__file__).resolve().parent.parent / "data"
DICT_PATH = DATA_DIR / "luna_pinyin.dict.yaml"

# 词库里覆盖不全但确实合法的语气词音节（口语高频，用户会打）
EXTRA_SYLLABLES = {"n", "ng", "hm", "hng", "m"}


def _iter_dict_lines(path: Path):
    """跳过 YAML frontmatter，只读 '...' 分隔符之后的 TSV 数据行。"""
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
            yield line


def load_syllables(path: Path = DICT_PATH) -> frozenset[str]:
    """从词库里提取全部出现过的合法音节（不含声调）。"""
    syllables: set[str] = set(EXTRA_SYLLABLES)
    for line in _iter_dict_lines(path):
        cols = line.split("\t")
        if len(cols) < 2:
            continue
        pinyin_field = cols[1]
        for syl in pinyin_field.split(" "):
            syl = syl.strip()
            if syl and syl.isascii() and syl.isalpha():
                syllables.add(syl)
    return frozenset(syllables)


SYLLABLES: frozenset[str] = load_syllables()

# 按长度降序排列一份列表，供最长匹配 / 前缀查找使用
_SYLLABLES_BY_LEN = sorted(SYLLABLES, key=len, reverse=True)
MAX_SYLLABLE_LEN = len(_SYLLABLES_BY_LEN[0]) if _SYLLABLES_BY_LEN else 0


def is_valid_syllable(s: str) -> bool:
    return s in SYLLABLES


def is_prefix_of_some_syllable(s: str) -> bool:
    """s 是否可能是某个合法音节打到一半的前缀（用于处理"最后一个音节还没打完"）。"""
    if not s:
        return False
    return any(syl.startswith(s) for syl in SYLLABLES)
