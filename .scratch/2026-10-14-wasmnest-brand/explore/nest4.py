#!/usr/bin/env python3
"""探索：真正的织法——多股双向螺旋 + 按「前后深度」决定压/被压。

前三版共同的毛病：压/被压是「按交叉顺序交替」拍脑袋定的，织出来像缠乱了的弹簧。
真实织物的上下关系由**三维深度**决定：把棒料当圆柱，股线位置
  P = 轴 + R·(n·cosθ + ẑ·sinθ)      （n = 画内法线，ẑ = 朝向观察者）
  → 画内偏移 = R·cosθ（就是 winding 的 offset）
  → 朝向观察者的深度 = R·sinθ
交叉处谁的 sinθ 大谁在前面，另一股被压断（挖缝）。这样每一股的压/被压
随它绕行自动交替，织出来是布，不是缠线团。

配合「多股」：每根棒料 3 股右手螺旋 + 3 股左手螺旋（节距各自微差、相位错开），
股线以浅角沿棒料走 → 读作藤条/细枝束，交叉成细长菱形 = 镂空。

用法：python3 nest4.py > nest4.svg && rsvg-convert -z 1 nest4.svg -o nest4.png
"""
import math

from nest2 import (INK, INK_2, PAPER, EMBER, DIM, VERTS, rods, inside, Rng,
                   split_run, arclen, seg_cross, cut_spans, poly, rod_solid, sheet)


