---
name: frontend-styles
description: |
  BedCode frontend styling conventions — Vue 3 + TailwindCSS v3.4+ + CSS design tokens.
  Use when writing or modifying CSS classes, design tokens, layouts, animations, themes, or responsive styling.
  Also use when creating Vue components, refactoring layouts, adding transitions, or working with dark/light themes.
---

# Frontend Styles

> **Base**: TailwindCSS v3.4.1+ (v3 series, v4 升级路径见 `MODERN-CSS.md`)
> **Stack**: Vue 3 `<script setup>` + Tailwind utility-first + CSS custom properties (design tokens) + scoped `<style>`
> **Stack**: `bedcode-desktop` — 桌面 app 风 + `--ui-scale` 等比缩放
> **Stack**: `bedcode-mobile` — 移动 app 风（dark-first Dracula-inspired）

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
- **tight** — transitions under 300ms and property-specific (never blanket `transition-all`), overflow explicit, GPU-composited properties only
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
- Mobile components use `--mobile-*` tokens exclusively — desktop tokens stay in desktop (migrate existing violations on touch: `--bg-hover`, `--text-tertiary`, `--color-*-light` etc. are still used in 16 mobile spots)

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
flex items-center gap-3 flex-1 min-w-0 px-4 py-3 bg-card text-[var(--text-primary)] rounded-card shadow-card hover:shadow-card-hover transition-all duration-200 truncate
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
| Color + shadow changes | `transition-all duration-200` (only when the changed property set is small) |
| Color-only changes | `transition-colors duration-200` |
| Layout shifts (keyboard, safe area) | `duration-250` + `cubic-bezier(0.4, 0, 0.2, 1)` |

Rules:

- **Transition specific properties** (`transition-colors`, `transition-transform`, `transition-shadow`) — blanket `transition-all` is an anti-pattern (animates every property change, defeats compositing)
- Animate `transform` and `opacity` only — they're GPU-composited
- `will-change: transform` during active animation, remove when done
- Vue `<Transition>` requires `v-if` (mount/unmount), not `v-show`

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
| Animating `height` / `width` | Animate `transform: scale()` or `max-height` |
| `@media (max-width)` for platform | `usePlatform()` composable |
| `100vw` in `clamp()` for components inside panels/drawers | Container query: `container-type: inline-size` + `cqw` |
| `clamp()` for layout shape changes (nav ↔ sidebar) | `useBreakpoints()` structure layer |
| Interactive control min height < 44px in `clamp()` | Keep constant 44px minimum |
| `text-white` / `text-black` on brand/accent backgrounds | Contrast token: `text-[var(--color-primary-contrast)]` / `text-[var(--mobile-text-on-accent)]` |
| `transition-all` on complex components | Property-specific: `transition-colors`, `transition-transform` |
| `--font-size-*` on desktop / Tailwind default `text-*` on mobile | Single source: `text-*` (desktop), `--font-size-*` mapped into `text-*` (mobile) |
| Hover-only feedback on touch devices | Always pair `hover:` with `active:` feedback |
| `v-show` with `<Transition>` | `v-if` |
| `backdrop-blur` + full-opacity bg | `/90` opacity |
| `will-change` on static elements | Add during animation only |
| Random z-index values | Follow safe-stack table |
| `:deep()` on own child components | Props or events |
| Hardcoded mobile button height | `clamp()` + CSS variable |

## New Component Checklist

- [ ] Colors are token-bound (semantic alias > `var()` > `color-mix()`)
- [ ] Brand/accent text uses contrast token, not `text-white`/`text-black`
- [ ] Font size follows the platform's single source (`text-*` desktop / fluid tokens mobile)
- [ ] Dark mode works via tokens (no redundant `dark:` on token-backed classes)
- [ ] Interactive elements carry property-specific transitions (`transition-colors` / `transition-transform`; `transition-all` only on tiny property sets)
- [ ] Text containers: `min-w-0` + `truncate`; fixed-width siblings: `flex-shrink-0`
- [ ] Scrollable areas have explicit overflow
- [ ] Mobile: `--mobile-*` tokens, safe area classes, 44px min touch targets
- [ ] Mobile controls: `clamp()` + `cqw` (root declares `container-type: inline-size`), min ≥44px touch targets (see [`MOBILE.md`](MOBILE.md))
- [ ] Overlays: `<Teleport to="body">`, safe-stack z-index (see [`BLUEPRINTS.md`](BLUEPRINTS.md))
- [ ] SVG icons follow project standard (see [`BLUEPRINTS.md`](BLUEPRINTS.md))
- [ ] Animations use `<Transition>` + scoped `<style>` (see [`ANIMATIONS.md`](ANIMATIONS.md))
