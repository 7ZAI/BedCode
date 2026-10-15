#!/usr/bin/env python3
"""探索：把缠绕角放平（节距 ≫ 棒料周长），让线沿棒料走 = 编绳/藤条感，而不是毛毛虫。

判据：沿棒料方向的线（浅螺旋）读作「材料」，垂直于轴线的密圈读作「腿」。
本表扫 pitch 12→24（棒料周长 ≈ 2πR ≈ 48），外加节距漂移 drift。
用法：python3 nest3.py > nest3.svg && rsvg-convert -z 1 nest3.svg -o nest3.png
"""
import math

from nest2 import build, sheet

CASES = [
    ("A 距12 线2.2 棒7.8 乱0.30 漂0.5 扎2",
     build(R=7.8, pitch=12.0, line=2.2, seam=1.6, gap=1.4, chaos=0.30, whip=2.0, drift=0.5)),
    ("B 距15 线2.2 棒7.8 乱0.30 漂0.5 扎2",
     build(R=7.8, pitch=15.0, line=2.2, seam=1.6, gap=1.4, chaos=0.30, whip=2.0, drift=0.5)),
    ("C 距18 线2.4 棒7.8 乱0.35 漂0.6 扎2",
     build(R=7.8, pitch=18.0, line=2.4, seam=1.6, gap=1.5, chaos=0.35, whip=2.0, drift=0.6)),
    ("D 距22 线2.4 棒7.8 乱0.40 漂0.6 扎2",
     build(R=7.8, pitch=22.0, line=2.4, seam=1.6, gap=1.5, chaos=0.40, whip=2.0, drift=0.6)),
    ("E 距15 线2.6 棒8.2 乱0.25 漂0.4 扎2",
     build(R=8.2, pitch=15.0, line=2.6, seam=1.7, gap=1.5, chaos=0.25, whip=2.0, drift=0.4)),
    ("F 距18 线2.0 棒7.4 乱0.35 漂0.5 扎2.5",
     build(R=7.4, pitch=18.0, line=2.0, seam=1.5, gap=1.4, chaos=0.35, whip=2.5, drift=0.5)),
    ("G 距15 线2.2 棒7.8 一程（不反向）",
     build(R=7.8, pitch=15.0, line=2.2, seam=1.6, gap=1.4, chaos=0.3, whip=2.0, drift=0.5)),
    ("H 距12 线2.0 棒8.6 乱0.45 漂0.8 扎2",
     build(R=8.6, pitch=12.0, line=2.0, seam=1.7, gap=1.4, chaos=0.45, whip=2.0, drift=0.8)),
]

sheet(CASES, "WasmNest — shallow winding angle (pitch 12–22, rod girth ≈ 48)",
      "strands run ALONG the rod (braid/rattan) instead of around it (caterpillar) · drift = pitch varies down the rod")
