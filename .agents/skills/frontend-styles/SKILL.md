---
name: frontend-styles
description: |
  BedCode frontend styling conventions — Vue 3 + TailwindCSS v3.4+ + CSS design tokens.
  Use when writing or modifying CSS classes, design tokens, layouts, animations, themes, or responsive styling.
  Also use when creating Vue components, refactoring layouts, adding transitions, or working with dark/light themes.
  Mandatory pairing: any UI *design decision* (new page/component, style/color/typography direction, interaction or a11y redesign) requires loading and querying the `ui-ux-pro-max` skill first — see the "UI 设计决策（强制前置 ui-ux-pro-max）" section.
---

# Frontend Styles

> **Base**: TailwindCSS v3.4.1+ (v3 series, v4 升级路径见 `MODERN-CSS.md`)
> **Stack**: Vue 3 `<script setup>` + Tailwind utility-first + CSS custom properties (design tokens) + scoped `<style>`
> **Stack**: `bedcode-desktop` — 桌面 app 风 + `--ui-scale` 等比缩放
> **Stack**: `bedcode-mobile` — 移动 app 风（dark-first Dracula-inspired）

## UI 设计决策（强制前置 ui-ux-pro-max）

本 skill 只管「BedCode 里怎么实现」；**「UI 该长什么样」的设计决策必须先查 [`ui-ux-pro-max`](../ui-ux-pro-max/SKILL.md)**。凭通用经验直接拍板视觉方案是不合规的。

**必须检索的场景（先跑检索，再动手写样式）：**

- 新页面 / 新视图 / 新组件（含插件 UI）
- 视觉方向类决定：配色、字体、图标风格、整体 style、密度与留白节奏
- 交互与动效方案设计：状态反馈、手势、动画编排、空态/加载/错误态
- 无障碍改造：对比度、焦点管理、语义、reduced-motion
- 图表 / 数据可视化的选型

**不必检索的场景（直接走本 skill 的 Checklist）：**纯 token 级修正（换个颜色类、修对齐/断点/溢出）、既有组件的微调、修 CSS bug、按本 skill 既有蓝图复制组件。

**检索模式对照：**

| 需求 | 命令形态 |
|------|---------|
| 新页面 / 视觉方向 | `search.py "<product> <tone>" --design-system` |
| 聚焦问题（焦点、表单错误、滚动、图标语义） | `search.py "<outcome>" --domain ux`（一次一个 observable outcome） |
| 配色 / 字体 / 图标语义 | `--domain color` / `--domain typography` / `--domain icons` |
| 实现层细节 | `--stack vue` 或 `--stack html-tailwind` |

BedCode 的技术栈是 **Vue 3 + Tailwind v3 + Tauri（桌面 webview / Android WebView）**：检索结果里 `react` / `nextjs` / `react-native` / `swiftui` 等专属条目一律不适用，`web` domain 中 App UI 的 safe-area / 触控目标条目适用。

**真源裁决（本 skill 优先于检索结果）：**

1. 检索结果只是建议，**不得覆盖**本 skill 的 token-bound / safe-stack / dark-mode / 移动端 safe-area 规则，也**不得引入新的第三方色板、hex 值或 Google Fonts**——颜色与字体的落点只能是 [`TOKENS.md`](TOKENS.md) 里的 token；图标规范以 [`BLUEPRINTS.md`](BLUEPRINTS.md) 的 SVG 标准为准（`@phosphor-icons/react` 之类 npm 图标包不适用）
2. 禁止把检索结果 `--persist` 成 `design-system/<slug>/MASTER.md`——BedCode 的设计真源就是本 skill 目录，不新建平行真源
3. 结论要可追溯：新增 UI 的设计决策需在改动说明里写明参考了哪条检索结果（domain + 关键词），而不是泛泛「参考了最佳实践」

## 引用文档

本 skill 配套文件位于 `.agents/skills/frontend-styles/`：

