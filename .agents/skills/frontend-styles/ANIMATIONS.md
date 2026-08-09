# Animation Patterns

Vue `<Transition>` patterns and CSS animation reference for BedCode.

## Transition Timing Reference

| Context | Duration | Easing | When |
|---------|----------|--------|------|
| Interactive feedback | `200ms` | `ease` | Buttons, cards, hover states |
| Color-only changes | `200ms` | `ease` | Background, text color |
| Layout shifts | `250ms` | `cubic-bezier(0.4, 0, 0.2, 1)` | Keyboard avoidance, safe area padding |
| Enter/leave (modals) | `200ms` | `ease` | Mount/unmount transitions |
| Enter/leave (sheets) | `300ms` | Material curve | Bottom sheets, slide panels |
| Theme switching | `200ms` | `ease` | Root containers only |

## Vue `<Transition>` Patterns

### Modal (scale + fade)

```vue
<Transition name="modal">
  <div v-if="show">...</div>
</Transition>

<style scoped>
.modal-enter-active,
.modal-leave-active { transition: all 0.2s ease; }
.modal-enter-from,
.modal-leave-to { opacity: 0; }
.modal-enter-from > :last-child,
.modal-leave-to > :last-child { transform: scale(0.95); }
</style>
```

### Fade only

```vue
<style scoped>
.fade-enter-active,
.fade-leave-active { transition: opacity 0.2s ease; }
.fade-enter-from,
.fade-leave-to { opacity: 0; }
</style>
```

### Slide up (mobile sheets)

```vue
<style scoped>
.slide-up-enter-active,
.slide-up-leave-active { transition: transform 0.3s cubic-bezier(0.4, 0, 0.2, 1); }
.slide-up-enter-from,
.slide-up-leave-to { transform: translateY(100%); }
</style>
```

### Toast (fade + slide)

```vue
<style scoped>
.toast-enter-active,
.toast-leave-active { transition: all 0.3s ease; }
.toast-enter-from,
.toast-leave-to { opacity: 0; transform: translateX(-50%) translateY(-10px); }
</style>
```

### Page transition (route changes)

Defined in desktop `style.css` as `.page-enter-active` / `.page-leave-active` — fade + slight Y offset for route transitions.

## Keyframe Animations

Defined in `style.css` (both projects):

| Class | Effect | Usage |
|-------|--------|-------|
| `animate-pulse-slow` | Opacity 1 → 0.5 → 1, slower than default pulse | Subtle status indicators |
| `animate-pulse` | Tailwind built-in pulse | Loading states |
| `animate-spin` | Tailwind built-in spin | Spinners |

Mobile `terminal.css` adds:

| Transition Class | Effect | Usage |
|------------------|--------|-------|
| `.loading-fade-*` | Fade in/out | Terminal loading overlay |
| `.selection-bar-*` | Slide + fade | Text selection action bar |
| `.scroll-indicator-*` | Fade in/out | Scroll-to-bottom button |

## Custom Keyframes

Add to `style.css` as `@keyframes` + utility class:

```css
@keyframes wiggle {
  0%, 100% { transform: rotate(-1deg); }
  50% { transform: rotate(1deg); }
}
.animate-wiggle { animation: wiggle 0.3s ease-in-out infinite; }
```

## Performance Rules

1. Animate `transform` and `opacity` only — GPU-composited, no layout recalc
2. `will-change: transform` on actively animating elements; remove on completion
3. Use scoped `<style>` for `<Transition>` classes to avoid global CSS pollution
4. `prefers-reduced-motion` is **mandatory**, not optional. Every project ships this global guard (in `style.css` / `mobile.css`), so decorative transitions collapse to near-instant:

```css
@media (prefers-reduced-motion: reduce) {
  *, *::before, *::after {
    animation-duration: 0.01ms !important;
    animation-iteration-count: 1 !important;
    transition-duration: 0.01ms !important;
  }
}
```

This block does stop spinners (`animate-spin`) and pulses — accepted, they freeze harmlessly at one frame. If a status indicator must keep pulsing (e.g. a connection warning), exempt it explicitly:

```css
@media (prefers-reduced-motion: reduce) {
  .status-dot-critical { animation-duration: 2s !important; }
}
```

Complex enter/leave choreography beyond 300ms should additionally gate on `(prefers-reduced-motion: no-preference)`.
