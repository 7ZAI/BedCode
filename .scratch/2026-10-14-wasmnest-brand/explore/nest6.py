#!/usr/bin/env python3
"""探索：定稿候选——中脊放宽 + 内棒稍细 + 端头规整（扎紧的绳头是整齐的）。"""
from nest2 import sheet, VERTS
from nest4 import build

WIDE = [(12.0, 20.0), (28.5, 74.0), (50.0, 47.0), (71.5, 74.0), (88.0, 20.0)]

CASES = [
    ("A 2+2股 距24 线2.3 棒7.8/7.0 宽脊",
     build(R=7.8, R2=7.0, pitch=24.0, line=2.3, n_down=2, n_up=2, chaos=0.30, verts=WIDE, seam=2.2, gap=1.6)),
    ("B 3+3股 距26 线2.0 棒7.8/7.0 宽脊",
     build(R=7.8, R2=7.0, pitch=26.0, line=2.0, n_down=3, n_up=3, chaos=0.30, verts=WIDE, seam=2.2, gap=1.6)),
    ("C 2+2股 距30 线2.3 棒7.8/7.0 宽脊 距大",
     build(R=7.8, R2=7.0, pitch=30.0, line=2.3, n_down=2, n_up=2, chaos=0.35, verts=WIDE, seam=2.2, gap=1.6)),
    ("D 3+3股 距22 线2.2 棒8.0/7.2 窄脊（原骨架）",
     build(R=8.0, R2=7.2, pitch=22.0, line=2.2, n_down=3, n_up=3, chaos=0.30, verts=VERTS, seam=2.0, gap=1.6)),
    ("E 2+2股 距26 线2.5 棒8.0/7.2 宽脊 端更收",
     build(R=8.0, R2=7.2, pitch=26.0, line=2.5, n_down=2, n_up=2, chaos=0.25, verts=WIDE, seam=2.2, gap=1.7, end_amp=0.42)),
    ("F 3+3股 距30 线2.0 棒7.8/7.0 宽脊 端0.45",
     build(R=7.8, R2=7.0, pitch=30.0, line=2.0, n_down=3, n_up=3, chaos=0.35, verts=WIDE, seam=2.2, gap=1.6, end_amp=0.45)),
    ("G 实心声（≤24px）宽脊",
     build(R=7.8, R2=7.0, pitch=24.0, line=2.3, solid=True, verts=WIDE, seam=2.2)),
    ("H 2+2股 距24 线2.3 棒7.8/7.0 宽脊 + 琥珀1",
     build(R=7.8, R2=7.0, pitch=24.0, line=2.3, n_down=2, n_up=2, chaos=0.30, verts=WIDE, seam=2.2, gap=1.6, ember_rod=1)),
]

if __name__ == "__main__":
    sheet(CASES, "WasmNest — final candidates (wide middle, thinner inner rods, tidy whipped ends)",
          "woven sleeve W · 2–3 strands each way · behind strand cut by sin θ · 96/64/48/32/24/16")
