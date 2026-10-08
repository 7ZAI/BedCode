#!/usr/bin/env python3
"""WasmApp 移动端品牌资产生成器

设计意图：标志真源只有一份（四段 W + 核心菱形，坐标定义在 MASTER_SEGS），
所有移动端变体都从同一组几何派生，避免出现「大致像」的平行版本。
改标志时只改 MASTER_SEGS 与 MASTER_CORE，重跑本脚本即可。

用法：
    python3 build-assets.py            # 只写 SVG
    python3 build-assets.py --png      # 同时导出 PNG（需要 PATH 里有 rsvg-convert）

真源对照：
    ../assets/wasmapp-mark.svg        标志真源（本脚本的 MASTER_* 与它逐值一致）
    ../brand-spec.md §6               品牌色系统
"""

import argparse
import shutil
import subprocess
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
SVG_DIR = HERE / "svg"
PNG_DIR = HERE / "png"

# ============================================================================
# 1. 几何真源（与 ../assets/wasmapp-mark.svg 完全一致，勿单独改动）
# ============================================================================
# 四段分离笔画：两个外斜 + 两个内斜，构成 W。段间留缝 = 隔离边界。
MASTER_SEGS = [
    ("23.2", "33", "36.8", "67"),
    ("39.3", "67.1", "48.7", "46.9"),
    ("51.3", "46.9", "60.7", "67.1"),
    ("63.2", "67", "76.8", "33"),
]
MASTER_STROKE = 13.0
# 核心菱形（「正在运行的那个实例」，全系统唯一的彩色元素）
MASTER_CORE = "M50 36.5 L56.5 43 L50 49.5 L43.5 43 Z"

# 派生量：标志视觉外框与中心（由上面的坐标算出，不手填）
_px = [float(v) for s in MASTER_SEGS for v in (s[0], s[2])]
_py = [float(v) for s in MASTER_SEGS for v in (s[1], s[3])]
MARK_W = (max(_px) + MASTER_STROKE / 2) - (min(_px) - MASTER_STROKE / 2)
MARK_H = (max(_py) + MASTER_STROKE / 2) - (min(_py) - MASTER_STROKE / 2)
MARK_CX = (min(_px) + max(_px)) / 2
MARK_CY = (min(_py) + max(_py)) / 2

# ============================================================================
# 2. 品牌色（brand-spec.md §6 + 移动端使用阶梯）
# ============================================================================
INK = "#08090B"        # 主深底
INK_2 = "#16181C"      # 抬升面 / 图标底板上端
INK_3 = "#101216"      # 小尺寸图标底（更平，避免渐变在 16px 产生噪点）
PAPER = "#F5F7F9"      # 主前景 / 笔画
EMBER = "#FF9E2C"      # 品牌琥珀：深底上的唯一彩色（深底 8.41-8.99:1）
EMBER_700 = "#9F621B"  # 浅底上的琥珀（浅底 4.50-4.83:1，见 brand-spec-mobile.md）
EMBER_800 = "#764914"  # 浅底长文案 / AAA（7.25-7.78:1）

# 圆角：iOS / App Store 方形图标用 22.37%（Apple 超椭圆近视值）
R_IOS = "22.37"
# Android 自适应图标：108dp 画布，安全区为居中 66dp
ADAPTIVE_CANVAS = 108.0
ADAPTIVE_SAFE = 66.0


def mark_group(scale=1.0, cx=None, cy=None, stroke=None, color=PAPER, core=EMBER):
    """标志 <g>：绕标志中心缩放，保持视觉居中。"""
    cx = MARK_CX if cx is None else cx
    cy = MARK_CY if cy is None else cy
    sw = MASTER_STROKE if stroke is None else stroke
    lines = "".join(
        f'<line x1="{a}" y1="{b}" x2="{c}" y2="{d}"/>' for a, b, c, d in MASTER_SEGS
    )
    tx, ty = cx - MARK_CX * scale, cy - MARK_CY * scale
    return (
        f'<g transform="translate({tx:.4f},{ty:.4f}) scale({scale:.4f})">'
        f'<g stroke="{color}" stroke-width="{sw}" stroke-linecap="butt" fill="none">{lines}</g>'
        f'<path d="{MASTER_CORE}" fill="{core}"/></g>'
    )


