# Mobile Reference

Mobile-specific styling rules for BedCode. Consult when working on `bedcode-mobile/` components.

## Flow Scaling

Mobile runs on phones (~360px) and tablets (~768px+). Responsive adaptation is **layered** — mainstream practice separates structural switches from continuous tuning:

| Layer | Concern | Mechanism |
|-------|---------|-----------|
| Structure | Layout shape (bottom nav → sidebar, single → split column) | `useBreakpoints()` / minimal `@media` — never `clamp()` |
| Continuous | Control size, font size, spacing within one shape | `clamp()` + CSS variables |
| Embedded | Component inside split panels, drawers, narrow columns | Container queries (`cqw`) |

`clamp()` only tunes values inside one layout shape; it cannot express structural change.

### Pattern

Define sizing variables on the component root. The root declares itself a **query container**, so scaling is based on the component's own width (`cqw`), not the viewport — correct in full-screen and embedded contexts alike:

```css
.component-root {
  container-type: inline-size; /* 组件根作为查询容器，cqw 以组件自身宽度为基准 */
  --btn-h: clamp(2.75rem, 2.75rem + (100cqw - 400px) / 800 * 4, 3rem);
  --btn-font: clamp(0.8rem, 0.8rem + (100cqw - 400px) / 800 * 0.8, 0.9rem);
  --icon-size: calc(var(--btn-h) * 0.44);
}
.child-btn { height: var(--btn-h); font-size: var(--btn-font); }
.child-icon { width: var(--icon-size); height: var(--icon-size); }
```

