from src.dictionary import Dictionary


def test_dictionary_loads_real_data():
    d = Dictionary()
    assert len(d) > 500000  # base.dict.yaml（雾凇拼音）实测约 55 万行词条


def test_common_word_lookup():
    d = Dictionary()
    cands = d.lookup(("ni", "hao"))
    words = [c.word for c in cands]
    assert "你好" in words


def test_common_char_outranks_rare_char_with_same_syllable():
    # 回归测试：换成 base.dict.yaml 之后重新验证——旧数据源（luna_pinyin）
    # 那个"词典自带百分比权重列只在同字内部有意义、不能跨字符比较"的坑，
    # 在新数据源里换了个姿势（第三列本来就是跨字符可比的整数词频，不该
    # 再需要额外修正），这条测试确保新数据源本身排序就是对的，不依赖
    # 任何 legacy 专属的 essay 频次表兜底。
    d = Dictionary()
    guo_cands = [c.word for c in d.lookup(("guo",))]
    assert guo_cands.index("国") < guo_cands.index("掴")

    de_cands = d.lookup(("de",))
    assert de_cands[0].word == "的"


def test_unknown_syllable_returns_empty():
    d = Dictionary()
    assert d.lookup(("zzz",)) == []


def test_prefix_lookup_matches_partial_last_syllable():
    d = Dictionary()
    # 已确定 "ni"，最后一个音节只打了 "h"（haoyihe 等前缀）
    cands = d.lookup_prefix(("ni", "h"))
    words = [c.word for c in cands]
    assert any(w for w in words)  # 至少有结果，不强绑定具体某个词，词库版本可能变


def test_legacy_luna_pinyin_still_loadable():
    # 旧数据源留作对比/回退，确认 legacy_luna_pinyin=True 这条路径没有跟着
    # 这轮改动一起坏掉——行为应该跟换源之前完全一致（essay 频次主排序 +
    # reading_share tie-break + essay 词组补位）。
    d = Dictionary(legacy_luna_pinyin=True)
    assert len(d) > 50000  # luna_pinyin + essay 组装出的候选量级，跟换源前一致

    guo_cands = [c.word for c in d.lookup(("guo",))]
    assert guo_cands.index("国") < guo_cands.index("掴")

    words = [c.word for c in d.lookup(("ni", "hao"))]
    assert "你好" in words  # essay 词组补位在旧模式下仍然生效