# 小尺寸折线（合并笔画版）。折线坐标与主标志同源：外轮廓、内部 V 形完全一致。
COMPACT_D = "M23 33 L38 68 L50 47 L62 68 L77 33"
COMPACT_CORE_Y = 43.2      # 核心中心：主标志两条内斜的交点
_ccx = (23, 38, 50, 62, 77)
_ccy = (33, 68, 47, 68, 33)


def compact_mark(stroke, core_size, color=PAPER, core=EMBER):
    """小尺寸专用：笔画合并为单条折线 + 方形核心。

    32px 以下四段接缝必然糊成一片，与其让它糊，不如主动合并：外轮廓与 W 字形和
    主标志完全一致，只是不再分段、核心改为实心方块，并保留琥珀让「活跃实例」的
    语义不丢。

    stroke 与 core_size 都是在 0-100 坐标空间里的值，但渲染时按像素算：
        渲染笔画 = stroke * size / 100
    下面三档是**实测**出来的（2.4/2.9/3.2/4.0/4.5/5/6/13/15/16.6 十档渲染比对，
    按 16px 的墨迹覆盖率与字形可辨性取最优），不是估算：
        16px  16.6 -> 2.66px 笔画（这套标志的可读性下限，笔画必须够粗）
        24px  15.0 -> 3.60px
        32px  13.5 -> 4.32px
    16px 的墨迹覆盖率 48px 像素（占底板 18.8%），与 24/32 视觉重量一致。
    """
    tx = 50 - 50
    ty = 50 - ((min(_ccy) + max(_ccy)) / 2)
    return (
        f'<path d="{COMPACT_D}" fill="none" stroke="{color}" stroke-width="{stroke}" '
        f'stroke-linejoin="miter" stroke-linecap="butt"/>'
        f'<rect x="{50 - core_size / 2:.4f}" y="{COMPACT_CORE_Y - core_size / 2:.4f}" '
        f'width="{core_size}" height="{core_size}" rx="{core_size * 0.2:.2f}" '
        f'fill="{core}"/>'
    )


def tile_gradient(gid="tile"):
    return (
        f'<linearGradient id="{gid}" x1="0" y1="0" x2="0.65" y2="1">'
        f'<stop offset="0" stop-color="{INK_2}"/><stop offset="1" stop-color="{INK}"/>'
        f"</linearGradient>"
    )


def svg(body, w, h, viewbox="0 0 100 100"):
    return (
        f'<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" '
        f'viewBox="{viewbox}">\n{body}\n</svg>\n'
    )


