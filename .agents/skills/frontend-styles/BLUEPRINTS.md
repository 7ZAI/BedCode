# Component Blueprints

Reusable component patterns for BedCode. Consult when building new components.

## SVG Icon Standard

All inline SVG icons share this base:

```html
<svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
  <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="..." />
</svg>
```

Sizes: `w-4 h-4` (small), `w-5 h-5` (medium), `w-6 h-6` (large). Color inherits from parent `text-*`.

## Status Indicator Dot

```html
<!-- Online / running -->
<div class="w-2 h-2 rounded-full bg-[var(--color-success)]"></div>

<!-- Offline / stopped -->
<div class="w-2 h-2 rounded-full bg-[var(--text-tertiary)]"></div>

<!-- Mobile connected glow -->
<div class="w-2.5 h-2.5 rounded-full bg-[var(--mobile-success)] shadow-[0_0_8px_rgba(16,185,129,0.5)] animate-pulse"></div>
```

Status colors always come from tokens (`--color-success/warning/danger`, `--mobile-*`) — never Tailwind palette classes (`bg-green-500`). The 12% alpha `*-light` / `*-muted` tokens provide tinted backgrounds for the same status.

## Badge / Tag

```html
<span class="inline-flex items-center h-7 px-3 rounded-tag text-xs font-medium
  bg-[var(--color-primary-light)] text-blue-600 dark:text-blue-400">
  WSL2
</span>
```

## Dynamic Classes

```html
<!-- Array syntax: mutually exclusive variants -->
<span :class="[
  'inline-flex items-center h-7 px-3 rounded-tag text-xs font-medium',
  isActive
    ? 'bg-[var(--color-primary-light)] text-blue-600 dark:text-blue-400'
    : 'bg-[var(--bg-hover)] text-[var(--text-secondary)]'
]">

<!-- Object syntax: independent toggles -->
<div :class="[
  'w-12 h-12 rounded-xl flex items-center justify-center',
  { 'bg-[var(--mobile-accent-muted)]': isOnline, 'bg-[var(--mobile-bg-elevated)]': !isOnline }
]">
```

## Inline Styles

Reserve inline `style` for:
- Dynamic JS values (`transform: translateX(${offset}px)`)
- CSS variable overrides scoped to one instance
- `color-mix()` derived values (Tailwind can't express these)

## Overlay Pattern (Modal / BottomSheet / Toast)

All fixed-position overlays use `<Teleport to="body">`:

```vue
<template>
  <Teleport to="body">
    <Transition name="modal">
      <div v-if="modelValue" class="fixed inset-0 z-50 flex items-center justify-center">
        <!-- Desktop backdrop -->
        <div class="absolute inset-0 bg-black/50 backdrop-blur-sm" @click="close" />
        <!-- Mobile backdrop -->
        <div class="absolute inset-0 bg-[var(--mobile-overlay)]" @click="close" />
        <!-- Content -->
        <div class="relative z-10 ...">
          <slot />
        </div>
      </div>
    </Transition>
  </Teleport>
</template>
```

### Backdrop by platform

| Platform | Pattern |
|----------|---------|
| Desktop | `bg-black/50 backdrop-blur-sm` |
| Mobile | `bg-[var(--mobile-overlay)]` (or `-heavy`, `-light` by context) |

### Mobile header backdrop blur

```html
<header class="bg-[var(--mobile-bg-secondary)]/90 backdrop-blur-xl border-b border-[var(--mobile-border)]">
```

The `/90` opacity is essential — full opacity backgrounds make `backdrop-blur` invisible.

## Scoped Styles

Use `<style scoped>` for:

1. **Transition animations** — Vue `<Transition>` hooks need named CSS classes
2. **Touch/gesture rules** — `touch-action`, `overscroll-behavior` must not leak
3. **Third-party overrides** — xterm, code highlighter internals via `:deep()`
4. **Complex positioning** — base rules for JS-computed position styles

### `:deep()` Rules

```vue
<style scoped>
.viewer-code :deep(pre) {
  font-size: var(--font-size-sm);
  line-height: 1.6;
}
</style>
```

- Scope under a specific parent class (`.viewer-code :deep(...)`)
- Third-party or unowned component internals only
- Use props/events for your own child components — `:deep()` is for library DOM
- Prefer `:deep()` over deprecated `::v-deep` or `/deep/`

## Button Pattern

Brand backgrounds invert between themes, so text uses the **contrast token** — never `text-white`:

```html
<!-- Primary -->
<button class="bg-brand hover:bg-[var(--color-primary-hover)] text-[var(--color-primary-contrast)] rounded-btn
  transition-all duration-200 h-8 px-4 text-sm">
  Save
</button>

<!-- Secondary -->
<button class="bg-card hover:bg-[var(--bg-hover)] border border-[var(--border)] rounded-btn
  transition-all duration-200 h-8 px-4 text-sm">
  Cancel
</button>

<!-- Danger -->
<button class="bg-[var(--color-danger-light)] hover:bg-red-100 dark:hover:bg-red-900/30
  text-red-600 dark:text-red-400 rounded-btn transition-all duration-200 h-8 px-4 text-sm">
  Delete
</button>

<!-- Ghost -->
<button class="bg-transparent hover:bg-[var(--bg-hover)] rounded-btn
  transition-all duration-200 h-8 px-4 text-sm">
  Edit
</button>
```

Hover-only feedback is meaningless on touch — every interactive element pairs `hover:` with an `active:` state (`active:opacity-80` on mobile).