def strand(rod, line, R, pitch, dirn, phase, seed, chaos=0.30, samples_per_turn=52,
           whip=2.0, whip_tight=0.5, drift=0.35, end_chaos=0.25, end_amp=0.55):
    """一股缠绕线：返回 (点列, 该点处的 θ 列)。θ 用于算前后深度。"""
    A, u, n, L = rod["A"], rod["u"], rod["n"], rod["L"]
    rng = Rng(seed * 7919 + rod["i"] * 104729 + int(phase * 1000) + int(pitch * 31))
    n_turn = max(1, int(round(L / pitch)))
    k_whip = max(1, min(int(round(whip)), max(1, n_turn // 3)))
    steps = []
    for k in range(n_turn + 2):
        w = whip_tight if (k < k_whip or k >= n_turn + 2 - k_whip) else 1.0
        d = 1.0 + drift * ((k + 0.5) / (n_turn + 2) - 0.5)
        steps.append(w * d * (1.0 + chaos * rng.rng(-0.24, 0.38)))
    tot = sum(steps)
    marks = [0.0]
    acc = 0.0
    for st in steps:
        acc += st
        marks.append(acc / tot)
    amp = [1.0 + chaos * rng.rng(-0.06, 0.06) for _ in range(n_turn + 3)]
    phj = [chaos * rng.rng(-0.22, 0.22) for _ in range(n_turn + 3)]
    wob = [chaos * rng.rng(-0.14, 0.14) for _ in range(n_turn + 3)]
    wph = [chaos * rng.rng(0, 2 * math.pi) for _ in range(n_turn + 3)]

    def tidy(s):
        """端头（缠扎处）降噪：中段保留错乱，两端规整——扎紧的绳头是整齐的。"""
        return end_chaos + (1.0 - end_chaos) * min(1.0, s * n_turn / 1.4, (1.0 - s) * n_turn / 1.4)
    Reff = R - line / 2.0
    total = max(48, int(n_turn * samples_per_turn))

    def seg_of(s):
        k = 0
        while k < n_turn + 1 and marks[k + 1] < s:
            k += 1
        k = min(k, n_turn)
        span = (marks[k + 1] - marks[k]) or 1.0
        return k, (s - marks[k]) / span

    def theta_at(s):
        k, t = seg_of(s)
        w_ = tidy(s) * ((wob[k] * (1 - t) + wob[k + 1] * t)
                        + (phj[k] * (1 - t) + phj[k + 1] * t)
                        + 0.05 * chaos * math.sin(2 * math.pi * s * 0.9 + wph[k]))
        return 2 * math.pi * dirn * (k + t) + phase + w_

    pts, ths = [], []
    for k in range(total + 1):
        s = k / total
        kk, tt = seg_of(s)
        ampv = Reff * (amp[kk] * (1 - tt) + amp[kk + 1] * tt)
        # 末端收到 end_amp：最后半圈往中轴收，端头不会甩出 V 形钩
        taper = min(1.0, end_amp + (1.0 - end_amp) * min(1.0, s * n_turn / 0.8,
                                                       (1.0 - s) * n_turn / 0.8))
        th = theta_at(s)
        off = ampv * math.cos(th) * taper
        pts.append((A[0] + u[0] * L * s + n[0] * off, A[1] + u[1] * L * s + n[1] * off))
        ths.append(th)
    return pts, ths


def split_pair(pts, ths, keep):
    """按 keep(点) 同时切分点列与 θ 列（保证两者下标一一对应）。"""
    rp, rt, cp, ct = [], [], [], []
    for p, t in zip(pts, ths):
        if keep(p):
            cp.append(p)
            ct.append(t)
        else:
            if len(cp) > 1:
                rp.append(cp)
                rt.append(ct)
            cp, ct = [], []
    if len(cp) > 1:
        rp.append(cp)
        rt.append(ct)
    return rp, rt


def interp_theta(pts, ths, arc):
    sl = arclen(pts)
    for i in range(len(sl) - 1):
        if sl[i] <= arc <= sl[i + 1]:
            span = (sl[i + 1] - sl[i]) or 1.0
            t = (arc - sl[i]) / span
            return ths[i] + (ths[i + 1] - ths[i]) * t
    return ths[-1]


def weave_cuts(strands, min_sin=0.25, min_depth=0.15):
    """strands = [(sid, runs, run_thetas)]；返回 {(sid, run_idx): [(run_idx, arc, ang)]}。

    交叉处谁在后（sinθ 小）谁被挖缝；近相切（深度差过小或夹角过小）跳过。
    """
    cuts = {}
    for a in range(len(strands)):
        for b in range(a + 1, len(strands)):
            sid_a, ra, ta = strands[a]
            sid_b, rb, tb = strands[b]
            for ia, pa in enumerate(ra):
                sa = arclen(pa)
                for ib, pb in enumerate(rb):
                    sb = arclen(pb)
                    for i in range(len(pa) - 1):
                        for j in range(len(pb) - 1):
                            hit = seg_cross(pa[i], pa[i + 1], pb[j], pb[j + 1])
                            if not hit:
                                continue
                            t, ss, ang = hit
                            if math.sin(ang) < min_sin:
                                continue
                            arc_a = sa[i] + (sa[i + 1] - sa[i]) * t
                            arc_b = sb[j] + (sb[j + 1] - sb[j]) * ss
                            da = math.sin(interp_theta(pa, ta[ia], arc_a))
                            db = math.sin(interp_theta(pb, tb[ib], arc_b))
                            if abs(da - db) < min_depth:
                                continue
                            if da < db:
                                cuts.setdefault((sid_a, ia), []).append((ia, arc_a, ang))
                            else:
                                cuts.setdefault((sid_b, ib), []).append((ib, arc_b, ang))
    return cuts


def build(R=7.9, pitch=20.0, line=2.1, seam=1.6, gap=1.5, chaos=0.30,
          n_down=3, n_up=3, ember_rod=None, solid=False, whip=2.0, whip_tight=0.5,
          drift=0.35, spread=0.17, verts=VERTS, min_sin=0.25, min_depth=0.15,
          R2=None, end_chaos=0.25, end_amp=0.55):
    parts = []
    for rod in rods(R, seam, verts):
        rr = R if (R2 is None or rod["i"] in (0, 3)) else R2
        col = EMBER if ember_rod == rod["i"] + 1 else PAPER
        if solid:
            parts.append(rod_solid(rod, rr, col))
            continue
        keep = (lambda p, rod=rod: (rod["i"] == 0 or inside(rod["cut0"], p, rod["mid"]))
                and (rod["i"] == 3 or inside(rod["cut1"], p, rod["mid"])))
        strands, art = [], []
        sid = 0
        for k in range(n_down):
            pt, th = strand(rod, line, rr, pitch * (1 + 0.10 * math.sin(k * 2.4)),
                            1.0, 2 * math.pi * k / n_down, 3 + k, chaos,
                            whip=whip, whip_tight=whip_tight, drift=drift,
                            end_chaos=end_chaos, end_amp=end_amp)
            rp, rt = split_pair(pt, th, keep)
            strands.append((sid, rp, rt))
            sid += 1
        for k in range(n_up):
            pt, th = strand(rod, line, rr, pitch * (1 + 0.10 * math.sin(k * 1.7 + 1.1)),
                            -1.0, 2 * math.pi * k / n_up + spread, 11 + k, chaos,
                            whip=whip, whip_tight=whip_tight, drift=-drift,
                            end_chaos=end_chaos, end_amp=end_amp)
            rp, rt = split_pair(pt, th, keep)
            strands.append((sid, rp, rt))
            sid += 1
        cuts = weave_cuts(strands, min_sin=min_sin, min_depth=min_depth)
        for s_id, runs, _ in strands:
            for ri, run in enumerate(runs):
                cs = cuts.get((s_id, ri), [])
                for piece in cut_spans([run], cs, line, gap):
                    parts.append(poly(piece, line, col))
    return "".join(parts)


CASES = [
    ("A 6股 距20 线2.1 棒7.9 乱0.30",
     build(R=7.9, pitch=20.0, line=2.1, n_down=3, n_up=3, chaos=0.30)),
    ("B 6股 距26 线2.1 棒7.9 乱0.30",
     build(R=7.9, pitch=26.0, line=2.1, n_down=3, n_up=3, chaos=0.30)),
    ("C 8股 距20 线1.9 棒8.0 乱0.35",
     build(R=8.0, pitch=20.0, line=1.9, n_down=4, n_up=4, chaos=0.35)),
    ("D 4股 距20 线2.4 棒7.9 乱0.25",
     build(R=7.9, pitch=20.0, line=2.4, n_down=2, n_up=2, chaos=0.25)),
    ("E 6股 距16 线2.1 棒8.2 乱0.45",
     build(R=8.2, pitch=16.0, line=2.1, n_down=3, n_up=3, chaos=0.45)),
    ("F 6股 距20 线2.1 棒7.9 + 琥珀1",
     build(R=7.9, pitch=20.0, line=2.1, n_down=3, n_up=3, chaos=0.30, ember_rod=1)),
    ("G 实心声（≤24px 档）",
     build(R=7.9, pitch=20.0, line=2.1, solid=True)),
    ("H 6股 距20 线2.1 棒7.9 乱0.15（最规整）",
     build(R=7.9, pitch=20.0, line=2.1, n_down=3, n_up=3, chaos=0.15)),
]

if __name__ == "__main__":
    sheet(CASES, "WasmNest — woven sleeve (strands both ways, depth decides over/under)",
          "3 right-hand + 3 left-hand helical strands per rod · behind strand cut by sin θ · pitch 16–26 · 96/64/48/32/24/16")
