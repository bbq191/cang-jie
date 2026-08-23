"""
音节切分：把用户连续敲的拉丁字母（可能带隔音符号 '，可能最后一个音节
还没打完）切成一串合法拼音音节。

设计要点（对应白皮书 9.2 节"最容易出 bug 的一步"）：
  1. 隔音符号 ' 是硬边界，用户主动消歧义（"xi'an" vs "xian"）时必须尊重。
  2. 没有隔音符号时，两种合法切分都存在的情况按"音节数更少"（等价于
     贪心最长匹配）优先——这也是《汉语拼音正词法基本规则》里"能连写
     成一个音节就不用隔音符号"的实际约定，例子：xianggang 应该切成
     xiang+gang 而不是 xi+ang+gang。
  3. 用户经常打到一半（比如刚敲完 "zho"，还不知道是 zhong/zhou/zhang
     里哪个），最后一段如果不是完整音节但是某个音节的前缀，要单独
     标记成"未完成前缀"交给候选生成去做前缀匹配，而不是切分失败。
"""
from __future__ import annotations

from dataclasses import dataclass, field
from functools import lru_cache

from .syllables import is_valid_syllable, is_prefix_of_some_syllable, MAX_SYLLABLE_LEN


@dataclass
class Segmentation:
    syllables: tuple[str, ...]          # 已经切出来的完整音节
    pending: str = ""                    # 打到一半、还不完整的最后一段（可能为空）

    @property
    def is_complete(self) -> bool:
        return self.pending == ""

    def __str__(self) -> str:  # 方便调试打印
        parts = list(self.syllables)
        if self.pending:
            parts.append(f"[{self.pending}]")
        return "'".join(parts)


def _split_on_apostrophe(raw: str) -> list[str]:
    return [seg for seg in raw.split("'") if seg != ""]


@lru_cache(maxsize=4096)
def _all_full_segmentations(s: str) -> tuple[tuple[str, ...], ...]:
    """s 必须整体切成合法音节（不允许剩余），返回所有可能的切法。
    用于处理一段不含隔音符号、内部本身就有歧义的连续字母。"""
    if s == "":
        return ((),)
    results: list[tuple[str, ...]] = []
    max_len = min(len(s), MAX_SYLLABLE_LEN)
    for cut in range(max_len, 0, -1):
        head, rest = s[:cut], s[cut:]
        if not is_valid_syllable(head):
            continue
        for tail in _all_full_segmentations(rest):
            results.append((head,) + tail)
    return tuple(results)


def _rank_key(segmentation: tuple[str, ...]) -> tuple:
    # 音节数越少（=贪心最长匹配）排名越靠前
    return (len(segmentation),)


def segment(raw: str) -> list[Segmentation]:
    """对外主入口。raw 只允许小写字母和 '。

    返回按"最合理"排序的候选切分列表（第一个是推荐结果），一般只有一个，
    确实有歧义时会有多个，方案 9.2 里没细化到全量搜索，这里做成开放
    接口，全文拼音搜索（9.7 节镇纸演示里的"ce s"那种功能）需要多候选时
    直接复用。
    """
    raw = raw.strip().lower()
    if raw == "":
        return [Segmentation(syllables=())]

    chunks = _split_on_apostrophe(raw)
    # 每个隔音符号分隔出来的 chunk 除了最后一个，都必须是"完整"切分
    # （用户已经确认了边界，不存在"打到一半"的情况）；只有整个输入串
    # 的最后一段才可能是 pending。
    *closed_chunks, last_chunk = chunks if chunks else [""]

    # 先把前面确定完整的 chunk 都切好（每个 chunk 内部仍可能有歧义）
    prefix_options: list[tuple[str, ...]] = [()]
    for chunk in closed_chunks:
        chunk_segs = _all_full_segmentations(chunk)
        if not chunk_segs:
            # 隔音符号前面这一段本身就不是合法音节组合——没法切，
            # 原样当一个不合法的"整体"塞进去，交给上层判断。
            prefix_options = [p + (chunk,) for p in prefix_options]
            continue
        best = min(chunk_segs, key=_rank_key)
        prefix_options = [p + best for p in prefix_options]
    fixed_prefix = prefix_options[0] if prefix_options else ()

    # 最后一段：优先看能不能整体切成完整音节（可能有歧义，多个结果）。
    # 只有当"整体切不出来"时才需要 pending——这是关键：如果整段本来就是
    # 合法的完整音节组合（比如 "an"），就不要因为它同时也是 "ang" 的前缀
    # 就硬把它标记成"没打完"，那样反而会把明明确定的结果标成不确定。
    full_segs = _all_full_segmentations(last_chunk)
    results: list[Segmentation] = []
    seen: set[tuple[tuple[str, ...], str]] = set()

    def add(segs: tuple[str, ...], pending: str) -> None:
        key = (fixed_prefix + segs, pending)
        if key not in seen:
            seen.add(key)
            results.append(Segmentation(syllables=fixed_prefix + segs, pending=pending))

    if full_segs:
        for segs in full_segs:
            add(segs, "")
    else:
        # 整体切不出来，才去找"最长确定前缀 + 最短未完成尾巴"的 pending 切法：
        # 从右往左找第一个可行的切点就停（cut 越大，确定前缀越长、pending
        # 越短，是最贴近用户实际输入进度的解读；tail 留空的 cut 已经在上面
        # full_segs 里试过且失败了，这里从 len-1 开始即可）。同一个 cut 下
        # head 部分若仍有多种切法，只取贪心最短音节数那一种，不把所有排列
        # 组合都塞进 pending 候选（"ni+hao" vs "ni+ha+o" 只保留前者）。
        for cut in range(len(last_chunk) - 1, -1, -1):
            head, tail = last_chunk[:cut], last_chunk[cut:]
            if not is_prefix_of_some_syllable(tail):
                continue
            head_segs = _all_full_segmentations(head)
            if not head_segs:
                continue
            best_head = min(head_segs, key=_rank_key)
            add(best_head, tail)
            break

    if not results:
        # 彻底切不出来（比如打了非法字母组合），原样返回，
        # 上层可以用 is_complete 之类的判断做兜底展示。
        results.append(Segmentation(syllables=fixed_prefix, pending=last_chunk))

    # 排序：完整切法之间按音节数升序（贪心最长匹配优先）；
    # 有 pending 的切法之间按确定前缀音节数降序（尽量少留 pending）。
    def rank(r: Segmentation) -> tuple:
        confirmed = len(r.syllables) - len(fixed_prefix)
        return (0 if not r.pending else 1, len(r.syllables) if not r.pending else -confirmed)

    results.sort(key=rank)
    return results


def best_segmentation(raw: str) -> Segmentation:
    return segment(raw)[0]