`cqw` (1% of the nearest query container's width) resolves against `.component-root` at the point of use, so variables defined on the root keep working for children. Unlike `100vw`, it stays correct when the component is embedded in a split panel or drawer. Container queries are supported on all BedCode targets (Chromium ≥105, Safari ≥16, Android WebView).

> **Root-level exception**: token definitions on `:root` (e.g. `--font-size-*` fluid scale in `mobile.css`) may use `100vw` — the app is full-screen, so viewport == container. The `cqw` rule applies to components that can be embedded.

### Fluid Type Scale (root-level)

Mobile font sizes are a **fluid scale** — `--font-size-*` tokens in `mobile.css` interpolate between phone and tablet via `100vw` clamp, multiplied by `--mobile-font-scale` (`useFontSize` composable):

```css
:root {
  --font-size-sm: calc(clamp(0.6875rem, 0.75rem + (100vw - 360px) / 840 * 2, 0.875rem) * var(--mobile-font-scale, 1));
}
```

11px → 14px across 360→1200px. **Tailwind `text-*` must map to these tokens** in `tailwind.config.js` (`xs: 'var(--font-size-xs)'`, …) so utilities and raw tokens stay identical — no parallel scale.

### `clamp()` Formula

```
clamp(min, base + (100cqw - small_bp) / (large_bp - small_bp) * (max - min), max)
```

- Small breakpoint: `400px`, large breakpoint: `1200px` (range: `800px`)
- The middle term is a linear interpolation between min and max across the container range
- All values in `rem` — never hardcode `px` min/max, so user browser font scaling still applies

### Touch targets are fixed, visual size is fluid

Fingers don't grow with the screen: interactive controls keep a **constant minimum of 44px**, and `clamp()` min values must never go below it. Tablets get more spacing and content density, not bigger buttons (Apple HIG / Material: touch targets are ergonomics, not display size).

### What scales, what doesn't

| Scales (use `clamp()`) | Fixed (no scaling) |
|------------------------|-------------------|
| Toolbar button heights (min 44px) | Touch target minimums (44px, constant) |
| Button font sizes | Header/nav height (safe area driven) |
| Icon sizes (proportional to parent) | Modal/dialog size (centered) |
| Spacing/padding (tighten on small screens) | Colors, borders, shadows |
| Input font size (height can stay fixed) | — |

### Container sizing

Panels and containers that depend on button sizes use `calc()`:

```css
.panel { max-height: calc(var(--btn-h) * 5 + 1rem); }
```

## Safe Areas

Always use utility classes (defined in `bedcode-mobile/src/style.css`) — raw `env(safe-area-inset-*)` is handled internally:

| Class | Purpose |
|-------|---------|
| `mobile-header-safe` | Top padding for status bar |
| `mobile-nav-safe` | Bottom padding for home indicator |
| `mobile-input-bar` | Bottom input container with keyboard avoidance |

The `mobile-input-bar` class handles `keyboard-inset-bottom` with `duration-250` smooth transitions.

## Touch & Gesture

### Touch targets

Constant minimum 44px height for interactive elements — never scaled down by `clamp()`, never scaled up by breakpoints. Fingers don't grow with the screen; tablets get spacing and content density, not bigger buttons (Apple HIG / Material).

Spec tokens (defined in `mobile.css`): `--mobile-input-height: 44px`, `--mobile-nav-item-height: 44px`, `--mobile-touch-target-min: 44px`.

### Active vs hover

Mobile uses `active:opacity-80` for press feedback. Cross-platform components include both — note mobile has **no dedicated hover-background token**, so hover uses `--mobile-bg-tertiary` (never desktop tokens like `--bg-hover`):

```html
<button class="hover:bg-[var(--mobile-bg-tertiary)] active:opacity-80 transition-colors duration-200">
```

Hover-only feedback is meaningless on touch — every interactive element must pair `hover:` with `active:`. For hover styles that would misbehave when a finger stays on screen (expanding tooltips, repositioning), gate with `@media (hover: hover)` or the Tailwind `hover:` variant only for color/shadow micro-changes.

### Touch-action

| Value | Usage |
|-------|-------|
| `touch-action: pan-y` | Vertical scroll containers that capture horizontal swipes |
| `touch-action: none` | Non-scrollable interactive elements (xterm viewport) |

### Overscroll

`overscroll-behavior: none` on scroll containers interacting with swipe gestures — prevents browser rubber-banding from stealing horizontal gestures.

### Scroll behavior

Mobile terminal uses a custom scroll container. The `.xterm-viewport` is hidden via `overflow-y: hidden !important` in `mobile.css`.

## Viewport

Root uses `h-[100dvh]` (locked viewport, inner content scrolls); scrollable full-height pages use `min-h-[100dvh]` — `100vh` breaks under dynamic browser chrome (address bar, tab bar).

## Breakpoint Composable — Structure Layer

`useBreakpoints()` (in `useOrientation.ts`) is the **structure layer**: layout shape changes (bottom nav → sidebar, single → split column, hiding a panel in landscape) go through JS logic, never through `clamp()`:

```typescript
import { useBreakpoints } from '@/composables/useOrientation'
const { isSmall, isMobile, isTablet } = useBreakpoints()
// isSmall: < 400px, isMobile: < 768px, isTablet: 768-1024px
```

Keep the number of shapes minimal (typically two: phone / tablet). Everything inside one shape scales with `clamp()` + `cqw`.

## Mobile Responsive Checklist

- [ ] Phone (~360px): controls ≥44px, not crowded or overlapping
- [ ] Tablet (~768px): structure switch handled by `useBreakpoints()`, controls sized by `clamp()` + `cqw`
- [ ] Embedded (split panel / drawer): scaling correct via `cqw` — no `100vw`
- [ ] Panels/popups fit within screen on small devices
- [ ] Container `max-height`/`min-height` tracks button sizes via `calc()`
- [ ] Icon sizes scale proportionally with parent via `calc(var(--btn-h) * ratio)`
- [ ] Touch targets constant ≥44px at every width; `clamp()` min/max in `rem` (user font scaling works)
- [ ] `text-*` maps to fluid `--font-size-*` tokens (no Tailwind defaults on mobile)
- [ ] Brand/accent text uses `--mobile-text-on-accent`, not `text-white`
- [ ] Hover states paired with `active:` feedback
