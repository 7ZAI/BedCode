# Splash 启动画面复刻与优化

## 任务记录

参照 `bedcode-desktop/src/components/SplashLoading.vue` 复刻一份独立 HTML，
在浏览器中截图对比，并按 `ui-ux-pro-max` 设计系统做一轮优化。

## 产物

| 文件 | 说明 |
|---|---|
| `replica.html` | 原始实现的静态副本（颜色/尺寸/动画时序完全对齐 SplashLoading.vue） |
| `optimized.html` | 第一轮优化：logo 辉光 + boot log 阶段栈 + 段式进度 + 磷光光标 |
| `optimized-v2.html` | 设计系统驱动的第二轮优化：JetBrains Mono + 弱化终端绿 + 底部 vignette + 呼吸 halo + 状态脚注 |
| `shots/` | 各版本在多个动画时刻（400/1200/2200/3000ms）的截图 |

## 视觉差异（replica vs optimized-v2）

### 保持的原始设计语言
- 深色品牌渐变（`#2E2A22 → #0A0907`，来自 icon.svg）
- 终端 Boot 语义：`$ bedcode` 打字机 + 块状光标
- BedCode 品牌名 + 副标语双层结构
- `prefers-reduced-motion` 完全回退
- Teleport / z-index 层级约定（此 HTML 未使用，因为非 Vue）

### 优化的 4 个方向

1. **字体升级**：引入 JetBrains Mono（设计系统推荐），替换系统 mono 栈
   - 提升等宽字符的识别度与终端气质
   - 使用 `display=swap` 避免 FOIT

2. **进度叙事升级**：把 generic 进度条换成 4 段 boot log
   - `✓ loading modules` / `✓ opening pty` / `✓ binding ports` / `✓ sessions ready`
   - 弱化终端绿 `rgba(140,212,138,0.92)` + 6px glow
   - 段式进度条与 boot log 同步入场

3. **视觉纵深**：
   - Logo 外辉光（`radial-gradient` + `filter: blur`）
   - 呼吸 halo 层（8s 周期，opacity 0.4→0.55，克制到不喧宾夺主）
   - 底部 vignette（60% 黑色渐变到透明）
   - 光标 phosphor glow（`box-shadow: 0 0 8px`）

4. **垂直节奏收紧**：
   - tagline→terminal 间距从 2.5rem 收到 1.75rem
   - terminal→boot log 从 1.25rem
   - 整体中心更紧凑，logo 有更强的存在感

## 遵循的 UX 约束（来自 ui-ux-pro-max 检索）

| 约束 | 落地 |
|---|---|
| 连续动画仅用于 loading 语义 | halo 呼吸极克制（8s/0.15 opacity 差），光标闪烁保持原设计 |
| 单屏 1-2 个主动效焦点 | 主：打字机 + 光标；辅：boot log 一次性入场（不循环） |
| 尊重 `prefers-reduced-motion` | 全部循环动画禁用、入场动画瞬间到终态 |
| Dark Mode OLED + 极简辉光 | text-shadow 最高 22px，box-shadow 8px，避免 Matrix 绿刺眼 |
| 无 emoji 图标 | 全部用 SVG / 字符 `✓` / 圆点 |
| 不做重 Cyberpunk | 无扫描线、无 glitch、无多色霓虹 |

## 下一步选项

- 把 optimized-v2 的改动移植回 `SplashLoading.vue`（保持品牌资产一致的前提下升级组件）
- 保留 replica.html 作为对比基线，仅内部评审
- 继续在 `.scratch/splash-replica/` 里试更多变体（如：CRT 扫描线版、多色霓虹版、极简版）

## 运行方式

```bash
cd bedcode-desktop/.scratch/splash-replica
python3 -m http.server 8931
# 浏览器打开 http://127.0.0.1:8931/replica.html 或 optimized-v2.html
```

截图（headless chrome，虚拟时间推进）：

```bash
google-chrome --headless=new --disable-gpu --no-sandbox \
  --window-size=1440,900 \
  --screenshot=/tmp/out.png \
  --virtual-time-budget=3000 \
  http://127.0.0.1:8931/optimized-v2.html
```
