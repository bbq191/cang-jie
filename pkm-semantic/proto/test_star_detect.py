"""★ 检测差分测试（自相交判据 + 颜色门控 + 真机 fixture 回归）。

设计见 star_detect 顶注：星 = 专用笔色 + 紧致 + 自相交≥5。颜色是主判别，几何放宽。
真机 fixture=《缺失功能》第2页（testdata/），黑色手写笔记 + 底部黑色手绘星，
坐实"黑对黑纯几何不可用、必须颜色门控"。
"""
import glob
import os
import random

import pytest

import star_detect as sd
import synth
from rm_strokes import read_strokes
from star_scan import _cluster, scan_document

CFG = sd.StarConfig()                        # 几何模式（无颜色门控，仅测试/调试）
HERE = os.path.dirname(__file__)
FIX_UUID = "9f71a39c-ca3f-49fb-828a-ef99f62596e5"
FIX_PAGE = os.path.join(HERE, "testdata", FIX_UUID,
                        "52638c34-7825-4039-a79b-c0a3ed5ffb46.rm")


def _instances(base_fn, n, jitter_pct):
    rng = random.Random(1234)
    for _ in range(n):
        scale = rng.uniform(20, 120)
        yield synth.instance(
            base_fn(rng), rng, scale=scale, rot=rng.uniform(0, 360),
            tx=rng.uniform(-400, 400), ty=rng.uniform(-400, 400),
            jitter=scale * jitter_pct)


# ---- 合成差分 ----

@pytest.mark.parametrize("name", list(synth.POSITIVES))
def test_positives_detected(name):
    hits = sum(sd.is_star(p, CFG)[0]
               for p in _instances(synth.POSITIVES[name], 120, 0.025))
    assert hits / 120 >= 0.90, f"{name} 检出率过低: {hits}/120"


@pytest.mark.parametrize("name", list(synth.NEGATIVES))
def test_negatives_rejected(name):
    fp = sum(sd.is_star(p, CFG)[0]
             for p in _instances(synth.NEGATIVES[name], 120, 0.025))
    assert fp / 120 <= 0.05, f"{name} 误检率过高: {fp}/120"


def test_self_intersection_invariant():
    """核心不变量：clean pentagram 恒 5 个自相交，圆 0 个。"""
    import geometry as g
    rng = random.Random(0)
    penta = synth.instance(synth.pentagram(True), rng, scale=60)
    circ = synth.instance(synth.circle(), rng, scale=60)
    assert g.self_intersections(penta) == 5
    assert g.self_intersections(circ) == 0


# ---- 颜色门控 ----

def test_color_gating():
    cfg = sd.StarConfig(todo_colors={"RED"})
    rng = random.Random(0)
    star = synth.instance(synth.rounded_star(), rng, scale=70)
    assert sd.is_star(star, cfg, color="RED")[0] is True
    assert sd.is_star(star, cfg, color="BLACK")[0] is False   # 非专用色被拒
    assert sd.is_star(star, sd.StarConfig(), color="BLACK")[0] is True  # 不门控则任意色


def test_size_gate_rejects_tiny_and_huge():
    rng = random.Random(0)
    assert sd.is_star(synth.instance(synth.pentagram(True), rng, scale=3), CFG)[0] is False
    assert sd.is_star(synth.instance(synth.pentagram(True), rng, scale=5000), CFG)[0] is False


def test_enum_name_intenum_regression():
    """PenColor 是 IntEnum：Py3.11+ 的 str() 返数字，归一必须走 .name。"""
    from rmscene import scene_items as si

    from rm_strokes import _enum_name
    assert _enum_name(si.PenColor.RED) == "RED"
    assert _enum_name(si.PenColor.MAGENTA) == "MAGENTA"
    assert _enum_name(si.Pen.FINELINER_1) == "FINELINER_1"
    assert _enum_name(None) == ""


# ---- 真机 fixture 回归 ----

def test_real_device_stars_detected_by_geometry():
    """真机《缺失功能》底部黑色手绘星：几何(自相交)至少检出 2 个星簇。"""
    if not os.path.exists(FIX_PAGE):
        pytest.skip("缺 fixture")
    ss = read_strokes(FIX_PAGE)
    bottom = [s for i, s in enumerate(ss) if i >= 181]   # 底部刻意画的形状区
    hits = sum(sd.is_star([p for s in grp for p in s.points], CFG)[0]
               for grp in _cluster(bottom, gap=25))
    assert hits >= 2, f"真机星簇检出过少: {hits}"


def test_real_device_red_stars_color_plus_shape():
    """真机页：用户把星画成红色(45 红笔划=星+红干扰，155 黑笔划=笔记)。
    RED 门控 + 星形几何 检出 ≥5 个红星，正确忽略红对勾/红方框等干扰。"""
    if not os.path.exists(FIX_PAGE):
        pytest.skip("缺 fixture")
    cfg = sd.StarConfig(todo_colors={"RED"})
    r = scan_document(FIX_UUID, os.path.join(HERE, "testdata"), cfg, cluster_gap=25)
    assert r is not None and r["hits"][0]["count"] >= 5
    assert r["hits"][0]["colors"] == ["RED"]


def test_unused_color_gate_empty():
    """未使用的笔色门控 → 空（GREEN/BLUE 页上没有 → 证明门控真的在筛色）。"""
    if not os.path.exists(FIX_PAGE):
        pytest.skip("缺 fixture")
    assert scan_document(FIX_UUID, os.path.join(HERE, "testdata"),
                         sd.StarConfig(todo_colors={"GREEN"})) is None


def test_real_samples_no_crash():
    """所有 fixture .rm 读取不崩（含 rmscene 报 newer-format 警告的页）。"""
    for rm in glob.glob(os.path.join(HERE, "testdata", FIX_UUID, "*.rm")):
        for s in read_strokes(rm):
            sd.is_star(s.points, CFG)
