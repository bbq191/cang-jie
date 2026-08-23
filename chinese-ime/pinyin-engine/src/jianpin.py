"""
简拼索引：给词典里每个多音节词条计算"简拼 key"——各音节首字母顺序
拼接，比如"拼音引擎"（pin yin yin qing）-> "pyyq"。

对应白皮书《拼音输入法开发方案》07 节"简拼——声母首字母倒排索引"。
这是那份设计落地的第一块：离线 Python 实现 + 单元测试，不碰设备、
不改 xovi-extensions/cangjie-langhook 里的任何 hook 代码——C 移植和
真机接入是后续独立步骤，跟 03 节 segment.c、06 节 dictionary.c 当初
"先 Python 原型+差分测试，再移植 C"的节奏一致，这次先走第一步。

不需要一张手写的"23 个声母"映射表：zh/ch/sh 这三个双字母声母的简拼
字母，07 节设计里定义成"取首字母"（z/c/s）——而这恰好就是这些音节
本身的第一个字符（"zhong" 第一个字符正好是 "z"），跟单字母声母
（b/p/m/...）、零声母音节（an/ou/...）的处理方式在字符层面完全统一。
也就是说 jianpin_letter(syllable) 永远等于 syllable[0]，不需要额外
编码一张查表——这是拼音正字法本身的巧合，07 节提到的"23 个声母"只是
用来说明字母对应关系从何而来，算法不需要真的查这张表。
"""
from __future__ import annotations

from bisect import bisect_left, bisect_right
from dataclasses import dataclass

from .dictionary import Candidate, Dictionary


def jianpin_letter(syllable: str) -> str:
    """单个音节的简拼字母——就是它的第一个字符（见模块文档字符串）。"""
    if not syllable:
        raise ValueError("empty syllable")
    return syllable[0]


def jianpin_key(syllables: tuple[str, ...]) -> str:
    """整词的简拼 key：各音节简拼字母顺序拼接。"""
    return "".join(jianpin_letter(s) for s in syllables)


@dataclass(frozen=True)
class JianpinEntry:
    key: str
    candidate: Candidate
    syllables: tuple[str, ...]


class JianpinIndex:
    """从 Dictionary 反推出的简拼倒排索引：key -> 候选（按 key 分组，
    组内按权重降序）。不重复存储候选文字/权重——直接持有 Dictionary 里
    已有的 Candidate 对象引用，对应白皮书 07 节"候选文字/权重复用主
    候选池，只加一份 key 表"的设计（Python 原型阶段没有 mmap/偏移量
    这些概念，直接引用对象即可满足"不重复存文字"这一点；C 移植时才
    需要真正设计偏移量格式，那是下一步）。

    只收多音节（>= 2 个音节）词条——单音节词的简拼字母跟它全拼的第一
    个字母是同一个东西，没有"缩写"效果：查询"p"这个简拼字母，效果
    等价于查 Dictionary.lookup_prefix(("p",))（03 节 segment.c 的
    pending 前缀匹配已经天然覆盖这个场景），重复收录进简拼索引只会
    让索引膨胀却没有额外收益，07 节设计文档没有明确写这条实现细节，
    这里补上。
    """

    def __init__(self, dictionary: Dictionary):
        entries: list[JianpinEntry] = []
        for syllables, candidates in dictionary.items():
            if len(syllables) < 2:
                continue
            key = jianpin_key(syllables)
            for cand in candidates:
                entries.append(JianpinEntry(key=key, candidate=cand, syllables=syllables))
        # 按 key 排序（用于二分查找区间），同一个 key 内部按权重降序
        # （bisect 只依赖 key 这一列有序，稳定排序保证同 key 内部的
        # 权重顺序不被 key 排序打乱）。
        entries.sort(key=lambda e: e.key)
        self._entries: list[JianpinEntry] = entries
        self._keys: list[str] = [e.key for e in entries]
        for start, end in self._iter_key_groups():
            entries[start:end] = sorted(
                entries[start:end], key=lambda e: e.candidate.weight, reverse=True
            )

    def _iter_key_groups(self):
        """产出 [start, end) 区间，每个区间对应连续相同 key 的一段。"""
        i = 0
        n = len(self._keys)
        while i < n:
            j = bisect_right(self._keys, self._keys[i], lo=i)
            yield i, j
            i = j

    def __len__(self) -> int:
        return len(self._entries)

    def lookup(self, key: str, limit: int = 50) -> list[Candidate]:
        """精确匹配简拼 key（用户已经把每个音节都打完的简拼输入）。"""
        lo = bisect_left(self._keys, key)
        hi = bisect_right(self._keys, key)
        return [e.candidate for e in self._entries[lo:hi]][:limit]

    def lookup_prefix(self, prefix: str, limit: int = 50) -> list[Candidate]:
        """前缀匹配（用户还没打完简拼，比如已经打了 "py"，可能是
        "拼音"(py) 本身，也可能是 "拼音引擎"(pyyq) 等更长词的前缀）。
        排序区间内混合了不同长度的 key，重新按权重整体排序（不能像
        lookup() 那样直接沿用组内预排序——预排序只保证单个 key 内部
        有序，跨多个 key 的区间需要重新排序才能得到全局权重降序）。
        """
        if not prefix:
            return []
        lo = bisect_left(self._keys, prefix)
        # 用 "prefix + 一个比任何简拼字母都大的哨兵字符" 定位右边界，
        # 跟 06 节 C 版 dictionary.c 前缀匹配用的思路一致，只是这里是
        # Python bisect，不需要真的选一个字符——直接对下一个可能前缀
        # 做区间扫描更简单：从 lo 开始线性扫，直到 key 不再以 prefix
        # 开头为止（简拼 key 本身很短，通常几个字符，扫描代价很小，
        # 不需要为这点长度专门再做一次二分）。
        out: list[Candidate] = []
        i = lo
        n = len(self._keys)
        while i < n and self._keys[i].startswith(prefix):
            out.append(self._entries[i].candidate)
            i += 1
        out.sort(key=lambda c: c.weight, reverse=True)
        return out[:limit]
