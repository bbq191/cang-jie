from src.segment import segment, best_segmentation


def test_greedy_longest_match_xianggang():
    # 白皮书 9.2 节点名的例子：xianggang 要切成 xiang+gang，不是 xi+ang+gang
    seg = best_segmentation("xianggang")
    assert seg.syllables == ("xiang", "gang")
    assert seg.is_complete


def test_apostrophe_forces_boundary_xian_vs_xi_an():
    # 不带隔音符号：xian 默认整体切成一个音节（正词法惯例）
    assert best_segmentation("xian").syllables == ("xian",)
    # 带隔音符号：用户明确要求切成 xi + an
    assert best_segmentation("xi'an").syllables == ("xi", "an")


def test_apostrophe_disambiguates_ang_gang_case():
    # ang'gang 强制切成 ang + gang（对照白皮书举的隔音符号例子）
    seg = best_segmentation("ang'gang")
    assert seg.syllables == ("ang", "gang")


def test_multiple_consecutive_initials_edge_case():
    # 连续声母边界：nihao 应该切成 ni + hao，而不是把 h 并进上一个音节
    seg = best_segmentation("nihao")
    assert seg.syllables == ("ni", "hao")


def test_single_syllable():
    seg = best_segmentation("wo")
    assert seg.syllables == ("wo",)
    assert seg.is_complete


def test_empty_input():
    seg = best_segmentation("")
    assert seg.syllables == ()
    assert seg.is_complete


def test_pending_partial_last_syllable():
    # 用户刚打完 "zho"：不是完整音节，但是 zhong/zhou/zhang 等的合法前缀，
    # 应该识别成 "已确定为空 + pending='zho'"，而不是报错切不出来
    seg = best_segmentation("zho")
    assert seg.syllables == ()
    assert seg.pending == "zho"
    assert not seg.is_complete


def test_pending_after_complete_syllables():
    # "nihao" 后面接着敲 "z"（比如打"你好啊"里的 a 之前先手滑打了 z）——
    # 前面 ni+hao 已经完整，最后的 "z" 是下一个音节的前缀
    seg = best_segmentation("nihaoz")
    assert seg.syllables == ("ni", "hao")
    assert seg.pending == "z"


def test_invalid_input_does_not_crash():
    # 打了不合法字母组合（比如手滑），不应该抛异常，应该给出尽量合理的兜底切分
    seg = best_segmentation("qwqwqw")
    assert isinstance(seg.syllables, tuple)  # 不崩，至少有个结果


def test_multiple_candidates_include_greedy_first():
    # segment() 暴露全部候选切法时，贪心结果（音节数最少）应该排第一
    results = segment("xianggang")
    assert results[0].syllables == ("xiang", "gang")
