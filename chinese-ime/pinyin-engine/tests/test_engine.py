from src.engine import PinyinEngine


def test_full_word_hit():
    engine = PinyinEngine()
    result = engine.query("nihao")
    assert "你好" in result.candidates
    assert result.candidates[0] == "你好"  # 词典精确匹配优先于逐字拼凑


def test_single_syllable_gives_top_char():
    engine = PinyinEngine()
    result = engine.query("de")
    assert result.candidates[0] == "的"


def test_sentence_fallback_when_no_exact_phrase():
    # 一个词典里大概率没收录成一个词条、但每个音节都有对应字的组合，
    # 应该退化成"贪心分词 + 逐字拼句"，而不是空手而归
    engine = PinyinEngine()
    result = engine.query("woshizhongguoren")  # 我是中国人
    assert result.candidates, "应该至少拼出一个候选句"
    assert "我" in result.candidates[0]


def test_pending_syllable_still_returns_candidates():
    engine = PinyinEngine()
    result = engine.query("nihaoz")  # ni + hao 完整，z 是下一个音节前缀
    assert "你好" in result.candidates


def test_commit_preview_falls_back_to_raw_when_unknown():
    engine = PinyinEngine()
    preview = engine.commit_preview("zzzzz")
    assert preview == "zzzzz"  # 词典查不到就原样透传，不瞎编字


def test_shuangpin_query_matches_equivalent_full_pinyin():
    # 9.3 节设计的验证：双拼只是键位转换层，同一个词双拼查询和全拼查询
    # 应该给出一样的候选（"中国" 全拼 zhongguo，flypy 双拼 vsgo）
    engine = PinyinEngine()
    full = engine.query("zhongguo")
    sp = engine.query_shuangpin("vsgo", scheme="flypy")
    assert sp.candidates == full.candidates
    assert sp.candidates[0] == "中国"


def test_query_jianpin_exact_match():
    # 07 节简拼查询： "zg" 精确匹配"中国"（zhong guo -> z, g）
    engine = PinyinEngine()
    words = engine.query_jianpin("zg")
    assert "中国" in words


def test_query_jianpin_prefix_fills_in_when_exact_is_sparse():
    engine = PinyinEngine()
    # "py" 前缀应该能找到 "拼音输入法"（pin yin shu ru fa -> key "pysrf"）
    # 这种精确匹配找不到、只有前缀匹配才能覆盖的更长词
    words = engine.query_jianpin("py", limit=200)
    assert "拼音输入法" in words


def test_query_jianpin_empty_input_returns_empty():
    engine = PinyinEngine()
    assert engine.query_jianpin("") == []


def test_query_jianpin_unknown_key_returns_empty():
    engine = PinyinEngine()
    assert engine.query_jianpin("zzzzzz") == []


def test_commit_converts_to_traditional_only_at_commit_point():
    # 9.4 节设计的验证：候选栏本身（query 的返回值）保持简体，
    # 只有 commit() 这一步才转换，且转换结果匹配 8.2 节验证过的词汇级差异
    engine = PinyinEngine()
    result = engine.query("wangluo")  # 网络
    assert "网络" in result.candidates  # 候选栏候选仍是简体

    assert engine.commit("网络", region=None) == "网络"       # 简体模式不转换
    assert engine.commit("网络", region="tw") == "網路"        # 台湾正体，词汇级
    assert engine.commit("网络", region="hk") == "網絡"        # 香港繁体，词汇级
