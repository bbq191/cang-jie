import pytest

from src.mixed_input import Action, ActionKind, InputMode, MixedInputState


def test_lowercase_char_when_idle_defers_to_pinyin():
    s = MixedInputState()
    action = s.handle_char("p")
    assert action.kind is ActionKind.PASSTHROUGH_KEY
    assert s.mode is InputMode.NONE
    assert s.buffer == ""


def test_uppercase_char_when_idle_enters_passthrough():
    s = MixedInputState()
    action = s.handle_char("H")
    assert action.kind is ActionKind.BUFFER
    assert s.mode is InputMode.PASSTHROUGH
    assert s.buffer == "H"


def test_subsequent_chars_append_regardless_of_case():
    s = MixedInputState()
    s.handle_char("H")
    for ch in "ello":
        action = s.handle_char(ch)
        assert action.kind is ActionKind.BUFFER
    assert s.buffer == "Hello"
    assert s.mode is InputMode.PASSTHROUGH


def test_mixed_case_word_preserved_verbatim():
    # "HTML" 全部 shift 态敲出来，中途也可能松开 Shift 打小写——不管
    # 每个字符的大小写来源如何，原样追加，不做任何大小写规范化。
    s = MixedInputState()
    for ch in "HTml":
        s.handle_char(ch)
    assert s.buffer == "HTml"


def test_space_commits_buffer_and_resets():
    s = MixedInputState()
    for ch in "Hello":
        s.handle_char(ch)
    action = s.handle_space_or_enter()
    assert action.kind is ActionKind.COMMIT_BUFFER
    assert action.text == "Hello"
    assert s.mode is InputMode.NONE
    assert s.buffer == ""


def test_enter_commits_buffer_same_as_space():
    s = MixedInputState()
    for ch in "Hi":
        s.handle_char(ch)
    action = s.handle_space_or_enter()  # 同一个方法处理空格和回车，见模块文档
    assert action.kind is ActionKind.COMMIT_BUFFER
    assert action.text == "Hi"


def test_space_when_idle_passes_through():
    s = MixedInputState()
    action = s.handle_space_or_enter()
    assert action.kind is ActionKind.PASSTHROUGH_KEY
    assert s.mode is InputMode.NONE


def test_backspace_pops_last_char():
    s = MixedInputState()
    for ch in "Hi":
        s.handle_char(ch)
    action = s.handle_backspace()
    assert action.kind is ActionKind.BUFFER
    assert s.buffer == "H"
    assert s.mode is InputMode.PASSTHROUGH  # 缓冲区还没空，仍在西文直通模式


def test_backspace_to_empty_resets_mode():
    s = MixedInputState()
    s.handle_char("H")
    action = s.handle_backspace()
    assert action.kind is ActionKind.BUFFER
    assert s.buffer == ""
    assert s.mode is InputMode.NONE  # 弹空后自动退出西文直通模式


def test_backspace_when_idle_passes_through():
    s = MixedInputState()
    action = s.handle_backspace()
    assert action.kind is ActionKind.PASSTHROUGH_KEY
    assert s.mode is InputMode.NONE


def test_other_key_flushes_nonempty_buffer():
    s = MixedInputState()
    for ch in "Hey":
        s.handle_char(ch)
    action = s.handle_other_key()
    assert action.kind is ActionKind.COMMIT_BUFFER
    assert action.text == "Hey"
    assert s.mode is InputMode.NONE
    assert s.buffer == ""


def test_other_key_when_idle_passes_through():
    s = MixedInputState()
    action = s.handle_other_key()
    assert action.kind is ActionKind.PASSTHROUGH_KEY


def test_handle_char_rejects_invalid_input():
    s = MixedInputState()
    for bad in ("", "ab", "3", "!", None):
        with pytest.raises(ValueError):
            s.handle_char(bad)  # type: ignore[arg-type]


def test_full_round_trip_scenario():
    # 模拟真实的一次西文直通输入：H-e-l-l-o-空格，提交完之后紧接着敲
    # 一个小写字母应该重新回到"交给拼音模式"的判断，不是继续追加进
    # 已经清空的缓冲区。
    s = MixedInputState()
    events = ["H", "e", "l", "l", "o"]
    for ch in events:
        action = s.handle_char(ch)
        assert action.kind is ActionKind.BUFFER
    commit = s.handle_space_or_enter()
    assert commit == Action(ActionKind.COMMIT_BUFFER, text="Hello")

    # 紧接着敲一个小写字母：应该被判定为"交给拼音模式"，不是残留状态
    next_action = s.handle_char("n")
    assert next_action.kind is ActionKind.PASSTHROUGH_KEY
    assert s.mode is InputMode.NONE


def test_two_consecutive_english_words_are_independent_commits():
    s = MixedInputState()
    for ch in "Hi":
        s.handle_char(ch)
    first = s.handle_space_or_enter()
    assert first.text == "Hi"

    for ch in "There":
        s.handle_char(ch)
    second = s.handle_space_or_enter()
    assert second.text == "There"  # 不会残留上一个词的内容
