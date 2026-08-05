---
name: frontend-styles
description: |
  Frontend CSS/layout/animation best practices for BedCode (Vue 3 + TailwindCSS + CSS Variables).
  Use this skill whenever you write or modify Vue component styles, layout structures, CSS animations,
  design tokens, or responsive/mobile-safe styling — even for small CSS tweaks. Also use when creating
  new components, refactoring layouts, adding transitions, or working with dark/light theme variables.
  If the task involves any CSS class, style attribute, layout property, or animation, this skill applies.
---

# Frontend Styles & Layout Best Practices

BedCode uses **TailwindCSS utility-first** + **CSS custom properties (design tokens)** + **scoped `<style>`** for animations and complex layouts. This skill defines the conventions that keep the UI consistent, maintainable, and theme-aware.

---

## 1. Design Token System

All visual values flow through CSS custom properties defined in `style.css` (desktop) and `mobile.css` (mobile). This is the single source of truth for colors, spacing, radii, shadows, and typography.

### Token Categories

| Category | Prefix | Example |
|----------|--------|---------|
| Background | `--bg-*` | `--bg-page`, `--bg-card`, `--bg-hover`, `--bg-input` |
| Text | `--text-*` | `--text-primary`, `--text-secondary`, `--text-tertiary` |
| Border | `--border` | `--border` |
| Brand | `--color-*` | `--color-primary`, `--color-primary-hover`, `--color-primary-light` |
| Status | `--color-*-light` | `--color-success-light`, `--color-warning-light`, `--color-danger-light` |
| Radius | `--radius-*` | `--radius-card`, `--radius-button`, `--radius-input`, `--radius-tag`, `--radius-nav` |
| Shadow | `--shadow-*` | `--shadow-card`, `--shadow-card-hover`, `--shadow-input-focus` |
| Layout | `--sidebar-width`, `--header-height`, `--page-padding-x/y` | |
| Typography | `--font-size-*` | `--font-size-title`, `--font-size-body`, `--font-size-label` |
| Component | `--input-height`, `--button-height`, `--tag-height`, `--action-button-size` | |
| Mobile | `--mobile-*` | `--mobile-bg-card`, `--mobile-accent`, `--mobile-nav-bg`, `--mobile-overlay` |
| Safe Area | `--safe-area-*` | `--safe-area-top`, `--safe-area-bottom` |

### Rules

1. **Always use tokens, never hardcode colors/sizes.** Write `text-[var(--text-primary)]` not `text-gray-900`. Write `bg-[var(--bg-card)]` not `bg-white`. Tokens carry theme semantics — hardcoding breaks dark mode.

2. **Use Tailwind semantic aliases when available.** The tailwind config maps tokens to utility names:
   - `bg-page` → `var(--bg-page)`
   - `bg-card` → `var(--bg-card)`
   - `bg-sidebar` → `var(--bg-sidebar)`
   - `brand` / `brand-light` → `var(--color-primary)` / `var(--color-primary-light)`
   - `rounded-card` → `var(--radius-card)`
   - `rounded-btn` → `var(--radius-button)`
   - `shadow-card` / `shadow-card-hover` → card shadows

   Prefer `bg-card` over `bg-[var(--bg-card)]` when the alias exists. Use the `var()` syntax for tokens without aliases.

3. **Mobile tokens use `--mobile-*` prefix.** Mobile components have their own token set (defined in `mobile.css`) for the Dracula-inspired dark-first design. Mobile components should use `--mobile-*` tokens, not desktop `--bg-*` tokens.

4. **Desktop vs Mobile token divergence:** Currently, desktop Button uses tokens (`bg-brand`, `bg-[var(--color-danger-light)]`) while mobile Button still uses Tailwind color palette (`bg-primary-600`, `bg-slate-100 dark:bg-dark-700`). When writing new mobile components, prefer `--mobile-*` tokens. When modifying existing mobile components that use Tailwind colors, migrate to tokens incrementally — don't mix both approaches in the same component.

5. **Adding new tokens:** When a visual value appears in 3+ components, promote it to a token. Add it to both `:root` (light) and `:root.dark` (dark) in `style.css`, and optionally register it in `tailwind.config.js` if it needs a utility alias.

6. **When not to add a token — use `color-mix()` instead.** For one-off variations of an existing token (e.g., 40% opacity of `--mobile-error`), use CSS `color-mix()` rather than creating a new token:
   ```css
   border-color: color-mix(in srgb, var(--mobile-error) 40%, transparent)
   ```
   This keeps the token system lean while allowing derived values. Only promote to a token if the same derived value appears in multiple places.

