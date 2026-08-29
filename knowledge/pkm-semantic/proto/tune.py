"""标定：打印各形状特征分布 + 分类命中率，用于设定/复核 is_star 阈值。"""
import random
import statistics as st

import star_detect as sd
import synth

CFG = sd.StarConfig()


def sweep(name, base_fn, positive):
    rng = random.Random(42)
    rows, hits = [], 0
    for _ in range(60):
        rot = rng.uniform(0, 360)
        scale = rng.uniform(20, 120)
        jit = rng.uniform(0, scale * 0.012)   # ~1% 真实笔迹抖动
        pts = synth.instance(base_fn(rng), rng, scale=scale, rot=rot,
                             tx=rng.uniform(-500, 500), ty=rng.uniform(-500, 500),
                             jitter=jit)
        ok, f = sd.is_star(pts, CFG)
        rows.append(f)
        hits += 1 if ok else 0
    def r(attr):
        vs = [getattr(x, attr) for x in rows]
        return f"{min(vs):6.1f}..{max(vs):6.1f}"
    tag = "POS" if positive else "NEG"
    rate = hits / len(rows)
    verdict = "OK" if (positive and rate > 0.9) or (not positive and rate < 0.05) else "**CHECK**"
    print(f"[{tag}] {name:12s} hit={rate:4.0%} {verdict:9s} "
          f"asp {r('aspect')} clo {r('closure')} corn {r('corners')} "
          f"cts {r('cts')} conc {r('conc')}")


print("=== POSITIVES (want hit≈100%) ===")
for n, fn in synth.POSITIVES.items():
    sweep(n, fn, True)
print("=== NEGATIVES (want hit≈0%) ===")
for n, fn in synth.NEGATIVES.items():
    sweep(n, fn, False)
