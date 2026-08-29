"""合成笔划：正例（星）与负例（对勾/括号/圆/方框/下划线/字母/涂鸦），用于差分测试。

真机手绘 ★ 样本到手前，用理想形状 + 抖动/旋转/缩放/平移来标定特征阈值与验证分类器。
所有生成器返回 list[(x,y)]，单位任意（检测器只看比例与角度）。
"""
from __future__ import annotations

import math

Pt = tuple[float, float]


def _xform(pts: list[Pt], scale: float, rot_deg: float, tx: float, ty: float,
           jitter: float, rng) -> list[Pt]:
    c, s = math.cos(math.radians(rot_deg)), math.sin(math.radians(rot_deg))
    out = []
    for x, y in pts:
        x, y = x * scale, y * scale
        rx, ry = x * c - y * s, x * s + y * c
        if jitter:
            rx += rng.gauss(0, jitter)
            ry += rng.gauss(0, jitter)
        out.append((rx + tx, ry + ty))
    return out


def _densify(verts: list[Pt], per_seg: int) -> list[Pt]:
    """在多边形顶点之间插值，模拟连续笔迹采样。"""
    out: list[Pt] = []
    for i in range(len(verts) - 1):
        a, b = verts[i], verts[i + 1]
        for k in range(per_seg):
            t = k / per_seg
            out.append((a[0] + t * (b[0] - a[0]), a[1] + t * (b[1] - a[1])))
    out.append(verts[-1])
    return out


# ---- 正例：星 ----

def pentagram(closed: bool = True) -> list[Pt]:
    """一笔画五角星（{5/2} 星形多边形，自相交）。"""
    outer = [(math.cos(math.radians(90 + 72 * k)), math.sin(math.radians(90 + 72 * k)))
             for k in range(5)]
    order = [0, 2, 4, 1, 3]
    verts = [outer[i] for i in order]
    if closed:
        verts.append(outer[order[0]])
    return _densify(verts, 10)


def rounded_star() -> list[Pt]:
    """圆钝五角星：一笔画 pentagram 但每段外拱成弧（建模真机松散手绘星）。

    外廓变圆、五个尖角变软，但**内部 5 个自相交不变**——这是检测的核心不变量。
    """
    outer = [(math.cos(math.radians(90 + 72 * k)), math.sin(math.radians(90 + 72 * k)))
             for k in range(5)]
    order = [0, 2, 4, 1, 3, 0]
    verts = [outer[i] for i in order]
    pts: list[Pt] = []
    for i in range(len(verts) - 1):
        a, b = verts[i], verts[i + 1]
        mx, my = (a[0] + b[0]) / 2, (a[1] + b[1]) / 2
        cx, cy = mx * 1.35, my * 1.35          # 中点外推 → 段外拱
        for t in [j / 12 for j in range(12)]:  # 二次贝塞尔
            u = 1 - t
            x = u * u * a[0] + 2 * u * t * cx + t * t * b[0]
            y = u * u * a[1] + 2 * u * t * cy + t * t * b[1]
            pts.append((x, y))
    pts.append(verts[-1])
    return pts


# ---- 负例 ----

def checkmark() -> list[Pt]:
    return _densify([(-0.5, 0.1), (-0.1, -0.5), (0.7, 0.6)], 14)


def bracket() -> list[Pt]:
    return _densify([(0.3, -0.8), (-0.3, -0.8), (-0.3, 0.8), (0.3, 0.8)], 12)


def circle() -> list[Pt]:
    return [(math.cos(math.radians(a)), math.sin(math.radians(a)))
            for a in range(0, 361, 4)]


def box() -> list[Pt]:
    return _densify([(-1, -1), (1, -1), (1, 1), (-1, 1), (-1, -1)], 12)


def underline() -> list[Pt]:
    return _densify([(-1, 0), (1, 0)], 40)


def letter_a() -> list[Pt]:
    # 单笔近似大写 A：左斜↑ → 右斜↓ → 横梁止于右腿内侧（不穿腿 → 近 0 自相交）
    return _densify([(-0.8, -1), (0, 1), (0.8, -1), (0.35, -0.13)], 12)


def vee() -> list[Pt]:
    return _densify([(-0.8, 1), (0, -1), (0.8, 1)], 16)


def scribble(rng, n: int = 60) -> list[Pt]:
    """紧致涂鸦：随机游走但转角平滑（最危险的假阳）。"""
    x = y = 0.0
    ang = rng.uniform(0, 360)
    pts = [(x, y)]
    for _ in range(n):
        ang += rng.gauss(0, 35)
        x += math.cos(math.radians(ang)) * 0.12
        y += math.sin(math.radians(ang)) * 0.12
        pts.append((x, y))
    return pts


POSITIVES = {"pentagram": lambda rng: pentagram(True),
             "rounded_star": lambda rng: rounded_star()}
NEGATIVES = {"checkmark": lambda rng: checkmark(),
             "bracket": lambda rng: bracket(),
             "circle": lambda rng: circle(),
             "box": lambda rng: box(),
             "underline": lambda rng: underline(),
             "letter_a": lambda rng: letter_a(),
             "vee": lambda rng: vee(),
             "scribble": lambda rng: scribble(rng)}


def instance(base: list[Pt], rng, scale=40.0, rot=0.0, tx=0.0, ty=0.0, jitter=0.0):
    return _xform(base, scale, rot, tx, ty, jitter, rng)
