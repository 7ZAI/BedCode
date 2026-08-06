---
name: frontend-styles
description: |
  BedCode frontend styling conventions — Vue 3 + TailwindCSS + CSS design tokens.
  Use when writing or modifying CSS classes, design tokens, layouts, animations, themes, or responsive styling.
  Also use when creating Vue components, refactoring layouts, adding transitions, or working with dark/light themes.
---

# Frontend Styles

BedCode: **TailwindCSS utility-first** + **CSS custom properties (design tokens)** + **scoped `<style>`** for animations and third-party overrides.

Four principles anchor every rule below:

- **token-bound** — every visual value flows through a CSS custom property; tokens carry theme semantics
- **tight** — transitions under 300ms, overflow explicit, GPU-composited properties only
- **flow** — mobile controls scale smoothly via `clamp()`, not fixed breakpoints
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

**Token namespaces:**

- Desktop: `--bg-*`, `--text-*`, `--border*`, `--color-*`, `--radius-*`, `--shadow-*`, `--font-size-*`
- Mobile: `--mobile-*` prefix (separate token set, dark-first Dracula-inspired design)
- Mobile components use `--mobile-*` tokens exclusively — desktop tokens stay in desktop

Full catalog: see [`TOKENS.md`](TOKENS.md) when choosing a specific token.

## Layout

Desktop root: `flex flex-col h-screen desktop-ui` → sidebar (`--sidebar-width`) + main (`flex-1 overflow-hidden bg-page`).
Mobile root: `min-h-[100dvh] mobile-ui mobile-app` → header (safe-area) + content (`flex-1 overflow-y`) + bottom nav (safe-area).

Rules:

1. Flexbox first; CSS Grid only for 2D layouts
2. Explicit `overflow-y-auto` or `overflow-hidden` on every scrollable container
3. `min-w-0` on flex children containing text, paired with `truncate`
4. `flex-shrink-0` on icons, badges, and action buttons
5. `min-h-[100dvh]` for mobile full-height — `100vh` breaks under dynamic browser chrome

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

## Tight Transitions

| Element | Pattern |
|---------|---------|
| Interactive (buttons, cards, links) | `transition-all duration-200` |
| Color-only changes | `transition-colors duration-200` |
| Layout shifts (keyboard, safe area) | `duration-250` + `cubic-bezier(0.4, 0, 0.2, 1)` |

Rules:

- Animate `transform` and `opacity` only — they're GPU-composited
- `will-change: transform` during active animation, remove when done
- Vue `<Transition>` requires `v-if` (mount/unmount), not `v-show`

Full transition and keyframe patterns: see [`ANIMATIONS.md`](ANIMATIONS.md) when writing animations.

## Safe-Stack: Z-Index & Overlays

| Layer | Class | Usage |
|-------|-------|-------|
| Base | default `0` | Normal content |
| Sticky | `z-10` | Sticky headers, input bars |
| Dropdown | `z-30` | Popovers, select dropdowns |
| Overlay | `z-50` | Modal, Toast, Tooltip, BottomSheet |
| Emergency | `z-[9999]` | Confirm dialog over another overlay (rare) |

- All fixed overlays use `<Teleport to="body">` to escape `overflow-hidden` containers
- Desktop backdrop: `bg-black/50 backdrop-blur-sm`
- Mobile backdrop: `bg-[var(--mobile-overlay)]`
- Mobile headers: `backdrop-blur-xl` with `/90` opacity background (full opacity kills blur visibility)

Component blueprints (modals, badges, icons): see [`BLUEPRINTS.md`](BLUEPRINTS.md) when building new components.

## Mobile

Platform detection via `usePlatform()` composable (`@tauri-apps/plugin-os`). Use `useBreakpoints()` for JS logic branches only — CSS `clamp()` handles visual scaling.

Mobile controls scale smoothly between 360px (phone) and 1200px (tablet):

```css
.component-root {
  --btn-h: clamp(2rem, 2.25rem + (100vw - 400px) / 800 * 0.5rem, 2.5rem);
  --btn-font: clamp(0.65rem, 0.75rem + (100vw - 400px) / 800 * 0.05rem, 0.8rem);
}
```

Define sizing variables on the component root; children reference `var(--btn-h)`. Icon sizes scale proportionally: `calc(var(--btn-h) * 0.44)`.

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
| `v-show` with `<Transition>` | `v-if` |
| `backdrop-blur` + full-opacity bg | `/90` opacity |
| `will-change` on static elements | Add during animation only |
| Random z-index values | Follow safe-stack table |
| `:deep()` on own child components | Props or events |
| Hardcoded mobile button height | `clamp()` + CSS variable |

## New Component Checklist

- [ ] Colors are token-bound (semantic alias > `var()` > `color-mix()`)
- [ ] Dark mode works via tokens (no redundant `dark:` on token-backed classes)
- [ ] Interactive elements carry `transition-all duration-200` or `transition-colors duration-200`
- [ ] Text containers: `min-w-0` + `truncate`; fixed-width siblings: `flex-shrink-0`
- [ ] Scrollable areas have explicit overflow
- [ ] Mobile: `--mobile-*` tokens, safe area classes, 44px min touch targets
- [ ] Mobile controls: `clamp()` + CSS variable for flow scaling (see [`MOBILE.md`](MOBILE.md))
- [ ] Overlays: `<Teleport to="body">`, safe-stack z-index (see [`BLUEPRINTS.md`](BLUEPRINTS.md))
- [ ] SVG icons follow project standard (see [`BLUEPRINTS.md`](BLUEPRINTS.md))
- [ ] Animations use `<Transition>` + scoped `<style>` (see [`ANIMATIONS.md`](ANIMATIONS.md))
