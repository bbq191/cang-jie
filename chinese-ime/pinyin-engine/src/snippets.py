"""快捷输入（text replacement，iOS 同款语义）——Python 参照实现，
C 移植版是 c/src/snippets.c。

功能：用户在 `CJ_DATA_DIR/snippets.tsv` 里定义"缩写 → 短语"，打字时
原始字母缓冲区**精确等于**某个缩写，就把对应短语注入为第 0 位候选
（打缩写+空格即上屏短语，跟 iOS text replacement 一致）。输入法对这个
文件**只读**（用户在 host 侧编辑，C 端带 mtime 热重载，本参照实现不管
热重载——那是纯 I/O 策略，不是算法）。

规则（差分测试要求 Python/C 逐字节一致，钉死在这里）：

- 文件格式：UTF-8 文本，每行 "shortcut\\tphrase\\n"。
- 合法性：shortcut 非空、≤ SHORTCUT_MAX=32 字节、只含小写字母 a-z
  （拼音缓冲区只收字母，大写/数字/符号永远匹配不上，直接拒收防止用户
  白配）；phrase 非空、≤ PHRASE_MAX=256 字节、不含 tab（行格式所限，
  多一个 tab 即格式错整行跳过）。不合法的行静默跳过（fail-safe）。
- 容量：MAX_ENTRIES=256 条，超出的行忽略（按文件顺序先到先得）。
- 重复缩写：允许，全部保留，lookup 按**文件出现顺序**返回（用户自己
  控制排列，第一条排最前）。完全相同的 (shortcut, phrase) 行也不去重
  ——文件是用户的，写几条注入几条，所见即所得。
- lookup(buffer)：buffer 精确等于 shortcut 的全部条目，文件序。
"""
from __future__ import annotations

from pathlib import Path

MAX_ENTRIES = 256
SHORTCUT_MAX = 32
PHRASE_MAX = 256


def _valid_shortcut(s: bytes) -> bool:
    return 0 < len(s) <= SHORTCUT_MAX and all(0x61 <= c <= 0x7A for c in s)


class Snippets:
    def __init__(self) -> None:
        self._shortcuts: list[bytes] = []  # 文件顺序
        self._phrases: list[bytes] = []

    def load_bytes(self, data: bytes) -> None:
        """从文件内容加载（C 端 load 的算法部分，方便差分测试喂内容）。"""
        self._shortcuts, self._phrases = [], []
        for line in data.split(b"\n"):
            if len(self._shortcuts) >= MAX_ENTRIES:
                break
            if not line:
                continue
            parts = line.split(b"\t")
            if len(parts) != 2:
                continue
            sc, ph = parts
            if not _valid_shortcut(sc):
                continue
            if not ph or len(ph) > PHRASE_MAX:
                continue
            self._shortcuts.append(sc)
            self._phrases.append(ph)

    def load(self, path: str | Path) -> None:
        p = Path(path)
        self.load_bytes(p.read_bytes() if p.exists() else b"")

    def lookup(self, buffer: str | bytes) -> list[str]:
        b = buffer.encode("utf-8") if isinstance(buffer, str) else buffer
        return [
            self._phrases[i].decode("utf-8")
            for i in range(len(self._shortcuts))
            if self._shortcuts[i] == b
        ]

    def __len__(self) -> int:
        return len(self._shortcuts)
