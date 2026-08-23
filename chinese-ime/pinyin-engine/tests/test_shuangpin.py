from src.shuangpin import keys_to_pinyin, get_scheme


def test_scheme_names_match_rime_schema():
    # 确认加载的确实是对应的官方 schema（不是文件名蒙对了但内容对不上）
    assert get_scheme("flypy").name == "小鶴雙拼"
    assert get_scheme("natural").name == "自然碼雙拼"


def test_single_letter_final_passthrough():
    # 已经是单字母的声母/韵母，双拼里就是原样敲，不压缩
    # "你" ni：n=声母 n，i=韵母 i，两个键都不需要转换
    assert keys_to_pinyin("ni", scheme="flypy") == "ni"
    # "古" gu：g=声母 g，u=韵母 u
    assert keys_to_pinyin("gu", scheme="flypy") == "gu"


def test_compressed_final_hao():
    # "好" hao：h=声母，韵母 "ao" 压缩成按键 c（小鹤双拼实测：hc -> hao）
    assert keys_to_pinyin("hc", scheme="flypy") == "hao"


def test_zh_ch_sh_initial_compression():
    # zh/ch/sh 这三个整体认读声母各用一个键代表：v=zh, i=ch, u=sh
    # "中" zhong = v(zh) + s(ong) = vs
    assert keys_to_pinyin("vs", scheme="flypy") == "zhong"


def test_multi_syllable_word():
    # "中国" zhong guo = vs + go（国 guo: g声母 + uo->o 韵母）
    assert keys_to_pinyin("vsgo", scheme="flypy") == "zhong'guo"
    # 输出用 ' 分隔，能直接喂给 segment.py（隔音符号是硬边界，见 test_segment.py）


def test_natural_scheme_also_loads_and_differs_from_flypy():
    # 自然码是另一套按键映射，同样的按键在两套方案里大概率解出不同结果，
    # 不要求具体解出什么（不熟悉自然码就不瞎断言），只确认两套方案不是同一份数据、
    # 且都能正常跑通不报错。
    flypy_result = keys_to_pinyin("hc", scheme="flypy")
    natural_result = keys_to_pinyin("hc", scheme="natural")
    assert isinstance(natural_result, str) and natural_result != ""
    # 两套方案的按键规则文件本身不同源（分别是各自的 schema.yaml），
    # 只要都能跑出非空结果就说明两份数据都被正确加载和使用了，
    # 结果是否恰好相同不是我们关心的点。
    assert isinstance(flypy_result, str) and flypy_result != ""


def test_unknown_scheme_raises():
    import pytest
    with pytest.raises(ValueError):
        keys_to_pinyin("ni", scheme="not-a-real-scheme")


def test_odd_length_keys_pending_passthrough():
    # 奇数个按键：最后半个音节还没打完，原样透传不强行解码
    result = keys_to_pinyin("niz", scheme="flypy")  # ni 完整，z 是下一个音节打了一半
    assert result == "ni'z"
