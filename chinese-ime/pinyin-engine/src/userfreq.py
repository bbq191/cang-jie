"""用户词频（自适应调频 + recency 近因因子）——Python 参照实现，
C 移植版是 c/src/userfreq.c。

功能定位：记录用户点选的 (key, word) 计数，查询时把选过的词提前（精确
key 强制召回 + 前缀池按 word 聚合重排）。静态词频（dict.bin）不动。

v2（2026-08-22）加入 **recency 因子**：纯累计计数会让旧习惯永远压住新
习惯——引入**逻辑时钟 gen**（每次成功记账 +1，不用墙钟，保确定性）与
每条目 last（最后使用时的 gen），排序/召回/淘汰一律用**有效分**：

    effective(e, gen) = e.count >> min((gen - e.last) // HALF_LIFE, 31)

即每闲置 HALF_LIFE=128 次（全局）选择，条目影响力减半。命中时先把存储
count 结算成有效分再 +n（"触碰即结算衰减"），last 更新为当前 gen。

设计规则（差分测试要求 Python/C 逐字节一致，全部钉死在这里）：

- 条目 = (key, word, count, last)。key/word 语义同 v1（查询 key 原文/
  UTF-8 词）；容量 MAX_ENTRIES=2048、key ≤ 64B、word ≤ 48B、count 饱和
  COUNT_CAP=1000000。超长/空的 record 静默忽略（不推进 gen）。
- gen 推进：仅当 record_n 实际改动了表（命中加分或插入成功）才 +1；
  被淘汰规则丢弃的新条目不推进。
- record_n 命中：count = min(effective(e, gen) + n, CAP)；last = gen；
  gen += 1。新条目：count=min(n,CAP)、last=gen、gen += 1。
- 内存序：按 (key 字节序, word 字节序) 升序（二分查找）。
- 淘汰（满员时插入）：受害者 = effective 最小者，并列取 (key, word)
  字节序最大者；新条目以 n（它的有效分）参与比较——受害者 effective
  更大则丢弃新条目；相等且新条目字节序更大也丢弃；否则淘汰受害者插入。
- words_for_key(key, max)：该 key 下条目，effective 降序，并列 word
  字节序升序。word_total(word)：各条目 effective 之和（内存序逐条饱和
  累加）。order(...)：计数 = effective（或 word_total），降序稳定排序。
  count(key, word) 返回**原始存储 count**（诊断用，不衰减）。
- 文件格式 v2：首行 "#gen\t<gen>"，随后每行 "count\tkey\tword\tlast"
  （内存序，同一状态 save 两次逐字节一致）。load（整体替换）：
  * "#" 开头行：解析 "#gen\t<数字>" 得 gen（无效则忽略该行）；
  * 4 字段行：新格式（count/last 非法或 count=0 跳过；count 钳 CAP）；
  * 3 字段行：v1 旧格式，last=0（无损迁移：gen 也为 0 时不衰减）；
  * 其余行跳过。重复 (key, word) 行：count 直接相加（饱和、不衰减）、
    last 取较大者。load 结束后 gen = max(头部 gen, 所有条目 last)。
    满员时按淘汰规则处理（比较用当时已累积的 gen 下的 effective）。
  旧 .so 读 v2 文件：头部行与 4 字段行都会被 v1 解析器跳过 → 空表
  （回滚丢学习数据、不崩，可接受）。
"""
from __future__ import annotations

from pathlib import Path

MAX_ENTRIES = 2048
KEY_MAX = 64
WORD_MAX = 48
COUNT_CAP = 1_000_000
HALF_LIFE = 128
GEN_MAX = 0xFFFFFFFF


