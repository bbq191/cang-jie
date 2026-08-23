from src.dictionary import Candidate, Dictionary
from src.traditional import build_traditional_table, to_traditional


def test_taiwan_and_hongkong_diverge_on_wordchoice():
    # 回归 8.2 节实测过的同一批词汇级转换差异，确保拼音引擎这边接的
    # 是同一套配置（s2twp/s2hk），不是退化成纯字形转换（那样 tw/hk 会给出同样的"網絡"）
    assert to_traditional("网络", region="tw") == "網路"
    assert to_traditional("网络", region="hk") == "網絡"
    assert to_traditional("软件", region="tw") == "軟體"
    assert to_traditional("软件", region="hk") == "軟件"


def test_simple_word_matches_expectation():
    assert to_traditional("中国", region="tw") == "中國"


def test_unknown_region_raises():
    import pytest
    with pytest.raises(ValueError):
        to_traditional("你好", region="jp")


def test_build_traditional_table_basic_conversion():
    table = {("zhong", "guo"): [Candidate(word="中国", weight=100.0)]}
    out = build_traditional_table(table, region="tw")
    assert [c.word for c in out[("zhong", "guo")]] == ["中國"]


def test_build_traditional_table_merges_real_collision():
    # 真实碰撞用例，不是构造的：雾凇拼音词库里"谙"（简体）和"諳"（词库
    # 自带的繁体异体，README 记录过"词库本身偏繁体、同一个字繁简变体
    # 都收"这个坑）转换后都是"諳"——合并前两条候选，合并后应该只剩
    # 一条，权重是两条之和（6174.0 + 0.0 = 6174.0），不是随便丢一条。
    d = Dictionary()
    table = {("an",): d.lookup(("an",))}
    before = table[("an",)]
    before_words = {c.word: c.weight for c in before}
    assert before_words.get("谙") == 6174.0
    assert before_words.get("諳") == 0.0

    out = build_traditional_table(table, region="tw")
    converted = out[("an",)]
    an_traditional = [c for c in converted if c.word == "諳"]
    assert len(an_traditional) == 1  # 合并成一条，不是保留两条重复的"諳"
    assert an_traditional[0].weight == 6174.0  # 权重相加，不是取其中一条


def test_build_traditional_table_resorts_after_merge():
    # 合并可能改变候选间的相对顺序，必须重新排序，不能假设合并前的
    # 顺序在合并后仍然有效——构造一个"合并后反超"的场景：候选 甲(20)
    # 权重最高，乙(15)/丙(15) 次之；转换后甲/乙撞成同一个词，合计 35，
    # 应该反超到原本排第一的甲前面。用假转换器（不依赖真实 OpenCC 数据
    # 里恰好存在这样的字）精确控制"谁跟谁撞车"，边界行为更可控——真实
    # OpenCC 碰撞用例已经在 test_build_traditional_table_merges_real_collision
    # 里覆盖过一次，两个测试角度不同，不是重复。
    table = {
        ("x",): [
            Candidate(word="甲", weight=20.0),
            Candidate(word="乙", weight=15.0),
            Candidate(word="丙", weight=15.0),
        ]
    }

    class _FakeConverter:
        # 甲/乙 都转换成"合并后"，丙独立保留——模拟"合并后反超"的场景，
        # 不依赖真实 OpenCC 数据里恰好有这样的字（真实碰撞用例已经在
        # 上面 test_build_traditional_table_merges_real_collision 里
        # 覆盖过一次，这里换成完全可控的假转换器测边界行为）。
        def convert(self, word: str) -> str:
            return "合并后" if word in ("甲", "乙") else word

    import src.traditional as traditional_module
    original_get_converter = traditional_module._get_converter
    traditional_module._get_converter = lambda region: _FakeConverter()
    try:
        out = build_traditional_table(table, region="tw")
    finally:
        traditional_module._get_converter = original_get_converter

    result = out[("x",)]
    assert result[0].word == "合并后"
    assert result[0].weight == 35.0  # 20 + 15，反超原本权重最高的"甲"(20)
    assert result[1].word == "丙"
    assert result[1].weight == 15.0
