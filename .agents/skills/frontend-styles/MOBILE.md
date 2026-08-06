# Mobile Reference

Mobile-specific styling rules for BedCode. Consult when working on `bedcode-mobile/` components.

## Flow Scaling

Mobile runs on phones (~360px) and tablets (~768px+). All controls scale smoothly via CSS `clamp()` — no fixed breakpoints.

### Pattern

Define sizing variables on the component root with `clamp()`:

```css
.component-root {
  --btn-h: clamp(2rem, 2.25rem + (100vw - 400px) / 800 * 0.5rem, 2.5rem);
  --btn-font: clamp(0.65rem, 0.75rem + (100vw - 400px) / 800 * 0.05rem, 0.8rem);
  --icon-size: calc(var(--btn-h) * 0.44);
}
.child-btn { height: var(--btn-h); font-size: var(--btn-font); }
.child-icon { width: var(--icon-size); height: var(--icon-size); }
```

### `clamp()` Formula

```
clamp(min, base + (100vw - small_bp) / (large_bp - small_bp) * (max - min), max)
```

- Small breakpoint: `400px`, large breakpoint: `1200px` (range: `800px`)
- The middle term is a linear interpolation between min and max across the viewport range

### What scales, what doesn't

| Scales (use `clamp()`) | Fixed (no scaling) |
|------------------------|-------------------|
| Toolbar button heights | Header/nav height (safe area driven) |
| Button font sizes | Modal/dialog size (centered) |
| Icon sizes (proportional to parent) | Colors, borders, shadows |
| Spacing/padding (tighten on small screens) | — |
| Input font size (height can stay fixed) | — |

### Container sizing

Panels and containers that depend on button sizes use `calc()`:

```css
.panel { max-height: calc(var(--btn-h) * 5 + 1rem); }
```

## Safe Areas

Always use utility classes — raw `env(safe-area-inset-*)` is handled internally:

| Class | Purpose |
|-------|---------|
| `mobile-header-safe` | Top padding for status bar |
| `mobile-nav-safe` | Bottom padding for home indicator |
| `mobile-input-bar` | Bottom input container with keyboard avoidance |

The `mobile-input-bar` class handles `keyboard-inset-bottom` with `duration-250` smooth transitions.

## Touch & Gesture

### Touch targets

Minimum 44px height for interactive elements. Token: `--input-height: 44px`, `--nav-item-height: 44px`.

### Active vs hover

Mobile uses `active:opacity-80` for press feedback. Cross-platform components include both:

```html
<button class="hover:bg-[var(--bg-hover)] active:opacity-80 transition-all duration-200">
```

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

`min-h-[100dvh]` for mobile full-height — `100vh` breaks under dynamic browser chrome (address bar, tab bar).

## Breakpoint Composable

`useBreakpoints()` (in `useOrientation.ts`) for JS logic branches:

```typescript
import { useBreakpoints } from '@/composables/useOrientation'
const { isSmall, isMobile, isTablet } = useBreakpoints()
// isSmall: < 400px, isMobile: < 768px, isTablet: 768-1024px
```

CSS `clamp()` handles visual scaling — JS breakpoints are for logic only (e.g., hiding a panel in landscape).

## Mobile Responsive Checklist

- [ ] Phone (~360px): buttons not crowded or overlapping
- [ ] Tablet (~768px): buttons not too small or wasting space
- [ ] Panels/popups fit within screen on small devices
- [ ] Container `max-height`/`min-height` tracks button sizes via `calc()`
- [ ] Icon sizes scale proportionally with parent via `calc(var(--btn-h) * ratio)`
