"""
拼音引擎主入口，把 segment.py（切分）和 dictionary.py（候选词典）接起来。

对应白皮书 9.2 节两步：
  1. 音节切分（segment.py 已完成）。
  2. 候选生成：优先整段精确匹配词典（luna_pinyin 本身收了不少多字词，
     常见短语一次就能命中）；命中不了就退化成"贪心最长匹配分词 + 逐段
     拼字"（类似输入法的整句转换，只是不做 9.2 节里说的 bigram 排序，
     MVP 阶段单靠词频表打底）；如果连最短的单字候选都没有，说明这个
     音节组合词典里确实没有对应词，原样返回空，不瞎编。

9.3 节的双拼、9.4 节的繁体转换都是在这层外面再包一层，本文件不涉及。

《拼音输入法开发方案》白皮书 07 节"简拼"设计的第一步落地在这里接入：
query_jianpin() 是独立入口，跟下面的 query()（全拼路径）互不干扰——
07 节设计提到的"全拼/简拼两条路径并行查询、加权合并展示"需要先有
真机试验数据才能定下简拼候选的降权系数，这次先只接一个能独立调用、
验证正确性的入口，合并逻辑是后续步骤，不在这次实现范围内。
"""
from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path

from .dictionary import Dictionary, Candidate, DEFAULT_DICT_PATHS
from .jianpin import JianpinIndex
from .segment import Segmentation, best_segmentation, segment
from .shuangpin import keys_to_pinyin
from .traditional import to_traditional


@dataclass
class EngineResult:
    input_raw: str
    segmentation: Segmentation
    candidates: list[str]           # 排好序的候选词/候选句，[0] 是默认上屏项
    matched_syllable_count: int     # 候选覆盖了前几个完整音节（不含 pending）


