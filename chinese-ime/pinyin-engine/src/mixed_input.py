"""
中英混输状态机——对应拼音输入法白皮书 07 节"中英混输——Shift 触发
西文直通模式"。

这是这个功能的第一版实现：纯 Python、不碰设备、不碰 `hook_init.c`——
跟 `segment.py`/`jianpin.py` 当初"先在 Python 里把状态机逻辑钉死、
写满边界测试，再翻译成 C 接进 hook"是同一个节奏。这里的"按键"都是
`handle_*()` 方法调用，不是真实设备事件；真机接入是这份原型验证完之后
的独立下一步，不在这一轮范围内。

设计（07 节原话，这里是逐条落地）：
  - 缓冲区为空时敲的第一个字符键，如果是 Shift（大写）态，整个输入
    会话切换进"西文直通"模式，不再走任何拼音/简拼解析；如果是普通
    （小写）态，交给拼音模式处理——那部分状态机已经在 `hook_init.c`
    的 Step T 实现，这个类不重复，只返回 `PASSTHROUGH_KEY`（"这次
    按键不归我管，你原来该怎么处理怎么处理"），不猜测拼音层会做什么。
  - 已经在西文直通模式后，字符原样追加（大小写不转换，`handle_char`
    接收的就是姊妹文档 02 节 Step N 已经验证过的"普通态/shift 态两个
    独立槽位读出来的那个字符串本身"，这一层不重新判断大小写规则）。
  - 空格/回车：缓冲区非空则原样提交缓冲区内容（不是插入真空格/发送
    真回车，跟 hook_init.c Step T 现有的"提交占位"机制同一个模式）；
    缓冲区为空则原样透传（正常空格/回车）。
  - 退格：弹出最后一个字符；弹空后自动退出西文直通模式，回到"下一次
    按键决定模式"的初始状态。
  - CapsLock/符号切换等其它按键：缓冲区非空先提交一次，再照常处理那个
    按键（`handle_other_key`）——跟 Step T 对拼音缓冲区的安全网是同一个
    模式，避免切走键盘布局后留一个悬空、不会再更新的缓冲区。
  - 明确排除启发式自动检测："能不能构成合法拼音前缀"这种判断对绝大多数
    英文单词都不成立（每个字母单独看几乎都是合法声母/韵母开头），07 节
    已经论证过这条路不可靠，改用显式 Shift 信号，判定 100% 确定。

集成假设（这份原型不处理，留给上层）：这个状态机只在"已经确认是中文
模式"（`virtualKeyboard.language` 是 zh_CN/zh_TW，Step T 已有的判断）
的前提下才会被调用——纯英文模式下用户敲字母本来就应该走原生路径，不
经过这里；这个假设不需要在这个类里重复实现，是调用方的职责。
"""
from __future__ import annotations

from dataclasses import dataclass
from enum import Enum, auto


class InputMode(Enum):
    NONE = auto()          # 缓冲区为空，下一次字符键决定模式
    PASSTHROUGH = auto()   # 西文直通模式，缓冲区非空


class ActionKind(Enum):
    BUFFER = auto()           # 已追加进缓冲区（供候选栏之类的上层展示用），不提交任何文本
    COMMIT_BUFFER = auto()    # 把缓冲区原样提交，之后缓冲区已清空、mode 已回到 NONE
    PASSTHROUGH_KEY = auto()  # 这次按键不归这个状态机管，原样按原有逻辑处理


@dataclass(frozen=True)
class Action:
    kind: ActionKind
    text: str = ""  # 只有 COMMIT_BUFFER 时有意义，是要提交的完整文本


class MixedInputState:
    def __init__(self) -> None:
        self.mode: InputMode = InputMode.NONE
        self.buffer: str = ""

    def handle_char(self, text: str) -> Action:
        """text 是这次按键实际应该提交的单个字符（已经是普通态还是
        shift 态选好的那一个，比如 "a" 或 "A"）——不在这里重新判断/
        转换大小写，只用它的大小写决定"要不要进入西文直通"。"""
        if not isinstance(text, str) or len(text) != 1 or not text.isalpha():
            raise ValueError(f"handle_char 只接受单个字母: {text!r}")

        if self.mode is InputMode.NONE:
            if text.islower():
                return Action(ActionKind.PASSTHROUGH_KEY)  # 交给拼音模式处理，这里不管
            self.mode = InputMode.PASSTHROUGH
            self.buffer = text
            return Action(ActionKind.BUFFER)

        self.buffer += text  # 已在西文直通模式：大小写原样追加，不重新判断
        return Action(ActionKind.BUFFER)

    def handle_space_or_enter(self) -> Action:
        if self.mode is InputMode.PASSTHROUGH and self.buffer:
            return self._commit()
        return Action(ActionKind.PASSTHROUGH_KEY)  # 缓冲区为空：正常打空格/回车

    def handle_backspace(self) -> Action:
        if self.mode is InputMode.PASSTHROUGH and self.buffer:
            self.buffer = self.buffer[:-1]
            if not self.buffer:
                self._reset()
            return Action(ActionKind.BUFFER)
        return Action(ActionKind.PASSTHROUGH_KEY)  # 缓冲区为空：正常删除文本框里已提交的内容

    def handle_other_key(self) -> Action:
        """CapsLock/符号切换/Shift 单独按下之类，不属于上面任何一类的
        按键——跟 Step T 对拼音缓冲区的安全网同一个模式：缓冲区非空先
        提交一次（避免残留悬空缓冲区），提交动作发生后这次按键本身依然
        要按原逻辑处理（调用方在拿到 COMMIT_BUFFER 之后自己决定怎么处理
        触发这次调用的原始按键，这个方法只负责"要不要先冲掉缓冲区"这一
        问，不吞掉原始按键事件）。"""
        if self.mode is InputMode.PASSTHROUGH and self.buffer:
            return self._commit()
        return Action(ActionKind.PASSTHROUGH_KEY)

    def _commit(self) -> Action:
        text = self.buffer
        self._reset()
        return Action(ActionKind.COMMIT_BUFFER, text=text)

    def _reset(self) -> None:
        self.mode = InputMode.NONE
        self.buffer = ""
