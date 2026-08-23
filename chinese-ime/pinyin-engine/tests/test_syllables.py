from src.syllables import SYLLABLES, is_valid_syllable, is_prefix_of_some_syllable


def test_common_syllables_are_valid():
    for syl in ["xiang", "gang", "ni", "hao", "zhong", "guo", "a", "e", "yi", "wu", "yu", "er"]:
        assert is_valid_syllable(syl), f"{syl} 应该是合法音节"


def test_illegal_combinations_are_rejected():
    # b/p/m/f 不跟 ia 这类韵母拼合；zh/ch/sh/r/z/c/s 不跟 j/q/x 专属的 i 系韵母拼合；
    # g/k/h 不跟 iang 这类韵母拼合
    # （注：'biang' 反而是合法音节——来自"biángbiáng面"那个 58 画的方言字 𰻞，
    #  真实存在于 RIME 词库里，写测试时手工核对过，不是这份规则的反例）
    for syl in ["bia", "jang", "zhia", "kiang", "pong"]:
        assert not is_valid_syllable(syl), f"{syl} 不应该是合法音节"


def test_retroflex_and_apical_i():
    # zhi/chi/shi/ri/zi/ci/si 里的 "i" 是舌尖元音，属于合法音节，
    # 但不能反过来推出 "zh" 单独 + "i" 单独也在表里之外还有别的含义
    for syl in ["zhi", "chi", "shi", "ri", "zi", "ci", "si"]:
        assert is_valid_syllable(syl)


def test_prefix_lookup():
    assert is_prefix_of_some_syllable("zho")   # zhong / zhou / zhang 的前缀
    assert is_prefix_of_some_syllable("x")
    assert not is_prefix_of_some_syllable("zzz")


def test_syllable_table_reasonable_size():
    # 公认的普通话合法音节数量级在 400 左右（不含声调），
    # 用一个宽松区间做 sanity check，不锁死具体数字（词库版本更新可能有微调）。
    assert 350 <= len(SYLLABLES) <= 500, f"音节表大小异常：{len(SYLLABLES)}"