class UserFreq:
    def __init__(self) -> None:
        # 平行列表，按 (key_bytes, word_bytes) 升序；跟 C 版的结构体数组对应
        self._keys: list[bytes] = []
        self._words: list[bytes] = []
        self._counts: list[int] = []
        self._lasts: list[int] = []
        self.gen: int = 0

    # ---- 内部 ----
    def _effective(self, i: int) -> int:
        shift = min((self.gen - self._lasts[i]) // HALF_LIFE, 31)
        return self._counts[i] >> shift

    def _find(self, kb: bytes, wb: bytes) -> tuple[int, bool]:
        """返回 (下标, 是否命中)：命中时是条目下标，未命中时是插入点。"""
        lo, hi = 0, len(self._keys)
        while lo < hi:
            mid = (lo + hi) // 2
            if (self._keys[mid], self._words[mid]) < (kb, wb):
                lo = mid + 1
            else:
                hi = mid
        hit = lo < len(self._keys) and self._keys[lo] == kb and self._words[lo] == wb
        return lo, hit

    def _victim(self) -> int:
        """淘汰受害者下标：effective 最小，并列取 (key, word) 字节序最大。"""
        best = 0
        for i in range(1, len(self._keys)):
            ei, eb = self._effective(i), self._effective(best)
            if ei < eb or (
                ei == eb
                and (self._keys[i], self._words[i]) > (self._keys[best], self._words[best])
            ):
                best = i
        return best

    def _bump_gen(self) -> None:
        if self.gen < GEN_MAX:
            self.gen += 1

    # ---- 写入 ----
    def record(self, key: str | bytes, word: str | bytes) -> None:
        self.record_n(key, word, 1)

    def record_n(self, key: str | bytes, word: str | bytes, n: int) -> None:
        self._record_n_at(key, word, n, None)

    def _record_n_at(self, key, word, n: int, last: int | None) -> None:
        """last=None：正常记账（last=gen、成功后 gen+1）。
        last=数值：load 路径（用行内 last、不推进 gen）。"""
        if n <= 0:
            return
        kb = key.encode("utf-8") if isinstance(key, str) else key
        wb = word.encode("utf-8") if isinstance(word, str) else word
        if not kb or not wb or len(kb) > KEY_MAX or len(wb) > WORD_MAX:
            return  # fail-safe：超长/空静默忽略（不推进 gen）
        n = min(n, COUNT_CAP)
        idx, hit = self._find(kb, wb)
        if hit:
            if last is None:
                # 命中：先结算衰减再加，last 更新到当前 gen
                self._counts[idx] = min(self._effective(idx) + n, COUNT_CAP)
                self._lasts[idx] = self.gen
                self._bump_gen()
            else:
                # load 重复行：count 直接相加（不衰减）、last 取较大
                self._counts[idx] = min(self._counts[idx] + n, COUNT_CAP)
                self._lasts[idx] = max(self._lasts[idx], last)
            return
        entry_last = self.gen if last is None else last
        if len(self._keys) >= MAX_ENTRIES:
            v = self._victim()
            ev = self._effective(v)
            # 新条目以 n（它的有效分）参与比较
            if ev > n:
                return
            if ev == n and (kb, wb) > (self._keys[v], self._words[v]):
                return
            del self._keys[v], self._words[v], self._counts[v], self._lasts[v]
            idx, _ = self._find(kb, wb)  # 删除可能移动插入点，重算
        self._keys.insert(idx, kb)
        self._words.insert(idx, wb)
        self._counts.insert(idx, n)
        self._lasts.insert(idx, entry_last)
        if last is None:
            self._bump_gen()

    # ---- 查询 ----
    def count(self, key: str | bytes, word: str | bytes) -> int:
        kb = key.encode("utf-8") if isinstance(key, str) else key
        wb = word.encode("utf-8") if isinstance(word, str) else word
        idx, hit = self._find(kb, wb)
        return self._counts[idx] if hit else 0

    def effective(self, key: str | bytes, word: str | bytes) -> int:
        kb = key.encode("utf-8") if isinstance(key, str) else key
        wb = word.encode("utf-8") if isinstance(word, str) else word
        idx, hit = self._find(kb, wb)
        return self._effective(idx) if hit else 0

    def word_total(self, word: str | bytes) -> int:
        wb = word.encode("utf-8") if isinstance(word, str) else word
        total = 0
        for i in range(len(self._keys)):
            if self._words[i] == wb:
                total = min(total + self._effective(i), COUNT_CAP)
        return total

    def words_for_key(self, key: str | bytes, max_out: int) -> list[tuple[str, int]]:
        kb = key.encode("utf-8") if isinstance(key, str) else key
        if max_out <= 0:
            return []
        idx, _ = self._find(kb, b"")
        rows: list[tuple[int, bytes, int]] = []
        i = idx
        while i < len(self._keys) and self._keys[i] == kb:
            rows.append((self._effective(i), self._words[i], self._counts[i]))
            i += 1
        rows.sort(key=lambda r: (-r[0], r[1]))
        return [(w.decode("utf-8"), c) for e, w, c in rows[:max_out]]

    def order(self, key: str | bytes, words: list[str | bytes], use_word_total: bool) -> list[int]:
        vals = []
        for w in words:
            if use_word_total:
                vals.append(self.word_total(w))
            else:
                vals.append(self.effective(key, w))
        # 稳定：有效分降序，并列保持原始下标
        return sorted(range(len(words)), key=lambda i: (-vals[i], i))

    # ---- 持久化 ----
    def dump(self) -> bytes:
        out = [b"#gen\t%d\n" % self.gen]
        for i in range(len(self._keys)):
            out.append(b"%d\t%s\t%s\t%d\n"
                       % (self._counts[i], self._keys[i], self._words[i], self._lasts[i]))
        return b"".join(out)

    def save(self, path: str | Path) -> None:
        Path(path).write_bytes(self.dump())

    def load(self, path: str | Path) -> None:
        self._keys, self._words, self._counts, self._lasts = [], [], [], []
        self.gen = 0
        p = Path(path)
        if not p.exists():
            return
        for line in p.read_bytes().split(b"\n"):
            if not line:
                continue
            if line.startswith(b"#"):
                parts = line.split(b"\t")
                if len(parts) == 2 and parts[0] == b"#gen" and parts[1].isdigit():
                    self.gen = max(self.gen, min(int(parts[1]), GEN_MAX))
                continue
            parts = line.split(b"\t")
            if len(parts) == 4:
                cnt_b, kb, wb, last_b = parts
                if not cnt_b.isdigit() or not last_b.isdigit():
                    continue
                cnt = int(cnt_b)
                if cnt <= 0:
                    continue
                last = min(int(last_b), GEN_MAX)
                self.gen = max(self.gen, last)
                self._record_n_at(kb, wb, min(cnt, COUNT_CAP), last)
            elif len(parts) == 3:
                cnt_b, kb, wb = parts  # v1 旧格式，last=0 无损迁移
                if not cnt_b.isdigit():
                    continue
                cnt = int(cnt_b)
                if cnt <= 0:
                    continue
                self._record_n_at(kb, wb, min(cnt, COUNT_CAP), 0)

    def __len__(self) -> int:
        return len(self._keys)
