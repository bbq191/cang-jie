"""userfreq.py（v2 含 recency）单元测试——覆盖 docstring 钉死的每条规则：
有效分衰减、触碰结算、淘汰边界、排序并列、v1→v2 迁移。"""
import unittest
import tempfile
from pathlib import Path

import sys

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))
from src.userfreq import (  # noqa: E402
    UserFreq, MAX_ENTRIES, COUNT_CAP, KEY_MAX, WORD_MAX, HALF_LIFE,
)


def advance(uf: UserFreq, n: int) -> None:
    """推进逻辑时钟 n 步：往一个专用 key 上打 n 次（该 key 不参与断言）。"""
    for _ in range(n):
        uf.record("zzclock", "钟")


class TestRecordAndCount(unittest.TestCase):
    def test_basic_record_count(self):
        uf = UserFreq()
        self.assertEqual(uf.count("ni hao", "你好"), 0)
        uf.record("ni hao", "你好")
        uf.record("ni hao", "你好")
        self.assertEqual(uf.count("ni hao", "你好"), 2)
        self.assertEqual(uf.gen, 2)
        uf.record("nh", "你好")
        self.assertEqual(uf.count("nh", "你好"), 1)
        self.assertEqual(uf.word_total("你好"), 3)  # 无衰减时 = 计数和

    def test_count_saturates(self):
        uf = UserFreq()
        uf.record_n("a", "字", COUNT_CAP)
        uf.record("a", "字")
        self.assertEqual(uf.count("a", "字"), COUNT_CAP)

    def test_oversize_ignored_and_gen_untouched(self):
        uf = UserFreq()
        uf.record("k" * (KEY_MAX + 1), "词")
        uf.record("k", "字" * (WORD_MAX // 3 + 1))
        uf.record("", "词")
        uf.record("k", "")
        self.assertEqual(len(uf), 0)
        self.assertEqual(uf.gen, 0)  # 被忽略的调用不推进时钟
        uf.record("k" * KEY_MAX, "字" * (WORD_MAX // 3))
        self.assertEqual(len(uf), 1)
        self.assertEqual(uf.gen, 1)


class TestRecency(unittest.TestCase):
    def test_effective_decays_by_half_life(self):
        uf = UserFreq()
        uf.record_n("shi", "是", 8)          # last=0, gen→1
        self.assertEqual(uf.effective("shi", "是"), 8)
        advance(uf, HALF_LIFE - 1)            # gen=HALF_LIFE, age=HALF_LIFE
        self.assertEqual(uf.effective("shi", "是"), 4)
        advance(uf, HALF_LIFE)                # age=2*HALF_LIFE
        self.assertEqual(uf.effective("shi", "是"), 2)
        # 原始 count 不变（count() 返回存储值）
        self.assertEqual(uf.count("shi", "是"), 8)

    def test_touch_resettles_decay(self):
        uf = UserFreq()
        uf.record_n("shi", "是", 8)
        advance(uf, 2 * HALF_LIFE)
        self.assertEqual(uf.effective("shi", "是"), 2)
        uf.record("shi", "是")               # 触碰：结算衰减(2)+1=3，last=now
        self.assertEqual(uf.count("shi", "是"), 3)
        self.assertEqual(uf.effective("shi", "是"), 3)

    def test_new_habit_overtakes_old(self):
        uf = UserFreq()
        uf.record_n("xing", "行", 3)          # 旧习惯
        advance(uf, 3 * HALF_LIFE)            # 旧习惯衰减到 3>>3=0
        uf.record("xing", "型")               # 新习惯 1 次
        got = uf.words_for_key("xing", 5)
        self.assertEqual(got[0][0], "型")     # 新习惯在前
        # order 同理
        self.assertEqual(uf.order("xing", ["行", "型"], False), [1, 0])

    def test_word_total_uses_effective(self):
        uf = UserFreq()
        uf.record_n("a", "行", 8)
        advance(uf, HALF_LIFE)
        uf.record_n("b", "行", 1)
        # a 条目已衰减到 4，b 条目 1 → 5
        self.assertEqual(uf.word_total("行"), 5)


class TestEviction(unittest.TestCase):
    def _fill_via_load(self, uf, count):
        """用 load 铺满 2048 条、全部 last=同一值——绕开 record 推进时钟
        导致的不均匀衰减，让并列比较可控（复刻 v1 的确定性淘汰测试）。"""
        lines = [b"#gen\t3000\n"]
        for i in range(MAX_ENTRIES):
            lines.append(b"%d\t%04x\t\xe8\xaf\x8d\t3000\n" % (count, i))
        with tempfile.TemporaryDirectory() as d:
            p = Path(d) / "uf.tsv"
            p.write_bytes(b"".join(lines))
            uf.load(p)
        self.assertEqual(len(uf), MAX_ENTRIES)
        self.assertEqual(uf.gen, 3000)

    def test_evicts_lowest_effective(self):
        uf = UserFreq()
        self._fill_via_load(uf, 2)
        uf.record_n("zzzz", "新", 5)          # 5 > 2 → 淘汰一条旧的
        self.assertEqual(len(uf), MAX_ENTRIES)
        self.assertEqual(uf.count("zzzz", "新"), 5)

    def test_new_entry_dropped_when_weaker(self):
        uf = UserFreq()
        self._fill_via_load(uf, 2)
        uf.record("zzzz", "新")               # 1 < 2 → 丢弃
        self.assertEqual(uf.count("zzzz", "新"), 0)

    def test_tie_drops_byte_larger_newcomer(self):
        uf = UserFreq()
        self._fill_via_load(uf, 1)
        uf.record("zzzz", "新")               # 并列 1，zzzz 字节序最大 → 丢弃
        self.assertEqual(uf.count("zzzz", "新"), 0)
        uf.record("!!!!", "新")               # '!' < '0' → 淘汰字节序最大的现有条目
        self.assertEqual(uf.count("!!!!", "新"), 1)
        self.assertEqual(uf.count("%04x" % (MAX_ENTRIES - 1), "词"), 0)

    def test_aged_entries_evicted_first(self):
        uf = UserFreq()
        self._fill_via_load(uf, 2)
        # 满表时 advance 的新 key 会被淘汰规则挡掉、时钟不走——改用命中
        # 已有条目推进时钟（命中路径必推进 gen）
        for _ in range(2 * HALF_LIFE):
            uf.record("0000", "词")
        uf.record("aaaa", "新")               # 其余条目有效分已衰减到 0 < 1 → 插入成功
        self.assertEqual(uf.count("aaaa", "新"), 1)


class TestWordsForKeyAndOrder(unittest.TestCase):
    def test_words_for_key_sorted(self):
        uf = UserFreq()
        uf.record_n("shi", "是", 3)
        uf.record_n("shi", "时", 5)
        uf.record_n("shi", "事", 3)
        uf.record_n("si", "四", 9)
        got = uf.words_for_key("shi", 10)
        self.assertEqual(got, [("时", 5), ("事", 3), ("是", 3)])
        self.assertEqual(uf.words_for_key("wu", 5), [])

    def test_order_stable_when_all_zero(self):
        uf = UserFreq()
        self.assertEqual(uf.order("x", ["甲", "乙", "丙"], False), [0, 1, 2])

    def test_order_tie_keeps_original(self):
        uf = UserFreq()
        uf.record_n("a", "甲", 2)
        uf.record_n("a", "乙", 2)
        self.assertEqual(uf.order("a", ["丙", "乙", "甲"], False), [1, 2, 0])


class TestPersistence(unittest.TestCase):
    def test_roundtrip_bytes_stable(self):
        uf = UserFreq()
        uf.record_n("ni hao", "你好", 3)
        uf.record_n("nh", "你好", 1)
        uf.record_n("shi", "是", 7)
        with tempfile.TemporaryDirectory() as d:
            p = Path(d) / "uf.tsv"
            uf.save(p)
            first = p.read_bytes()
            self.assertTrue(first.startswith(b"#gen\t3\n"))
            uf2 = UserFreq()
            uf2.load(p)
            self.assertEqual(uf2.gen, 3)
            self.assertEqual(uf2.dump(), first)

    def test_v1_migration(self):
        with tempfile.TemporaryDirectory() as d:
            p = Path(d) / "uf.tsv"
            p.write_bytes(b"3\tni hao\t\xe4\xbd\xa0\xe5\xa5\xbd\n4\tshi\t\xe6\x98\xaf\n")
            uf = UserFreq()
            uf.load(p)
            self.assertEqual(len(uf), 2)
            self.assertEqual(uf.gen, 0)
            # last=0、gen=0 → 不衰减，v1 计数全额保留
            self.assertEqual(uf.effective("ni hao", "你好"), 3)
            self.assertEqual(uf.effective("shi", "是"), 4)

    def test_load_skips_malformed(self):
        with tempfile.TemporaryDirectory() as d:
            p = Path(d) / "uf.tsv"
            p.write_bytes(
                b"#gen\t50\n"
                b"#comment junk\n"
                b"3\tni hao\t\xe4\xbd\xa0\xe5\xa5\xbd\t10\n"   # 合法 v2
                b"not-a-number\tk\tw\t1\n"
                b"5\tk\tw\tbadlast\n"
                b"0\tk\tw\t1\n"
                b"2\tk\tw\t1\textra\n"
                b"\n"
                b"4\tshi\t\xe6\x98\xaf\t60\n"                   # last>header → gen=60
            )
            uf = UserFreq()
            uf.load(p)
            self.assertEqual(len(uf), 2)
            self.assertEqual(uf.gen, 60)
            self.assertEqual(uf.count("ni hao", "你好"), 3)
            self.assertEqual(uf.count("shi", "是"), 4)

    def test_load_merges_duplicate_lines(self):
        with tempfile.TemporaryDirectory() as d:
            p = Path(d) / "uf.tsv"
            p.write_bytes(b"3\ta\tw\t5\n2\ta\tw\t9\n")
            uf = UserFreq()
            uf.load(p)
            self.assertEqual(uf.count("a", "w"), 5)   # 3+2 不衰减
            self.assertEqual(uf.gen, 9)

    def test_load_missing_file_ok(self):
        uf = UserFreq()
        uf.record("a", "w")
        uf.load("/nonexistent/path/uf.tsv")
        self.assertEqual(len(uf), 0)
        self.assertEqual(uf.gen, 0)


if __name__ == "__main__":
    unittest.main()
