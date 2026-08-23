"""
双拼预处理层——对应白皮书 9.3 节"不需要单独的引擎，加一层键位转换"。

按键映射表没有凭记忆手写：双拼的按键含义会因为前一个键是什么而变
（比如同样按 "k" 键，在 "zh/ch/sh/r/z/c/s" 这类整体认读音节后面代表
"uai"，其它情况下代表 "ing"），手写错一条的代价是那一批音节全部解码错，
风险太高。改成直接复用 RIME 官方 <code>rime-double-pinyin</code> 仓库
的 schema 文件（<code>double_pinyin_flypy.schema.yaml</code> = 小鹤双拼，
<code>double_pinyin_natural.schema.yaml</code> = 自然码），这些 schema
本身就是 RIME 引擎拿来解码用的权威规则，我们只是照搬同一套规则跑一遍，
不是重新发明。

规则格式是 RIME 的 "algebra" DSL：一串 `xform/正则/替换/` 语句，
按顺序对字符串做正则替换（`$1` 是反向引用，等价于 Python 的 `\\1`）。
schema 里 `translator.preedit_format` 这一段规则链，本来是 RIME 拿来把
"用户敲的双拼按键" 实时转换成"预编辑区显示的完整拼音"用的——正好就是
我们需要的"双拼键位 -> 完整拼音"解码逻辑，直接复用，不用另外发明。
"""
from __future__ import annotations

import re
from pathlib import Path

import yaml

from .syllables import DATA_DIR

SCHEMES = {
    "flypy": DATA_DIR / "double_pinyin_flypy.schema.yaml",   # 小鹤双拼
    "natural": DATA_DIR / "double_pinyin_natural.schema.yaml",  # 自然码
}


def _parse_xform(rule: str) -> tuple[str, str] | None:
    """解析一条 RIME algebra 语句，只支持 xform（我们只需要解码方向用得到的这种）。"""
    if not rule.startswith("xform/"):
        return None  # derive/erase/xlit 用在编码方向（拼音->双拼），解码不需要
    body = rule[len("xform/"):]
    parts = body.split("/")
    if len(parts) < 2:
        return None
    pattern, repl = parts[0], parts[1]
    repl_py = re.sub(r"\$(\d)", r"\\\1", repl)  # RIME 的 $1 -> Python re 的 \1
    return pattern, repl_py


class ShuangpinScheme:
    def __init__(self, schema_path: Path):
        data = yaml.safe_load(schema_path.read_text(encoding="utf-8"))
        self.name = data["schema"]["name"]
        rules = data["translator"]["preedit_format"]
        self._compiled: list[tuple[re.Pattern, str]] = []
        for rule in rules:
            parsed = _parse_xform(rule)
            if parsed is None:
                continue
            pattern, repl = parsed
            self._compiled.append((re.compile(pattern), repl))

    def _decode_joined(self, joined: str) -> str:
        """对"按 2 键一组、组间空格分隔"的原始按键串跑一遍解码规则链。"""
        s = joined
        for pattern, repl in self._compiled:
            s = pattern.sub(repl, s)
        return s

    def keys_to_pinyin(self, keys: str) -> str:
        """双拼按键序列 -> 标准拼音字符串（音节间用 ' 分隔，喂给 segment.py）。

        双拼固定"每个音节正好 2 个按键"，所以直接按 2 字符一组切分；
        如果总按键数是奇数，说明最后一个音节只打了一半（双拼场景下这种
        "半个音节"没法用 schema 的规则链解码——那串规则本身就是按完整
        2 键设计的），这里的处理方式是把这半个键原样透传，不强行解码，
        交给上层（还没接，见模块顶部说明的局限）按"未完成前缀"处理。
        """
        keys = keys.strip().lower()
        if not keys:
            return ""
        chunks = [keys[i:i + 2] for i in range(0, len(keys), 2)]
        pending = ""
        if len(chunks[-1]) == 1:
            pending = chunks.pop()
        joined = " ".join(chunks)
        decoded = self._decode_joined(joined)
        pinyin = decoded.replace(" ", "'")
        if pending:
            pinyin = f"{pinyin}'{pending}" if pinyin else pending
        return pinyin


_scheme_cache: dict[str, ShuangpinScheme] = {}


def get_scheme(name: str = "flypy") -> ShuangpinScheme:
    if name not in _scheme_cache:
        if name not in SCHEMES:
            raise ValueError(f"未知双拼方案: {name!r}，可选 {list(SCHEMES)}")
        _scheme_cache[name] = ShuangpinScheme(SCHEMES[name])
    return _scheme_cache[name]


def keys_to_pinyin(keys: str, scheme: str = "flypy") -> str:
    return get_scheme(scheme).keys_to_pinyin(keys)