- [`TOKENS.md`](./TOKENS.md) — 完整 token 目录
- [`ANIMATIONS.md`](./ANIMATIONS.md) — 动画/过渡模式
- [`MOBILE.md`](./MOBILE.md) — 移动端专项（safe area / 键盘避让 / touch-action）
- [`BLUEPRINTS.md`](./BLUEPRINTS.md) — 组件蓝图（modal / badge / icon）
- [`I18N.md`](./I18N.md) — 字体策略 / 行高 / logical properties / RTL
- [`PERFORMANCE.md`](./PERFORMANCE.md) — CLS / 字体 / 图片 / 长列表优化
- [`VUE3-STYLING.md`](./VUE3-STYLING.md) — v-bind() / useCssVars() / scoped 机制
- [`MODERN-CSS.md`](./MODERN-CSS.md) — @layer / @property / :has() / View Transitions
- [`LINTING.md`](./LINTING.md) — Stylelint 配置 + CI 集成 + token 命名护栏


BedCode: **TailwindCSS utility-first** + **CSS custom properties (design tokens)** + **scoped `<style>`** for animations and third-party overrides.

Four principles anchor every rule below:

- **token-bound** — every visual value flows through a CSS custom property; tokens carry theme semantics
- **tight** — default transition ≤300ms (exceed it deliberately — see [`ANIMATIONS.md`](ANIMATIONS.md) for the gate), property-specific (`transition-all` only on a leaf control with ≤2 lightweight properties), overflow explicit, compositor-friendly properties first
- **flow** — structure switches via breakpoints, value tuning via `clamp()` container queries
- **safe-stack** — z-index layers and Teleport stacking follow a fixed convention

## Token-Bound Styling

Every visual value — color, radius, shadow, spacing — flows through CSS custom properties. Tokens carry theme semantics; hardcoded values break theming.

**Preference order:**

1. Tailwind semantic alias: `bg-card`, `bg-page`, `bg-sidebar`, `brand`, `brand-light`, `rounded-card`, `rounded-btn`, `shadow-card`
2. Raw token: `bg-[var(--bg-card)]`, `text-[var(--text-primary)]`
3. `color-mix()` for one-off derived values:
   ```css
   border-color: color-mix(in srgb, var(--mobile-error) 40%, transparent)
   ```
4. New token — when the same value appears in 3+ components, promote to token in both `:root` (light) and `:root.dark` (dark)

**Single source of truth** — each dimension has exactly one access path:

- Font size: `text-*` utilities only on desktop; `--font-size-*` (fluid) tokens only on mobile — never both, never Tailwind defaults on mobile (see [`TOKENS.md`](TOKENS.md))
- Text on brand/accent backgrounds: always the contrast token (`--color-primary-contrast` / `--mobile-text-on-accent`) — `text-white`/`text-black` are banned because brand colors invert between themes

**Token namespaces:**

- Desktop: `--bg-*`, `--text-*`, `--border*`, `--color-*`, `--radius-*`, `--shadow-*`, `--font-size-*`
- Mobile: `--mobile-*` prefix (separate token set, dark-first Dracula-inspired design) — **except the root-level fluid type scale `--font-size-*`** (single documented exception)
- Mobile components use `--mobile-*` tokens exclusively — desktop tokens stay in desktop (desktop tokens referenced from mobile source: 0 — migrated; re-run `grep -rn -- '--bg-\|--text-\|--color-' bedcode-mobile/src` before adding one)

Full catalog: see [`TOKENS.md`](TOKENS.md) when choosing a specific token.

## Layout

Desktop root: `flex flex-col h-screen desktop-ui` → sidebar (`--sidebar-width`) + main (`flex-1 overflow-hidden bg-page`).
Mobile root: `h-[100dvh] mobile-ui mobile-app` → header (safe-area) + content (`flex-1 overflow-y`) + bottom nav (safe-area).

Rules:

1. Flexbox first; CSS Grid only for 2D layouts
2. Explicit `overflow-y-auto` or `overflow-hidden` on every scrollable container
3. `min-w-0` on flex children containing text, paired with `truncate`
4. `flex-shrink-0` on icons, badges, and action buttons
5. Mobile full-height: root uses `h-[100dvh]` (locked viewport, inner content scrolls); scrollable full-height pages use `min-h-[100dvh]` — `100vh` breaks under dynamic browser chrome

## Class Ordering

Write classes in this order:

```
layout → sizing → spacing → visual → state → transition → misc
flex items-center gap-3 flex-1 min-w-0 px-4 py-3 bg-card text-[var(--text-primary)] rounded-card shadow-card hover:shadow-card-hover transition-colors duration-200 truncate
```

`:class` with arrays for mutually exclusive variants, objects for independent toggles. Break long class strings across lines when >5 classes.

## Dark Mode

