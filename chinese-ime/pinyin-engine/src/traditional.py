"""
繁体候选转换层。

模块里有两套独立的转换机制，对应拼音输入法白皮书 05 节新旧两版设计，
都保留（旧版本不是被新版本淘汰删除，是使用场景不同）：

1. `to_traditional()`——最早的 9.4 节设计"候选栏保持简体，选中那一刻
   才转换"，`engine.commit()` 还在用这个，Python 桌面原型/离屏 UI 场景
   够用（不要求候选栏本身是繁体字）。

2. `build_traditional_table()`——05 节设计修订后的新方案，给设备端
   C 引擎用：候选栏本身要显示繁体字（跟简体模式功能完全对等），做法
   是在离线生成 `dict.bin`/`dict_jianpin.bin` 的同一条流水线里，额外
   批量生成一份繁体版词典表，C 引擎运行时只是换一个 mmap 目标文件，
   不需要在设备上跑任何 OpenCC 代码（05 节记录过的取舍：不把 OpenCC
   这个 C++ 库静态链接进 `cangjie-langhook.so`，离线一次性转换更安全）。

复用姊妹文档 3.2 节已经验证过的同一个 OpenCC 依赖和配置选择：词汇级
转换（s2twp/s2hk）而不是纯字形转换，理由同 3.2 节（"网络"->"網路"是
台湾习惯用词，纯字形转换只会给出"網絡"）。设备端语言切换器目前只有
"简体中文"/"繁体中文"两项（姊妹文档 Step R+S，没有单独的香港繁体选
项），`build_traditional_table()` 默认给的也是 `region="tw"`，`"hk"`
留着给可能的将来用，不是这轮的目标。
"""
from __future__ import annotations

from collections import defaultdict

import opencc

from .dictionary import Candidate

_REGION_CONFIG = {
    "tw": "s2twp",  # 台湾正体，词汇级
    "hk": "s2hk",   # 香港繁体，词汇级
}

_converter_cache: dict[str, opencc.OpenCC] = {}


def _get_converter(region: str) -> opencc.OpenCC:
    if region not in _REGION_CONFIG:
        raise ValueError(f"未知繁体地区: {region!r}，可选 {list(_REGION_CONFIG)}")
    if region not in _converter_cache:
        _converter_cache[region] = opencc.OpenCC(_REGION_CONFIG[region])
    return _converter_cache[region]


def to_traditional(text: str, region: str = "tw") -> str:
    return _get_converter(region).convert(text)


def build_traditional_table(
    table: dict[tuple[str, ...], list[Candidate]], region: str = "tw"
) -> dict[tuple[str, ...], list[Candidate]]:
    """把一份（简体）候选表整体转换成繁体版本，供离线生成
    `dict.zh_tw.bin`/`dict_jianpin.zh_tw.bin` 用（见 gen_dict_blob.py
    的 --variant 参数、gen_jianpin_blob.py 同名参数）。

    逐条候选转换后，同一个音节序列 key 下可能出现"不同简体词转换后
    撞成同一个繁体词"的情况（比如两种简体写法都对应同一个规范繁体
    写法）——不能留到运行时处理（C 端只做只读查询，不做去重合并），
    这里做跨候选的合并：撞车的候选权重相加（语义：这个繁体词形的
    综合使用支持度，是原来分散在不同简体写法上的权重的合计，不是
    随便选一个丢弃另一个），合并后按权重重新降序排序（原表已经按
    简体权重排好，合并可能改变相对顺序，必须重排，不能假设合并前的
    顺序还有效）。
    """
    out: dict[tuple[str, ...], list[Candidate]] = {}
    converter = _get_converter(region)
    for syllables, cands in table.items():
        merged: dict[str, float] = defaultdict(float)
        for c in cands:
            merged[converter.convert(c.word)] += c.weight
        merged_cands = [Candidate(word=w, weight=wt) for w, wt in merged.items()]
        merged_cands.sort(key=lambda c: c.weight, reverse=True)
        out[syllables] = merged_cands
    return out