class PinyinEngine:
    def __init__(self, dict_path: Path | tuple[Path, ...] = DEFAULT_DICT_PATHS):
        # 注意：dict_path 默认是 DEFAULT_DICT_PATHS（8105.dict.yaml 单字表 +
        # base.dict.yaml 词/短语表，M5 换源），不是 syllables.py 里给切分算法
        # 用的 luna_pinyin.dict.yaml——两者是不同数据源，音节表仍然固定读
        # luna_pinyin（切分逻辑没变），候选词典默认读雾凇拼音这两个文件。
        self.dictionary = Dictionary(dict_path)
        # 07 节简拼索引：从同一份 dictionary 反推，不是另一份数据源
        # （构建耗时约 0.8s，量级上跟 Dictionary() 本身的加载耗时相近，
        # 在 __init__ 里一次性建好，不是每次查询都重建）。
        self.jianpin_index = JianpinIndex(self.dictionary)

    # ---- 内部：贪心最长匹配分词 + 逐段查字，实现"整句候选" ----
    def _compose(self, syllables: tuple[str, ...]) -> list[str]:
        if not syllables:
            return []
        n = len(syllables)
        # dp[i] = 从 syllables[i:] 开始，能拼出的最佳字符串（None 表示拼不出来）
        dp: list[str | None] = [None] * (n + 1)
        dp[n] = ""
        for i in range(n - 1, -1, -1):
            best: str | None = None
            # 优先尝试更长的整词（最长匹配），找到第一个能让后半段也
            # 拼通的切法就停（贪心，不做全局最优 DP——9.2 节明确说
            # MVP 不追求语言模型级别的排序）。
            for j in range(n, i, -1):
                cands = self.dictionary.lookup(syllables[i:j])
                if not cands:
                    continue
                rest = dp[j]
                if rest is None:
                    continue
                best = cands[0].word + rest
                break
            dp[i] = best
        return [dp[0]] if dp[0] else []

    def candidates_for_complete(self, syllables: tuple[str, ...], limit: int = 9) -> list[str]:
        """完整（不含 pending 前缀）音节序列 -> 候选词/候选句列表。"""
        if not syllables:
            return []
        out: list[str] = []
        seen: set[str] = set()

        # 1) 整段精确匹配（常见短语词典里直接有）
        for c in self.dictionary.lookup(syllables):
            if c.word not in seen:
                seen.add(c.word)
                out.append(c.word)

        # 2) 贪心分词 + 逐段拼字（整句候选兜底）
        for composed in self._compose(syllables):
            if composed and composed not in seen:
                seen.add(composed)
                out.append(composed)

        # 3) 只看第一个音节的单字候选，方便用户只想打一个字就选重
        first_syl_cands = self.dictionary.lookup(syllables[:1])
        for c in first_syl_cands:
            if c.word not in seen:
                seen.add(c.word)
                out.append(c.word)

        return out[:limit]

    def query(self, raw: str, limit: int = 9) -> EngineResult:
        seg = best_segmentation(raw)
        cands = self.candidates_for_complete(seg.syllables, limit=limit)

        if seg.pending:
            # 最后一个音节还没打完：用前缀匹配追加一批"如果打完是这个"的候选，
            # 排在完整匹配候选之后，避免抢占已经确定的结果。
            prefix_cands = self.dictionary.lookup_prefix(
                seg.syllables + (seg.pending,), limit=limit
            )
            seen = set(cands)
            for c in prefix_cands:
                if c.word not in seen:
                    seen.add(c.word)
                    cands.append(c.word)
            cands = cands[:limit]

        return EngineResult(
            input_raw=raw,
            segmentation=seg,
            candidates=cands,
            matched_syllable_count=len(seg.syllables),
        )

    def commit_preview(self, raw: str) -> str:
        """给候选栏默认展示用：直接返回排第一的候选，没有候选就原样透传拼音。"""
        result = self.query(raw, limit=1)
        return result.candidates[0] if result.candidates else raw

    def query_shuangpin(self, keys: str, scheme: str = "flypy", limit: int = 9) -> EngineResult:
        """9.3 节说的"双拼只是加一层键位转换"：这里就是那层——把双拼按键
        转成标准拼音字符串，再原样交给上面的 query()，不重新实现一遍逻辑。"""
        pinyin = keys_to_pinyin(keys, scheme=scheme)
        return self.query(pinyin, limit=limit)

    def query_jianpin(self, keys: str, limit: int = 9) -> list[str]:
        """07 节简拼查询：keys 是用户已经敲的简拼字母（每个字母对应一个
        音节的简拼字母，没有分隔符，比如 "zg" 对应"中国"）。

        直接用 JianpinIndex.lookup_prefix()——它本身就是"key 以 keys
        开头"的全体候选按权重降序，天然包含 key 长度正好等于 keys 的
        精确匹配（"py".startswith("py") 为真），不需要在这层再拆成
        "先精确、不够再前缀"两段查询。最初这么拆过一版，结果被自己的
        单元测试打脸：精确匹配（2 音节词，数量多、权重普遍不低）经常
        自己就把 limit 配额占满，永远轮不到前缀匹配补位，反而让"拼音
        输入法"这种更长、但明显是用户想要的词被挤没——教训跟白皮书
        06 节 dictionary.c 那次"攒够 512 个就不再看后面的 key"是同一
        类型的错误（只是这次在设计/测试阶段就被抓到，没有等到接了
        真实数据规模才暴露）。统一交给权重排序，就没有这个问题。
        """
        keys = keys.strip().lower()
        if not keys:
            return []
        seen: set[str] = set()
        out: list[str] = []
        for cand in self.jianpin_index.lookup_prefix(keys, limit=limit * 8):
            if cand.word not in seen:
                seen.add(cand.word)
                out.append(cand.word)
            if len(out) >= limit:
                break
        return out

    def commit(self, word: str, region: str | None = None) -> str:
        """9.4 节说的"候选栏仍显示简体，选中那一刻才转换"：这里就是那个转换点。
        region=None（默认，简体模式）原样返回；region='tw'/'hk' 才转换成繁体，
        候选栏列表本身（query() 的返回值）不受影响，只有真正插入文本框前经过这里。
        """
        if region is None:
            return word
        return to_traditional(word, region=region)