Strategy: class-based (`darkMode: 'class'`, `dark` on `<html>`).

1. **Tokens handle light/dark automatically.** `bg-card` works in both modes — no `dark:` override needed
2. `dark:` prefix only for Tailwind built-in palette values lacking a semantic alias: `text-blue-600 dark:text-blue-400`
3. Mobile tokens include both themes internally (`html:not(.dark) .mobile-ui` selector in `mobile.css`) — `var(--mobile-bg-card)` works without `dark:` prefix
4. Theme transition lives on root containers (`mobile-ui`, `mobile-app`, `desktop-ui`): `transition: background-color 0.2s, color 0.2s, border-color 0.2s` — apply once at root level
5. Declare `color-scheme` on the theme root so native controls/scrollbars match (mobile: `color-scheme: light/dark` toggled by `App.vue`; desktop: `color-scheme: dark` in `:root.dark`)

## Tight Transitions

| Element | Pattern |
|---------|---------|
| Color changes (buttons, cards, links) | `transition-colors duration-200` |
| Color + shadow | `transition-colors transition-shadow duration-200` |
| Layout shifts (keyboard, safe area) | `duration-300` + `cubic-bezier(0.4, 0, 0.2, 1)` |
| Off-scale duration (e.g. 250ms) | `duration-[250ms]` — Tailwind v3 ships no `duration-250` (75/100/150/200/300/500/700/1000) |

Rules:

- **Transition specific properties** (`transition-colors`, `transition-transform`, `transition-shadow`) — prefer these. `transition-all` is acceptable only when the change set is ≤2 lightweight properties (`background-color`, `color`, `box-shadow`, `border-color`) on a leaf control; never on a container that carries layout
- **Prefer compositor-friendly properties**: `transform` / `opacity` / `filter`, and CSS variables registered with `@property` (they animate once typed — see [`MODERN-CSS.md`](MODERN-CSS.md)). Animating layout properties (`width` / `height` / `top` / `left` / `margin` / `padding`) forces layout recalc — allowed for content-expansion UX (`grid-template-rows: 0fr → 1fr`, `max-height`), state it in a comment
- `will-change: transform` during active animation, remove when done
- Vue `<Transition>` supports both `v-if` (mount/unmount) and `v-show` (toggle). Prefer `v-if` — `v-show` keeps the element in the DOM permanently, so hidden state is still measurable and paintable. Use `v-show` when the element must stay in the DOM (preserved scroll position, cached measurement); note the reason inline

Full transition and keyframe patterns: see [`ANIMATIONS.md`](ANIMATIONS.md) when writing animations.

## Safe-Stack: Z-Index & Overlays

| Layer | Class | Usage |
|-------|-------|-------|
| Base | default `0` | Normal content |
| Sticky | `z-10` | Sticky headers, input bars |
| Local overlay | `z-20` | Scrims inside a terminal/viewport (`TerminalWindowView`) |
| Dropdown | `z-30` | Popovers, select dropdowns |
| Overlay | `z-50` | Modal, Toast, Tooltip, BottomSheet |
| Fullscreen | `z-[100]` | Splash, input assistant, full-screen config modals — must cover all overlays |
| Emergency | `z-[9999]` | Confirm dialog over another overlay (rare, only when nothing else fits) |

- All fixed overlays use `<Teleport to="body">` to escape `overflow-hidden` containers
- Desktop backdrop: `bg-black/50 backdrop-blur-sm`
- Mobile backdrop: `bg-[var(--mobile-overlay)]`
- Mobile headers: `backdrop-blur-xl` with `/90` opacity background (full opacity kills blur visibility)

Component blueprints (modals, badges, icons): see [`BLUEPRINTS.md`](BLUEPRINTS.md) when building new components.

## Mobile

Platform detection via `usePlatform()` composable (`@tauri-apps/plugin-os`). Responsive adaptation is **layered**:

1. **Structure** (layout shape: bottom nav ↔ sidebar, single ↔ split column) — `useBreakpoints()`, never `clamp()`
2. **Continuous** (control size, font, spacing within a shape) — `clamp()` with **container-query units** (`cqw`), never `100vw` in components

> Root-level token definitions (e.g. `--font-size-*` in `mobile.css` `:root`) may use `100vw` — the app is full-screen, so viewport == container. `100vw` is banned only inside components that can be embedded in panels/drawers.

