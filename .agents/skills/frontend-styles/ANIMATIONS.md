# Animation Patterns

Vue `<Transition>` patterns and CSS animation reference for BedCode.

## Transition Timing Reference

| Context | Duration | Easing | When |
|---------|----------|--------|------|
| Interactive feedback | `200ms` | `ease` | Buttons, cards, hover states |
| Color-only changes | `200ms` | `ease` | Background, text color |
| Layout shifts | `250ms` | `cubic-bezier(0.4, 0, 0.2, 1)` | Keyboard avoidance, safe area padding |
| Enter/leave (modals) | `200ms` | `ease` | Mount/unmount transitions |
| Enter/leave (sheets) | `300ms` | `cubic-bezier(0.4, 0, 0.2, 1)` | Bottom sheets, slide panels |
| Page / view swaps | in `220ms` · out `160ms` | `--motion-page-ease` | Route + wasm-app view swaps (tokens, see below) |
| Theme switching | `200ms` | `ease` | Root containers only |

## Vue `<Transition>` Patterns

### Modal (scale + fade)

```vue
<Transition name="modal">
  <div v-if="show">...</div>
</Transition>

<style scoped>
.modal-enter-active,
.modal-leave-active { transition: opacity 0.2s ease, transform 0.2s ease; }
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
.toast-leave-active { transition: opacity 0.3s ease, transform 0.3s ease; }
.toast-enter-from,
.toast-leave-to { opacity: 0; transform: translateX(-50%) translateY(-10px); }
</style>
```

### Page transition (route / view swaps)

Desktop only, and **one system for the whole端** — host routes plus all four wasm apps
(they inject their CSS into the host document, so they share these classes).

| Rule | Value |
| --- | --- |
| Transition name | always `page` — never a per-plugin variant (`ah-page` / `ft-page` / `page-fade` / `view-slide` / `tab-fade` are retired) |
| Container class | always `page-swap` (positioning context for the leaving layer) |
| Padding containers | `page-swap-pad-md` (p-5) / `page-swap-pad-lg` (px-6 py-5) — padding and offset compensation share one variable |
| Mode | **no `mode`** — see below |
| Timing | `--motion-page-duration-in` 220ms / `--motion-page-duration-out` 160ms, `--motion-page-ease` |

```vue
<div class="page-swap">
  <Transition name="page"><component :is="view" /></Transition>
</div>
```

**Why no `mode="out-in"`:** out-in finishes the leave *before* mounting the enter, so
the container is empty for a frame or more and shows `--bg-page` — near-black in every
dark theme (`#15130f` / `#0f172a` / `#101713` / `#0b1620` / `#1b1210`). On a full-viewport
swap that reads as a black flash, not a transition. The default overlapping mode avoids
it: `.page-leave-active` sets `position: absolute` (padded containers get negative
top/left from `--page-swap-pad-*` so the layer stays aligned), so the old view leaves the
flow while the new one occupies it from frame one — the container is never empty.

**Effects (multiple, one switch point):** pick one of `fade` / `slide-up` (default) /
`slide-left` / `zoom` by setting `PAGE_TRANSITION_EFFECT` in
`bedcode-desktop/src/utils/pageTransition.ts` — it writes `<html data-page-fx="...">` and
the CSS selects the matching enter-from / leave-to pair. No settings UI on purpose: page
motion is app-level look, not a user preference, and a setting would destroy the single
source of truth. An unknown name throws rather than silently falling back.

**Enforcement:** `src/__tests__/style/pageTransition.test.ts` locks the name, the
absence of `out-in`, the container class, effect coverage, the reduced-motion fallback,
and that the four base classes are defined **only** in `src/style.css`.

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

1. Prefer compositor-friendly properties: `transform` / `opacity` / `filter`, plus `@property`-registered variables. Animating layout properties (`width` / `height` / `top` / `left` / `margin` / `padding`) forces a reflow — acceptable for content-expansion UX (`grid-template-rows: 0fr → 1fr`, `max-height`) with a comment explaining why
2. Durations default to 200–300ms. Anything beyond 300ms is a deliberate choice (see the `prefers-reduced-motion` gate below)
3. `will-change: transform` on actively animating elements; remove on completion
4. Use scoped `<style>` for `<Transition>` classes to avoid global CSS pollution
5. `prefers-reduced-motion` is **mandatory**, not optional

Every project ships this global guard (in `style.css` / `mobile.css`), so decorative transitions collapse to near-instant:

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
