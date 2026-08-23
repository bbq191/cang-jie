"""snippets.py 单元测试——覆盖 docstring 钉死的合法性/容量/顺序规则。"""
import unittest
import tempfile
from pathlib import Path
import sys

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))
from src.snippets import Snippets, MAX_ENTRIES, SHORTCUT_MAX, PHRASE_MAX  # noqa: E402


class TestLoadAndLookup(unittest.TestCase):
    def test_basic(self):
        s = Snippets()
        s.load_bytes("dz\t北京市海淀区中关村大街1号\nomw\tOn my way!\n".encode())
        self.assertEqual(s.lookup("dz"), ["北京市海淀区中关村大街1号"])
        self.assertEqual(s.lookup("omw"), ["On my way!"])
        self.assertEqual(s.lookup("xx"), [])
        self.assertEqual(s.lookup("d"), [])  # 前缀不触发，必须精确

    def test_duplicate_shortcuts_keep_file_order(self):
        s = Snippets()
        s.load_bytes(b"vx\twxid_abc\nvx\t13800138000\nvx\twxid_abc\n")
        self.assertEqual(s.lookup("vx"), ["wxid_abc", "13800138000", "wxid_abc"])

    def test_invalid_lines_skipped(self):
        s = Snippets()
        s.load_bytes(
            b"ok\tgood\n"
            b"UP\tbad-upper\n"          # 大写
            b"a1\tbad-digit\n"          # 数字
            b"\tno-shortcut\n"          # 空缩写
            b"noph\t\n"                 # 空短语
            b"twotab\ta\tb\n"           # 多 tab
            b"notab\n"                  # 无 tab
            + b"x" * (SHORTCUT_MAX + 1) + b"\ttoolong\n"
            + b"ph\t" + b"y" * (PHRASE_MAX + 1) + b"\n"
            b"\n"
            b"okk\tgood2\n"
        )
        self.assertEqual(len(s), 2)
        self.assertEqual(s.lookup("ok"), ["good"])
        self.assertEqual(s.lookup("okk"), ["good2"])

    def test_at_limit_accepted(self):
        s = Snippets()
        s.load_bytes(b"a" * SHORTCUT_MAX + b"\t" + b"p" * PHRASE_MAX + b"\n")
        self.assertEqual(len(s), 1)

    def test_capacity(self):
        lines = b"".join(b"a\tp%d\n" % i for i in range(MAX_ENTRIES + 50))
        s = Snippets()
        s.load_bytes(lines)
        self.assertEqual(len(s), MAX_ENTRIES)
        self.assertEqual(s.lookup("a")[0], "p0")
        self.assertEqual(len(s.lookup("a")), MAX_ENTRIES)

    def test_load_missing_file(self):
        s = Snippets()
        s.load_bytes(b"a\tb\n")
        with tempfile.TemporaryDirectory() as d:
            s.load(Path(d) / "missing.tsv")
        self.assertEqual(len(s), 0)


if __name__ == "__main__":
    unittest.main()
