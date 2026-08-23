from src.dictionary import Dictionary
from src.jianpin import JianpinIndex, jianpin_key, jianpin_letter


def test_jianpin_letter_is_just_first_char():
    # 07 节设计的核心简化：不需要声母映射表，zh/ch/sh 的简拼字母天然
    # 就是它们自己的首字母
    assert jianpin_letter("zhong") == "z"
    assert jianpin_letter("chi") == "c"
    assert jianpin_letter("shi") == "s"
    assert jianpin_letter("bei") == "b"
    assert jianpin_letter("an") == "a"  # 零声母音节
    assert jianpin_letter("ying") == "y"


def test_jianpin_key_concatenates_letters():
    assert jianpin_key(("pin", "yin", "yin", "qing")) == "pyyq"
    assert jianpin_key(("zhong", "guo")) == "zg"


def test_jianpin_letter_rejects_empty_syllable():
    import pytest
    with pytest.raises(ValueError):
        jianpin_letter("")


def test_index_excludes_single_syllable_entries():
    # 单音节词没有"缩写"效果，不该被简拼索引收录（07 节实现细节，
    # 见 jianpin.py 模块文档字符串）——直接查内部条目列表核实这个不变量
    d = Dictionary()
    idx = JianpinIndex(d)
    assert all(len(e.key) >= 2 for e in idx._entries)  # noqa: SLF001 — 白盒检查内部不变量
    # 查长度为 1 的 key（比如 "国" 的全拼首字母 "g"）应该查不到任何东西——
    # 单字候选走的是 03 节 segment.c 的前缀匹配路径，不经过简拼索引
    assert idx.lookup("g") == []


def test_common_word_findable_by_jianpin():
    d = Dictionary()
    idx = JianpinIndex(d)
    # "中国" zhong guo -> 简拼 "zg"
    words = [c.word for c in idx.lookup("zg")]
    assert "中国" in words


def test_prefix_lookup_matches_longer_words():
    d = Dictionary()
    idx = JianpinIndex(d)
    # "py" 精确匹配（key 长度正好是 2）能找到 "拼音"（pin yin）本身——
    # 默认 limit=50 排不进去（"py" 这个 key 底下同音字词很多，"朋友"/
    # "便宜" 这类词权重比"拼音"本身还高），放大 limit 确认它确实在里面
    exact_words = [c.word for c in idx.lookup("py", limit=100000)]
    assert "拼音" in exact_words
    # "py" 前缀匹配还应该额外命中 key 更长的词，比如 "拼音输入法"
    # （pin yin shu ru fa -> key "pysrf"，以 "py" 开头但长度是 5）——
    # 这是前缀匹配跟精确匹配的真正区别，不是同一个查询换了个名字
    prefix_words = [c.word for c in idx.lookup_prefix("py", limit=100000)]
    assert "拼音输入法" in prefix_words
    assert "拼音" in prefix_words
    assert "拼音输入法" not in exact_words  # key "pysrf" != "py"，精确匹配找不到它


def test_lookup_prefix_sorted_by_weight_desc():
    d = Dictionary()
    idx = JianpinIndex(d)
    cands = idx.lookup_prefix("z", limit=30)
    weights = [c.weight for c in cands]
    assert weights == sorted(weights, reverse=True)


def test_unknown_key_returns_empty():
    d = Dictionary()
    idx = JianpinIndex(d)
    assert idx.lookup("zzzzzz") == []
    assert idx.lookup_prefix("zzzzzz") == []


def test_empty_prefix_returns_empty():
    d = Dictionary()
    idx = JianpinIndex(d)
    assert idx.lookup_prefix("") == []


def test_index_smaller_than_full_dictionary_candidate_count():
    # 排除单音节词之后，索引条目数应该明显小于词典总候选数（8105/41448
    # 两张单字表贡献了词典里相当一部分候选）
    d = Dictionary()
    idx = JianpinIndex(d)
    assert 0 < len(idx) < len(d)
