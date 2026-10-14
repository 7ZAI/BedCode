# WasmNest 品牌工程（工作目录）

品牌名称：**WasmNest**（v1 的 WasmApp / BedCode 命名到此为止）。
设计结论、构造原理、使用规则与落地清单都在 **[`brand-spec.md`](./brand-spec.md)**，先读它。

## 目录

| 路径 | 内容 |
|---|---|
| `brand-spec.md` | 品牌规范（策略 / 构造 / 资产 / 规则 / 色板 / 字体 / 落地清单） |
| `build-assets.py` | **标志几何真源** + 全部资产生成器（`MASTER` 常量在文件顶部） |
| `brand-board.py` | 规范板排版（纯矢量，引用 `assets/` 里的真源资产） |
| `assets/` | 交付资产（SVG / PNG 阶梯 / `.ico` / `.icns`）+ 转曲字标 |
| `build/board.png` | 规范板 2400×2400（`board.svg` 为矢量源） |
| `explore/` | 探索过程稿（每一版的结构与为什么被否决，见 spec §3） |
| `fonts/` + `fonts.conf` | 转曲与板式渲染用的自持字体（Inter / Inter Display / JetBrains Mono） |

## 重新生成

```bash
# 全部 SVG 资产（含字标转曲缓存）
python3 build-assets.py

# 追加 PNG 阶梯 + wasmnest.ico + wasmnest.icns（需要 rsvg-convert）
python3 build-assets.py --png

# 强制重新转曲字标（需要 rsvg-convert + gs + pdftocairo）
python3 build-assets.py --wordmark

# 规范板（需要 rsvg-convert，字体经 fonts.conf 指定）
python3 brand-board.py > board.svg && rsvg-convert -w 2400 board.svg -o board.png
```

改标志只改 `build-assets.py` 顶部的 `MASTER` / `VERTS`，重跑即可——规范板会跟着变，
因为板子引用的是同一份真源函数与同一批 `assets/` 文件。

## 环境依赖（本机已确认存在）

`rsvg-convert`（librsvg）、`gs`（ghostscript）、`pdftocairo`（poppler）、`ffmpeg`（仅用于我的
目视缩放，不是生成链条的一部分）。**没有** ImageMagick、PIL、fontTools——
`.ico` / `.icns` 由脚本用纯 Python 组装，字标转曲走 rsvg→gs→pdftocairo 绕过 fontTools。