# ============================================================================
# 3. 资产清单
# ============================================================================
def build():
    out = {}

    # ---- 标志（裸标识，透明底）------------------------------------------------
    # 移动端图形标志统一用 62% 占用率的外框：四周留白 = 核心菱形对角线的一半
    _ = MARK_W  # 供文档引用
    out["mark.svg"] = svg(
        mark_group(scale=1.0), 100, 100, "16 26 68 48"
    )
    out["mark-ember.svg"] = svg(
        mark_group(scale=1.0, color=EMBER, core=PAPER), 100, 100, "16 26 68 48"
    )
    out["mark-light.svg"] = svg(
        mark_group(scale=1.0, color=INK_2, core=EMBER_700), 100, 100, "16 26 68 48"
    )

    # ---- iOS / App Store 方形图标（圆角已烘焙）--------------------------------
    # 标志占底板宽度 62%（scale 0.931），比旧版 66.6% 略收，四周留出呼吸
    ios_scale = (0.62 * 100) / MARK_W
    ios = (
        f'<defs>{tile_gradient()}</defs>'
        f'<rect width="100" height="100" rx="{R_IOS}" fill="url(#tile)"/>'
        + mark_group(scale=ios_scale)
    )
    out["app-icon-ios.svg"] = svg(ios, 1024, 1024)

    # 浅色主题方形图标：琥珀在浅底只有 2.01:1，核心换 ember-700（4.83:1）
    out["app-icon-light.svg"] = svg(
        f'<rect width="100" height="100" rx="{R_IOS}" fill="#F5F4F0"/>'
        f'<rect x="0.8" y="0.8" width="98.4" height="98.4" rx="{R_IOS}" fill="none" '
        f'stroke="rgba(38,35,27,0.12)" stroke-width="1.6"/>'
        + mark_group(scale=ios_scale, color=INK_2, core=EMBER_700),
        1024,
        1024,
    )

    # ---- Android 自适应图标 ---------------------------------------------------
    # 前景层不画底板、不切圆角：系统负责遮罩，所以内容必须落在居中 66dp 安全区内。
    # 取标志占可见区（72dp）的 67% => 48dp，等于安全区的 73%。
    adapt_scale = 48.0 / MARK_W
    out["app-icon-adaptive-foreground.svg"] = svg(
        mark_group(
            scale=adapt_scale, cx=ADAPTIVE_CANVAS / 2, cy=ADAPTIVE_CANVAS / 2
        ),
        432,
        432,
        f"0 0 {ADAPTIVE_CANVAS:g} {ADAPTIVE_CANVAS:g}",
    )
    out["app-icon-adaptive-background.svg"] = svg(
        f'<defs>{tile_gradient()}</defs>'
        f'<rect width="{ADAPTIVE_CANVAS:g}" height="{ADAPTIVE_CANVAS:g}" fill="url(#tile)"/>',
        432,
        432,
        f"0 0 {ADAPTIVE_CANVAS:g} {ADAPTIVE_CANVAS:g}",
    )
    # 单色层（Android 13 主题图标）：系统上色，只给轮廓
    out["app-icon-adaptive-monochrome.svg"] = svg(
        mark_group(
            scale=adapt_scale,
            cx=ADAPTIVE_CANVAS / 2,
            cy=ADAPTIVE_CANVAS / 2,
            color="#FFFFFF",
            core="#FFFFFF",
        ),
        432,
        432,
        f"0 0 {ADAPTIVE_CANVAS:g} {ADAPTIVE_CANVAS:g}",
    )

    # ---- 小尺寸专用 -----------------------------------------------------------
    # 16 / 24 / 32：接缝在 32px 以下必然丢失，改用合并笔画 + 方块核心
    # 16px 是这套标志的可读性下限。经 2.4 / 2.9 / 3.2px 三档实测，3.2px 笔画
    # 才能让 W 与琥珀核心同时立住；24px 略收笔画，32px 用回接近标准图标的比例。
    # scale 决定整体大小，stroke 是在 0-100 坐标空间里的笔画宽度：
    # 实际渲染笔画 = stroke * scale（见 brand-spec-mobile.md 的实测表）。
    for size, stroke, core in ((16, 16.6, 7.6), (24, 15.0, 7.6), (32, 13.5, 7.6)):
        out[f"app-icon-compact-{size}.svg"] = svg(
            f'<rect width="100" height="100" rx="{R_IOS}" fill="{INK_3}"/>'
            + compact_mark(stroke=stroke, core_size=core),
            size,
            size,
        )
    # 48：接缝尚可辨认，保留完整标志
    out["app-icon-48.svg"] = svg(
        f'<defs>{tile_gradient()}</defs>'
        f'<rect width="100" height="100" rx="{R_IOS}" fill="url(#tile)"/>'
        + mark_group(scale=0.90),
        48,
        48,
    )

    # ---- 单色降级（打印 / 水印 / 单色位图）------------------------------------
    out["icon-mono-paper.svg"] = svg(
        f'<defs>{tile_gradient()}</defs>'
        f'<rect width="100" height="100" rx="{R_IOS}" fill="url(#tile)"/>'
        + mark_group(scale=0.90, color=PAPER, core=PAPER),
        1024,
        1024,
    )
    out["icon-mono-ink.svg"] = svg(
        f'<rect width="100" height="100" rx="{R_IOS}" fill={PAPER!r}/>'.replace("'", '"')
        + mark_group(scale=0.90, color=INK_2, core=INK_2),
        1024,
        1024,
    )

    # ---- 移动端横向锁定组合（顶栏 / 商店标题栏 / 启动页）-----------------------
    # 移动端字号比桌面锁定组合小，字重仍取 700，字距收紧到 -1.2
    out["lockup-horizontal.svg"] = svg(
        mark_group(scale=0.86, cx=58, cy=60)
        + f'<text x="120" y="80" font-family="Inter, Geist, \'PingFang SC\', sans-serif" '
        f'font-size="54" font-weight="700" letter-spacing="-1.2" fill="{PAPER}">Wasm'
        f'<tspan fill="{EMBER}">App</tspan></text>',
        460,
        120,
        "0 0 460 120",
    )
    # 紧凑锁定组合：只有标志 + 「WasmApp」，用于约 120px 宽的顶栏
    out["lockup-compact.svg"] = svg(
        mark_group(scale=0.74, cx=40, cy=42)
        + f'<text x="88" y="57" font-family="Inter, Geist, \'PingFang SC\', sans-serif" '
        f'font-size="40" font-weight="700" letter-spacing="-1" fill="{PAPER}">Wasm'
        f'<tspan fill="{EMBER}">App</tspan></text>',
        300,
        84,
        "0 0 300 84",
    )

    # ---- 启动页 / 商店角标 ----------------------------------------------------
    out["launch-mark.svg"] = svg(
        mark_group(scale=1.34, cx=50, cy=50), 400, 400, "0 0 100 100"
    )
    out["badge-official.svg"] = svg(
        f'<rect x="1" y="1" width="98" height="98" rx="24" fill="{INK_2}" '
        f'stroke="rgba(245,247,249,0.14)" stroke-width="2"/>'
        + mark_group(scale=0.72, color=PAPER, core=EMBER),
        96,
        96,
    )

    # ---- 图案（背景纹理）------------------------------------------------------
    # 8x8 网格，单元即「一个隔离的实例」；仅一格着琥珀 = 当前活跃实例
    cells = []
    step = 100 / 8
    for r in range(8):
        for c in range(8):
            x, y = c * step + step * 0.22, r * step + step * 0.22
            s = step * 0.56
            if r == 3 and c == 4:
                cells.append(
                    f'<rect x="{x:.2f}" y="{y:.2f}" width="{s:.2f}" height="{s:.2f}" '
                    f'rx="{s*0.22:.2f}" fill="{EMBER}"/>'
                )
            else:
                cells.append(
                    f'<rect x="{x:.2f}" y="{y:.2f}" width="{s:.2f}" height="{s:.2f}" '
                    f'rx="{s*0.22:.2f}" fill="{PAPER}" fill-opacity="0.16"/>'
                )
    out["pattern-grid.svg"] = svg("".join(cells), 400, 400)

    return out