Components declare themselves as query containers so scaling follows the component's own width — correct in split panels and drawers, not just full-screen:

```css
.component-root {
  container-type: inline-size; /* cqw 以组件自身宽度为基准 */
  --btn-h: clamp(2.75rem, 2.75rem + (100cqw - 400px) / 800 * 4, 3rem);
  --btn-font: clamp(0.8rem, 0.8rem + (100cqw - 400px) / 800 * 0.8, 0.9rem);
}
```

Define sizing variables on the component root; children reference `var(--btn-h)`. Icon sizes scale proportionally: `calc(var(--btn-h) * 0.44)`. Touch targets keep a **constant minimum 44px** — `clamp()` min values never go below it.

Full mobile reference — safe areas, touch targets, keyboard avoidance, scroll behavior, touch-action: see [`MOBILE.md`](MOBILE.md) when working on mobile components.

## Anti-Patterns

| Instead | Write |
|---------|-------|
| `bg-white dark:bg-slate-800` | `bg-card` |
| `text-gray-900 dark:text-gray-100` | `text-[var(--text-primary)]` |
| `padding: 16px` in style | `p-4` utility |
| `style="width: 240px"` | `w-[var(--sidebar-width)]` |
| Animating `height` / `width` | Decorative bars/skeleton: `scaleY()` / `scaleX()` + `transform-origin`. Content expansion: `grid-template-rows: 0fr → 1fr` or `max-height` — scaling distorts text, borders, radii |
| `@media (max-width)` for platform | `usePlatform()` composable |
| `100vw` in `clamp()` for components inside panels/drawers | Container query: `container-type: inline-size` + `cqw` |
| `clamp()` for layout shape changes (nav ↔ sidebar) | `useBreakpoints()` structure layer |
| Interactive control min height < 44px in `clamp()` | Keep constant 44px minimum |
| `text-white` / `text-black` on brand/accent backgrounds | Contrast token: `text-[var(--color-primary-contrast)]` / `text-[var(--mobile-text-on-accent)]` |
| `transition-all` on a layout-carrying container | Property-specific: `transition-colors`, `transition-transform` |
| `--font-size-*` on desktop / Tailwind default `text-*` on mobile | Single source: `text-*` (desktop), `--font-size-*` mapped into `text-*` (mobile) |
| Hover-only feedback on touch devices | Always pair `hover:` with `active:` feedback |
| `v-show` with `<Transition>` without reason | `v-if`, or `v-show` + inline note (element must stay in DOM) |
| `backdrop-blur` + full-opacity bg | `/90` opacity |
| `will-change` on static elements | Add during animation only |
| Random z-index values | Follow safe-stack table |
| `:deep()` on own child components | Props or events |
| Hardcoded mobile button height | `clamp()` + CSS variable |

## New Component Checklist

- [ ] 设计决策已用 `ui-ux-pro-max` 检索过（新页面/新组件/视觉方向/交互或无障碍改造必查；纯 token 级修正豁免），且结论未被检索建议覆盖 token-bound / safe-stack 规则
- [ ] Colors are token-bound (semantic alias > `var()` > `color-mix()`)
- [ ] Brand/accent text uses contrast token, not `text-white`/`text-black`
- [ ] Font size follows the platform's single source (`text-*` desktop / fluid tokens mobile)
- [ ] Dark mode works via tokens (no redundant `dark:` on token-backed classes)
- [ ] Interactive elements carry property-specific transitions (`transition-colors` / `transition-transform`; `transition-all` only on ≤2 lightweight properties of a leaf control)
- [ ] Text containers: `min-w-0` + `truncate`; fixed-width siblings: `flex-shrink-0`
- [ ] Scrollable areas have explicit overflow
- [ ] Mobile: `--mobile-*` tokens, safe area classes, 44px min touch targets
- [ ] Mobile controls: `clamp()` + `cqw` (root declares `container-type: inline-size`), min ≥44px touch targets (see [`MOBILE.md`](MOBILE.md))
- [ ] Overlays: `<Teleport to="body">`, safe-stack z-index (see [`BLUEPRINTS.md`](BLUEPRINTS.md))
- [ ] SVG icons follow project standard (see [`BLUEPRINTS.md`](BLUEPRINTS.md))
- [ ] Animations use `<Transition>` + scoped `<style>` (see [`ANIMATIONS.md`](ANIMATIONS.md))
