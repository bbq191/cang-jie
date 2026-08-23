"""★ 检测器：专用笔色 + 星形几何（真机对账后的设计）。

真机《缺失功能》页对账结论（见 README）：
- 真手绘星是**多笔叠加、外廓圆钝、内部自相交**的松散手势，不是干净的尖角五角星；
  "5 个尖角 + 闭合"的合成阈值对它全漏。
- 满页黑色手写里，草书汉字也有 4~11 个自相交 + 紧致 → **纯几何黑对黑每页约 8 个假阳，不可用**。
- 定案（用户拍板）：**专用笔色画星**——检测 = 该笔色 + 星形几何双条件。颜色是主判别，
  几何放宽到能容忍圆钝松散星（紧致 + 自相交/尖角，不强求闭合）。

几何判据（在指定笔色内才评估）：
  紧致(长宽比≈1) + 尺寸在窗内 + 自相交 ≥ si_min
**自相交**是核心——星之所以为星就在于笔画交叉：pentagram 恒 5 个、真机松散星 10~20 个，
而圆/方框/对勾/正常字母是 0。合成标定：clean pentagram=5、圆钝星≥5、真机星 10~20。
（尖角计数对圆钝真机星不可靠，已弃用；角点特征仅留作诊断。）
"""
from __future__ import annotations

from dataclasses import dataclass

import geometry as g

Pt = tuple[float, float]


@dataclass
class StarConfig:
    n_resample: int = 96
    smooth: int = 2
    corner_min_deg: float = 40.0
    corner_gap: int = 4
    corner_half: int = 3
    si_resample: int = 48
    # 判定阈值
    aspect_max: float = 1.9
    size_min: float = 40.0
    size_max: float = 400.0
    self_int_min: int = 5          # 自相交下限（pentagram 不变量 = 5）
    # 颜色门控：None=不门控（黑对黑，每页约 8 假阳，仅调试用）；
    # 生产必须设为专用笔色集合，如 {"RED"} 或 {"BLUE"}。
    todo_colors: set[str] | None = None


@dataclass
class Features:
    size: float
    aspect: float
    closure: float
    corners: int
    cts: float
    conc: float
    self_int: int


def _smooth(pts: list[Pt], half: int) -> list[Pt]:
    if half <= 0 or len(pts) < 2 * half + 1:
        return pts
    out, n = [], len(pts)
    for i in range(n):
        lo, hi = max(0, i - half), min(n, i + half + 1)
        out.append((sum(p[0] for p in pts[lo:hi]) / (hi - lo),
                    sum(p[1] for p in pts[lo:hi]) / (hi - lo)))
    return out


def extract(pts: list[Pt], cfg: StarConfig = StarConfig()) -> Features:
    size, asp, clo = g.size_diag(pts), g.aspect(pts), g.closure(pts)
    rs = _smooth(g.resample(pts, cfg.n_resample), cfg.smooth)
    turn = g.turning(rs)
    ci = g.corners(turn, cfg.corner_min_deg, cfg.corner_gap)
    tot = g.total_abs_turning(turn)
    cts = g.corner_turn_sum(turn, ci, cfg.corner_half)
    conc = cts / tot if tot > 1e-6 else 0.0
    si = g.self_intersections(pts, cfg.si_resample)
    return Features(size, asp, clo, len(ci), cts, conc, si)


def _geometry_star(f: Features, cfg: StarConfig) -> bool:
    return (cfg.size_min <= f.size <= cfg.size_max
            and f.aspect <= cfg.aspect_max
            and f.self_int >= cfg.self_int_min)


def is_star(pts: list[Pt], cfg: StarConfig = StarConfig(),
            color: str | None = None) -> tuple[bool, Features]:
    if cfg.todo_colors is not None and (color or "") not in cfg.todo_colors:
        return False, extract(pts, cfg)
    f = extract(pts, cfg)
    return _geometry_star(f, cfg), f