PNG_JOBS = [
    # (源 SVG, 输出名, 宽, 高)。方形图标给相同值；锁定组合必须给真实宽高，
    # 否则会被拉伸成正方形（第一版就踩了这个坑）。
    ("app-icon-ios.svg", "app-icon-1024.png", 1024, 1024),
    ("app-icon-ios.svg", "app-icon-512.png", 512, 512),
    ("app-icon-ios.svg", "app-icon-256.png", 256, 256),
    ("app-icon-48.svg", "app-icon-48.png", 48, 48),
    ("app-icon-compact-32.svg", "app-icon-32.png", 32, 32),
    ("app-icon-compact-24.svg", "app-icon-24.png", 24, 24),
    ("app-icon-compact-16.svg", "app-icon-16.png", 16, 16),
    ("app-icon-light.svg", "app-icon-light-256.png", 256, 256),
    ("mark.svg", "mark-256.png", 256, 256),
    ("lockup-horizontal.svg", "lockup-horizontal.png", 920, 240),
    ("lockup-compact.svg", "lockup-compact.png", 300, 84),
]


def export_png():
    if not shutil.which("rsvg-convert"):
        print("SKIP png: rsvg-convert 不在 PATH", file=sys.stderr)
        return
    PNG_DIR.mkdir(parents=True, exist_ok=True)
    for src, dst, w, h in PNG_JOBS:
        src_path = SVG_DIR / src
        dst_path = PNG_DIR / dst
        cmd = ["rsvg-convert", "-w", str(w), "-h", str(h), str(src_path), "-o", str(dst_path)]
        subprocess.run(cmd, check=True)
        print(f"  png  {dst}  {w}x{h}")


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--png", action="store_true", help="同时导出 PNG")
    args = ap.parse_args()

    SVG_DIR.mkdir(parents=True, exist_ok=True)
    assets = build()
    for name, content in sorted(assets.items()):
        (SVG_DIR / name).write_text(content, encoding="utf-8")
        print(f"  svg  {name}")
    print(f"\n{len(assets)} 个 SVG 写入 {SVG_DIR}")
    print(f"标志视觉框 {MARK_W:.1f} x {MARK_H:.1f}，中心 ({MARK_CX:.1f}, {MARK_CY:.1f})")

    if args.png:
        print("\n导出 PNG:")
        export_png()


if __name__ == "__main__":
    main()