---

## 2. Layout Patterns

### Desktop Layout

```
┌─────────────────────────────────────────┐
│ TitleBar (h = --header-height)          │
├──────────┬──────────────────────────────┤
│ Sidebar  │ Main Content                 │
│ w=240px  │ flex-1 overflow-hidden       │
│          │ bg-page                      │
└──────────┴──────────────────────────────┘
```

- Root: `flex flex-col h-screen desktop-ui`
- Content area: `flex flex-1 overflow-hidden`
- Sidebar: fixed width `--sidebar-width`
- Main: `flex-1 overflow-hidden bg-page`

### Mobile Layout

```
┌─────────────────────┐
│ Header (safe-area)  │
│ backdrop-blur-xl    │
├─────────────────────┤
│ Content             │
│ flex-1 overflow-y   │
├─────────────────────┤
│ Bottom Nav (safe)   │
│ backdrop-blur-xl    │
└─────────────────────┘
```

- Root: `min-h-[100dvh] mobile-ui mobile-app`
- Safe areas: `mobile-header-safe` (top), `mobile-nav-safe` (bottom)
- Keyboard: `mobile-input-bar` class handles keyboard avoidance

### Layout Rules

1. **Use flexbox for component-level layout.** `flex`, `flex-1`, `flex-col`, `items-center`, `justify-between` cover 90% of cases.

2. **Use CSS Grid only for 2D layouts** (dashboards, card grids, complex forms). Prefer `grid grid-cols-N gap-N`.

3. **Overflow control is critical.** Every scrollable container needs explicit `overflow-y-auto` or `overflow-hidden`. Never rely on browser default overflow behavior.

4. **`min-w-0` on flex children with text.** Prevents text from overflowing its container:
   ```html
   <div class="flex-1 min-w-0">
     <p class="truncate">Long text here</p>
   </div>
   ```

5. **`flex-shrink-0` on fixed-width elements.** Icons, badges, action buttons should not shrink:
   ```html
   <div class="flex items-center gap-3">
     <div class="flex-1 min-w-0">...</div>
     <button class="flex-shrink-0">Action</button>
   </div>
   ```

6. **Full-height pages:** Use `h-full` on page containers inside the main content area (which is already `flex-1 overflow-hidden`). For standalone full-height, use `min-h-screen` or `min-h-[100dvh]` (mobile).

---

## 3. Component Styling Conventions

### Class Organization Order

Write classes in this order for readability:

```
1. Layout:     flex, grid, block, items-center, justify-*
2. Sizing:     w-*, h-*, min-w-0, flex-1, flex-shrink-0
3. Spacing:    p-*, m-*, gap-*
4. Visual:     bg-*, text-*, border-*, rounded-*, shadow-*
5. State:      hover:*, focus:*, active:*, dark:*
6. Animation:  transition-*, duration-*, animate-*
7. Misc:       truncate, overflow-*, cursor-*, select-*
```

Example:
```html
<div class="flex items-center gap-3 flex-1 min-w-0 px-4 py-3 bg-card text-[var(--text-primary)] rounded-card shadow-card hover:shadow-card-hover transition-all duration-200 truncate">
```

### Dynamic Classes

Use `:class` with arrays for conditional styling, objects for toggles:

```html
<!-- Array syntax for mutually exclusive variants -->
<span :class="[
  'inline-flex items-center h-7 px-3 rounded-tag text-xs font-medium',
  isActive
    ? 'bg-[var(--color-primary-light)] text-blue-600 dark:text-blue-400'
    : 'bg-[var(--bg-hover)] text-[var(--text-secondary)]'
]">

<!-- Object syntax for feature toggles -->
<div :class="[
  'w-12 h-12 rounded-xl flex items-center justify-center',
  { 'bg-[var(--mobile-accent-muted)]': isOnline, 'bg-[var(--mobile-bg-elevated)]': !isOnline }
]">
```

### Inline Styles

