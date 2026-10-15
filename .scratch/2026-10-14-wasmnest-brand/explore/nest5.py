#!/usr/bin/env python3
"""探索：织法已定，收敛参数——要更透气（股少、距大）、端头更利落、缝更清楚。"""
from nest2 import sheet
from nest4 import build

CASES = [
    ("A 3+3股 距24 线2.0 棒7.6 乱0.30",
     build(R=7.6, pitch=24.0, line=2.0, n_down=3, n_up=3, chaos=0.30)),
    ("B 2+2股 距26 线2.2 棒7.8 乱0.30",
     build(R=7.8, pitch=26.0, line=2.2, n_down=2, n_up=2, chaos=0.30)),
    ("C 3+3股 距30 线2.0 棒7.6 乱0.35",
     build(R=7.6, pitch=30.0, line=2.0, n_down=3, n_up=3, chaos=0.35)),
    ("D 2+2股 距22 线2.4 棒7.5 乱0.25",
     build(R=7.5, pitch=22.0, line=2.4, n_down=2, n_up=2, chaos=0.25)),
    ("E 3+3股 距26 线2.2 棒8.0 缝2.2 乱0.30",
     build(R=8.0, pitch=26.0, line=2.2, n_down=3, n_up=3, chaos=0.30, seam=2.2, gap=1.6)),
    ("F 2+3股 距24 线2.2 棒7.6 乱0.40",
     build(R=7.6, pitch=24.0, line=2.2, n_down=2, n_up=3, chaos=0.40)),
    ("G 实心声（≤24px）",
     build(R=7.6, pitch=24.0, line=2.0, solid=True)),
    ("H 3+3股 距24 线2.0 棒7.6 + 琥珀1",
     build(R=7.6, pitch=24.0, line=2.0, n_down=3, n_up=3, chaos=0.30, ember_rod=1)),
]

if __name__ == "__main__":
    sheet(CASES, "WasmNest — refine: more air (pitch 22–30, 2–3 strands each way)",
          "woven sleeve · behind strand cut by sin θ · 96/64/48/32/24/16")
