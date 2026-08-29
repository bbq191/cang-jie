"""笔划几何工具：弧长重采样 + 转角 + 角点检测。

纯函数、无 rmscene 依赖，方便差分测试与后续移植 Rust（remarkable_lines）。
坐标单位与 .rm 一致（设备像素，float，可负）。角度一律用**度**。
"""
from __future__ import annotations

import math

Pt = tuple[float, float]


def bbox(pts: list[Pt]) -> tuple[float, float, float, float]:
    xs = [p[0] for p in pts]
    ys = [p[1] for p in pts]
    return min(xs), min(ys), max(xs), max(ys)


def size_diag(pts: list[Pt]) -> float:
    x0, y0, x1, y1 = bbox(pts)
    return math.hypot(x1 - x0, y1 - y0)


def aspect(pts: list[Pt]) -> float:
    """长宽比 = 长边 / 短边（≥1）。星形≈1，下划线/括号很大。"""
    x0, y0, x1, y1 = bbox(pts)
    w, h = x1 - x0, y1 - y0
    lo, hi = sorted((abs(w), abs(h)))
    return hi / lo if lo > 1e-6 else 1e9


def resample(pts: list[Pt], n: int) -> list[Pt]:
    """按弧长等距重采样为 n 点（去掉书写速度/采样密度差异）。"""
    if len(pts) < 2:
        return list(pts)
    d = [0.0]
    for i in range(1, len(pts)):
        d.append(d[-1] + math.dist(pts[i], pts[i - 1]))
    total = d[-1]
    if total <= 1e-9:
        return [pts[0]] * n
    step = total / (n - 1)
    out: list[Pt] = []
    j = 0
    for i in range(n):
        target = i * step
        while j < len(d) - 1 and d[j + 1] < target:
            j += 1
        if j >= len(pts) - 1:
            out.append(pts[-1])
            continue
        seg = d[j + 1] - d[j]
        t = 0.0 if seg <= 1e-9 else (target - d[j]) / seg
        x = pts[j][0] + t * (pts[j + 1][0] - pts[j][0])
        y = pts[j][1] + t * (pts[j + 1][1] - pts[j][1])
        out.append((x, y))
    return out


def turning(pts: list[Pt]) -> list[float]:
    """每个内部点的**有符号**转角（度）：入向量→出向量的夹角，左正右负。"""
    ang: list[float] = []
    for i in range(1, len(pts) - 1):
        ax, ay = pts[i][0] - pts[i - 1][0], pts[i][1] - pts[i - 1][1]
        bx, by = pts[i + 1][0] - pts[i][0], pts[i + 1][1] - pts[i][1]
        cross = ax * by - ay * bx
        dot = ax * bx + ay * by
        ang.append(math.degrees(math.atan2(cross, dot)))
    return ang


def total_abs_turning(turn: list[float]) -> float:
    return sum(abs(a) for a in turn)


def corners(turn: list[float], min_deg: float, min_gap: int) -> list[int]:
    """角点 = |转角| 局部极大且超过 min_deg 的采样点，NMS 间距 min_gap。

    重采样后一个尖角的转角会摊到相邻几个采样点上，故用 min_gap 抑制重复计数。
    """
    n = len(turn)
    idx: list[int] = []
    for i in range(n):
        if abs(turn[i]) < min_deg:
            continue
        lo, hi = max(0, i - min_gap), min(n, i + min_gap + 1)
        if abs(turn[i]) >= max(abs(turn[k]) for k in range(lo, hi)) - 1e-9:
            if not idx or (i - idx[-1]) >= min_gap:
                idx.append(i)
    return idx


def corner_turn_sum(turn: list[float], idx: list[int], half: int) -> float:
    """角点强度合计：每个角点 ±half 采样窗内的转角绝对值之和（把摊开的尖角收回）。"""
    n = len(turn)
    used: set[int] = set()
    s = 0.0
    for c in idx:
        for k in range(max(0, c - half), min(n, c + half + 1)):
            if k not in used:
                s += abs(turn[k])
                used.add(k)
    return s


def _seg_cross(a: Pt, b: Pt, c: Pt, d: Pt) -> bool:
    def ccw(p: Pt, q: Pt, r: Pt) -> float:
        return (r[1] - p[1]) * (q[0] - p[0]) - (q[1] - p[1]) * (r[0] - p[0])
    return ((ccw(c, d, a) > 0) != (ccw(c, d, b) > 0)) and \
           ((ccw(a, b, c) > 0) != (ccw(a, b, d) > 0))


def self_intersections(pts: list[Pt], n: int = 48) -> int:
    """路径自相交次数（重采样到 n 段后数非相邻线段的交点）。

    五角星(pentagram)的核心不变量：一笔画有 5 个自相交；圆/blob/直线为 0。
    真机手绘星外廓圆钝、尖角不可靠，自相交比"5 个尖角"稳得多。
    """
    rs = resample(pts, n)
    m = len(rs) - 1
    c = 0
    for i in range(m):
        for j in range(i + 2, m):
            if i == 0 and j == m - 1:      # 首尾线段相邻，跳过
                continue
            if _seg_cross(rs[i], rs[i + 1], rs[j], rs[j + 1]):
                c += 1
    return c


def closure(pts: list[Pt]) -> float:
    """闭合度 = 首尾距离 / 尺寸对角线。星形一笔画会回到起点附近（值小）。"""
    d = math.dist(pts[0], pts[-1])
    sz = size_diag(pts)
    return d / sz if sz > 1e-6 else 1e9