Avoid inline `style` attributes except for:
- Dynamic values from JS (e.g., `transform: translateX(${offset}px)`)
- CSS variable overrides scoped to one instance
- Computed dimensions that can't use Tailwind utilities
- `color-mix()` derived values (Tailwind can't express these)

### Common Component Patterns

**Status indicator dot:**
```html
<!-- Online / running status -->
<div class="w-2 h-2 rounded-full bg-green-500 animate-pulse"></div>
<!-- Offline / stopped status -->
<div class="w-2 h-2 rounded-full bg-[var(--text-tertiary)]"></div>
<!-- Mobile connected glow effect -->
<div class="w-2.5 h-2.5 rounded-full bg-[var(--mobile-success)] shadow-[0_0_8px_rgba(16,185,129,0.5)] animate-pulse"></div>
```

**Inline SVG icon (project standard):**
```html
<svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
  <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="..." />
</svg>
```
All icons share: `fill="none" stroke="currentColor" viewBox="0 0 24 24" stroke-width="2"`. Sizes are `w-4 h-4` (small) or `w-5 h-5` (medium) or `w-6 h-6` (large). Color inherits from `text-*` on parent.

**Badge / tag:**
```html
<span class="inline-flex items-center h-7 px-3 rounded-tag text-xs font-medium bg-[var(--color-primary-light)] text-blue-600 dark:text-blue-400">
  WSL2
</span>
```

---

## 4. Dark Mode

### Strategy: Class-based (`darkMode: 'class'`)

The `dark` class is toggled on `<html>` by the theme system. All dark mode styles use the `dark:` prefix.

### Rules

1. **Prefer token-based theming over `dark:` prefixes.** Tokens already handle light/dark switching internally. `bg-card` works in both modes. `bg-white dark:bg-slate-800` is redundant when `bg-card` exists.

2. **Use `dark:` only for tokens that lack a semantic alias.** Example: status badge colors that use Tailwind's built-in palette:
   ```html
   <span class="text-blue-600 dark:text-blue-400">
   ```

3. **Never mix token and hardcoded approaches.** Don't write `bg-[var(--bg-card)] dark:bg-slate-800` — the token already handles dark mode.

4. **Mobile light mode** uses `html:not(.dark) .mobile-ui` selector in `mobile.css`. Mobile tokens have both dark (default) and light variants built in, so `var(--mobile-bg-card)` works in both themes without `dark:` prefixes.

5. **Theme transition:** Root containers (`mobile-ui`, `mobile-app`, `desktop-ui`) have `transition: background-color 0.2s ease, color 0.2s ease, border-color 0.2s ease` for smooth theme switching. Don't add this to individual components — it's handled at the root level.

---

## 5. Animation & Transitions

### Transition Rules

1. **Default transition: `transition-all duration-200`** for interactive elements (buttons, cards, links). This is the standard micro-interaction timing.

2. **Hover state transitions: `transition-colors duration-200`** when only color changes (lighter than `transition-all`).

3. **Layout transitions (keyboard, safe area): `duration-250` with `cubic-bezier(0.4, 0, 0.2, 1)`** (Material ease-out). Used for safe area padding, keyboard avoidance.

4. **Theme switching: `duration-200 ease`** for background-color, color, border-color transitions on root containers.

### Vue `<Transition>` Patterns

Use `<Transition name="xxx">` with scoped `<style>` for enter/leave animations:

**Modal (scale + fade):**
```vue
<Transition name="modal">
  <div v-if="show">...</div>
</Transition>

<style scoped>
.modal-enter-active,
.modal-leave-active {
  transition: all 0.2s ease;
}
.modal-enter-from,
.modal-leave-to {
  opacity: 0;
}
.modal-enter-from > :last-child,
.modal-leave-to > :last-child {
  transform: scale(0.95);
}
</style>
```

**Fade only:**
```vue
<style scoped>
.fade-enter-active,
.fade-leave-active {
  transition: opacity 0.2s ease;
}
.fade-enter-from,
.fade-leave-to {
  opacity: 0;
}
</style>
```

**Slide up (mobile sheets):**
```vue
<style scoped>
.slide-up-enter-active,
.slide-up-leave-active {
  transition: transform 0.3s cubic-bezier(0.4, 0, 0.2, 1);
}
.slide-up-enter-from,
.slide-up-leave-to {
  transform: translateY(100%);
}
</style>
```

**Toast (fade + slide):**
```vue
<style scoped>
.toast-enter-active,
.toast-leave-active {
  transition: all 0.3s ease;
}
.toast-enter-from,
.toast-leave-to {
  opacity: 0;
  transform: translateX(-50%) translateY(-10px);
}
</style>
```

### Animation Guidelines

1. **Keep animations under 300ms.** Anything longer feels sluggish. 200ms is the sweet spot for most UI feedback.

2. **Use `ease` for simple transitions, `cubic-bezier(0.4, 0, 0.2, 1)` for layout shifts.** The Material curve feels natural for elements that change position or size.

3. **Respect `prefers-reduced-motion`.** For complex animations, consider wrapping in a media query. Simple hover transitions (200ms) are fine universally.

4. **Animate `transform` and `opacity`, not `width`/`height`/`top`/`left`.** Transform and opacity are GPU-composited and don't trigger layout recalculation.

5. **Avoid `!important` in animations.** If specificity is an issue, use scoped styles or more specific selectors.

6. **Loading spinners:** Use Tailwind's `animate-spin` or `animate-pulse`. Custom animations go in `style.css` as `@keyframes` + utility class (e.g., `animate-pulse-slow`).

7. **GPU acceleration:** Add `will-change: transform` in scoped styles for elements that animate frequently (swipe tracks, drag handles). Remove it when the animation completes to free GPU memory — don't leave it on static elements.

---

## 6. Scoped Styles & Deep Selectors

### When to Use `<style scoped>`

Not just for animations — use scoped styles whenever:

1. **Transition animations** — Vue `<Transition>` hooks need named CSS classes
2. **Touch/gesture components** — `touch-action`, `overscroll-behavior`, `-webkit-overflow-scrolling` are layout-critical and shouldn't leak
3. **Third-party component overrides** — xterm, code highlighter, etc. need `:deep()` to penetrate their internal DOM
4. **Complex positioning** — when `positionStyle` is computed in JS and applied via `:style`, the base positioning rules belong in scoped CSS

### `:deep()` Usage

Use `:deep()` to style child component internals from a parent's scoped style:

```vue
<style scoped>
/* Override xterm's viewport — only way to style a library component's internals */
.viewer-code :deep(pre) {
  font-size: var(--font-size-sm);
  line-height: 1.6;
}

.viewer-code :deep(.diff-added) {
  background: rgba(16, 185, 129, 0.08);
}
</style>
```

Rules:
- Only use `:deep()` for third-party or unowned component internals
- Never use `:deep()` to reach into your own component's children — pass props or emit events instead
- Always scope `:deep()` under a specific parent class (`.viewer-code :deep(...)`) to avoid affecting unrelated components
- Prefer `:deep()` over `::v-deep` or `/deep/` (those are deprecated)

---

## 7. Z-Index Layering

Fixed layering convention to prevent stacking conflicts:

| Layer | Z-Index | Usage |
|-------|---------|-------|
| Base content | `0` (default) | Normal page content |
| Sticky elements | `z-10` | Sticky headers, input bars |
| Dropdowns | `z-30` | Select dropdowns, popover menus |
| Terminal input bar | `z-40` | `TerminalInputBar` (sticky bottom) |
| Overlays & Modals | `z-50` | Modal, BottomSheet, Toast, Tooltip |
| Emergency overlay | `z-[9999]` | Full-screen blocking dialogs (rare) |

Rules:
- `z-50` is the standard for overlays. All modals, toasts, and tooltips share this layer — they shouldn't appear simultaneously
- Never use `z-[9999]` unless you need to overlay on top of other `z-50` elements (e.g., a confirmation dialog over a modal)
- Sticky elements within scrollable containers use `z-10` (relative to their container)
- Don't invent intermediate z-values (`z-20`, `z-40`) without a reason — the gaps leave room for future layers

---

## 8. Overlay & Teleport Patterns

### Teleport to body

All fixed-position overlays (Modal, Toast, Tooltip, BottomSheet) use `<Teleport to="body">` to escape parent `overflow: hidden` containers:

```vue
<template>
  <Teleport to="body">
    <Transition name="modal">
      <div v-if="modelValue" class="fixed inset-0 z-50 ...">
        <!-- backdrop + content -->
      </div>
    </Transition>
  </Teleport>
</template>
```

Without Teleport, a modal inside a `overflow-hidden` container gets clipped.

### Overlay backdrop

**Desktop:** `bg-black/50 backdrop-blur-sm`
**Mobile:** `bg-[var(--mobile-overlay)]` (varies by context: `--mobile-overlay`, `--mobile-overlay-heavy`, `--mobile-overlay-light`)

### Header backdrop blur

Mobile headers use `backdrop-blur-xl` with semi-transparent background for the frosted glass effect:

```html
<header class="bg-[var(--mobile-bg-secondary)]/90 backdrop-blur-xl border-b border-[var(--mobile-border)]">
```

The `/90` opacity is essential — full opacity backgrounds make `backdrop-blur` invisible.

Desktop headers don't use backdrop-blur (opaque background).

---

## 9. Responsive & Platform-Specific

### Platform Detection

Use `@tauri-apps/plugin-os` for platform detection, **never screen width**:

```typescript
import { usePlatform } from '@/composables/usePlatform'
const { platformInfo } = usePlatform()
```

### Mobile Screen Size Compatibility (Critical)

移动端同时运行在手机和平板上，屏幕宽度差异大（手机 ~360px，平板 ~768px+）。**所有移动端组件必须兼容大小屏幕，不能只按一种尺寸设计。**

#### 核心原则

1. **按钮/控件尺寸必须流式缩放**，不能硬编码固定值。手机上固定 36px 按钮会太挤，平板上又偏小。
2. **优先使用 CSS `clamp()` 实现平滑缩放**，避免媒体查询断点跳变。
3. **在组件根元素上定义 CSS 变量**，子元素统一引用，一处调整全局生效。

#### 缩放模式：CSS 变量 + `clamp()`

在组件根元素上定义尺寸变量，用 `clamp(最小值, 首选值, 最大值)` 实现流式缩放：

```css
.component-root {
  /* 按钮高度：小屏 2rem，默认 2.25rem，大屏 2.5rem */
  --btn-h: clamp(2rem, 2.25rem + (100vw - 400px) / 800 * 0.5rem, 2.5rem);
  /* 字号：小屏 0.65rem，默认 0.75rem，大屏 0.8rem */
  --btn-font: clamp(0.65rem, 0.75rem + (100vw - 400px) / 800 * 0.05rem, 0.8rem);
}

.child-btn {
  height: var(--btn-h);
  font-size: var(--btn-font);
}
```

**`clamp()` 公式推导**：
- `clamp(min, preferred, max)` 中 `preferred` 是线性插值
- 格式：`clamp(小屏值, 默认值 + (100vw - 小屏断点) / (大屏断点 - 小屏断点) * (大屏值 - 小屏值), 大屏值)`
- 小屏断点 400px，大屏断点 1200px（差值 800px）

#### 何时必须缩放

| 场景 | 要求 |
|------|------|
| 快捷键/工具栏按钮网格 | 高度和字号用 `clamp()` 变量 |
| 面板/弹出层内按钮组 | 同上，面板高度也跟随 `calc()` |
| 输入框/文本区域 | 高度可固定，但字号应缩放 |
| 图标尺寸 | 跟随按钮高度按比例缩放，如 `calc(var(--btn-h) * 0.44)` |
| 间距/内边距 | 小屏适当收紧（`clamp()` 或固定较小值） |

#### 何时不需要缩放

- Header/导航栏高度（由安全区域和设计规范固定）
- 弹窗/模态框尺寸（居中显示，不受屏幕宽度影响）
- 颜色/边框/阴影（由 token 控制，与尺寸无关）

#### 断点 Composable

`useBreakpoints()`（在 `useOrientation.ts` 中）提供响应式断点，用于 JS 逻辑判断：

```typescript
import { useBreakpoints } from '@/composables/useOrientation'
const { isSmall, isMobile, isTablet } = useBreakpoints()
// isSmall: < 400px, isMobile: < 768px, isTablet: 768-1024px
```

**CSS 缩放优先，JS 断点仅用于逻辑分支**（如横屏时隐藏面板）。不要用 JS 断点切换 CSS 类来模拟响应式 — 用 `clamp()` 更平滑。

#### 常见问题检查清单

- [ ] 手机（~360px）上按钮是否拥挤/重叠？
- [ ] 平板（~768px）上按钮是否偏小/浪费空间？
- [ ] 面板/弹出层在小屏上是否超出屏幕？
- [ ] 固定高度的容器（`max-height`、`min-height`）是否跟随按钮缩放？
- [ ] 图标尺寸是否跟随父按钮缩放？

### Mobile-Specific Concerns

1. **Safe areas:** Always use `mobile-header-safe` / `mobile-nav-safe` classes. Never hardcode status bar / home indicator padding.

2. **Touch targets:** Minimum 44px height for interactive elements. Use `--input-height: 44px` and `--nav-item-height: 44px` tokens.

3. **Active states over hover:** Mobile uses `active:opacity-80` instead of `hover:` for press feedback. Include both for cross-platform components:
   ```html
   <button class="hover:bg-[var(--bg-hover)] active:opacity-80 transition-all duration-200">
   ```

4. **Keyboard avoidance:** Use `mobile-input-bar` class on bottom input containers. It handles `keyboard-inset-bottom` with smooth transitions.

5. **Scroll behavior:** Mobile terminal uses custom scroll container, not native xterm scroll. The `.xterm-viewport` is hidden via `overflow-y: hidden !important`.

6. **`100dvh` over `100vh`:** Use `min-h-[100dvh]` for mobile full-height containers to account for dynamic viewport (browser chrome).

7. **Overscroll behavior:** Add `overscroll-behavior: none` on mobile scroll containers that interact with swipe gestures to prevent browser rubber-banding from stealing horizontal gestures.

8. **Touch action:** Use `touch-action: pan-y` on containers that should allow vertical scrolling but capture horizontal swipes. Set `touch-action: none` on non-scrollable interactive elements (xterm viewport).

---

## 10. Common Anti-Patterns

| Anti-Pattern | Fix |
|---|---|
| `bg-white dark:bg-slate-800` | `bg-card` (token handles both) |
| `text-gray-900 dark:text-gray-100` | `text-[var(--text-primary)]` |
| Hardcoded `padding: 16px` | `p-4` (Tailwind utility) |
| `style="width: 240px"` | `w-60` or `w-[var(--sidebar-width)]` |
| `!important` in component styles | Increase specificity or use scoped styles |
| Animating `height` / `width` | Animate `transform: scale()` or `max-height` |
| `@media (max-width: 768px)` for platform | Use `usePlatform()` composable |
| `position: fixed` + `top: 0` without safe area | Add `mobile-header-safe` class |
| `v-show` with `<Transition>` | Use `v-if` — `<Transition>` only works with mount/unmount |
| Long class strings on one line | Break into multiple lines for readability when >5 classes |
| Modal without `<Teleport>` | Always wrap overlays in `<Teleport to="body">` |
| `backdrop-blur-xl` with full-opacity bg | Use `/90` opacity so blur is visible |
| `will-change` on static elements | Only add during animation, remove when done |
| Random z-index values | Follow the z-index layering convention (Section 7) |
| `:deep()` on own child components | Use props/events instead |
| 硬编码移动端按钮 `height: 2.25rem` | 使用 `clamp()` + CSS 变量实现流式缩放（见 §9） |
| 用 JS 断点切换 CSS 类做响应式 | 优先用 `clamp()`，JS 断点仅用于逻辑分支 |
| 手机/平板用同一固定尺寸 | 所有小控件必须流式缩放，兼容 360px–1200px |

---

## 11. New Component Style Checklist

When creating a new Vue component, verify:

- [ ] All colors use design tokens (`var(--*)`) or Tailwind semantic aliases (`bg-card`, `brand`)
- [ ] Dark mode works without manual `dark:` overrides (tokens handle it)
- [ ] Interactive elements have `transition-all duration-200` or `transition-colors duration-200`
- [ ] Text containers use `min-w-0` + `truncate` pattern
- [ ] Fixed-width elements use `flex-shrink-0`
- [ ] Scrollable areas have explicit `overflow-y-auto`
- [ ] Mobile components use `--mobile-*` tokens and safe area classes
- [ ] Touch targets are at least 44px on mobile
- [ ] **Mobile button/control sizes use `clamp()` + CSS variables for responsive scaling (§9)**
- [ ] **Container `max-height`/`min-height` that depend on button sizes use `calc()` with variables**
- [ ] **Icon sizes scale proportionally with parent button via `calc(var(--btn-h) * ratio)`**
- [ ] Animations use `<Transition>` + scoped `<style>`, not JS animation libraries
- [ ] No hardcoded pixel values that should be tokens
- [ ] No `!important` (except xterm viewport overrides)
- [ ] Fixed overlays use `<Teleport to="body">` and follow z-index convention
- [ ] SVG icons follow the project standard (`fill="none" stroke="currentColor" viewBox="0 0 24 24" stroke-width="2"`)
- [ ] Scoped styles only for animations, touch/gesture rules, or `:deep()` overrides — not as a default styling approach
- [ ] `:deep()` scoped under a specific parent class, never on own child components
