# Token Catalog

Complete design token reference for BedCode. Consult when choosing a specific token.

## Token Principles

- **Semantic-first** — tokens are role-named (`--bg-card`, `--text-primary`), not value-named (`--dark-gray`). Pick by role, never by hue.
- **Two layers** (W3C DTCG-inspired, simplified): global semantic tokens (`--bg-*` / `--text-*` / `--mobile-*` base set) + component tokens (`--mobile-<component>-*` prefix groups). No primitive layer — raw values live in the token definitions themselves.
- **Promote at 3+ reuse** — same value in 3+ components becomes a token in both `:root` and `:root.dark`.
- **Single source of truth** — every dimension has exactly one access path (see Typography below). Never bypass it with a parallel token or hardcoded value.
- **Contrast rule** — text on brand/accent backgrounds must use the dedicated contrast token (`--color-primary-contrast`, `--mobile-text-on-accent`), never `text-white` / `text-black`.

## Desktop Tokens

Defined in `bedcode-desktop/src/style.css`. Light is `:root`, dark is `:root.dark`, palettes via `[data-palette='name']` (default `warm`; `cool` overrides all tokens).

### Backgrounds

| Token | Light | Dark | Semantic Alias |
|-------|-------|------|----------------|
| `--bg-page` | `#F5F4F0` | `#15130F` | `bg-page` |
| `--bg-sidebar` | `#EFEDE7` | `#191712` | `bg-sidebar` |
| `--bg-card` | `#FDFCFA` | `#1D1A15` | `bg-card` |
| `--bg-hover` | `#E7E4DC` | `#2A261E` | — |
| `--bg-input` | `#FDFCFA` | `#1D1A15` | — |

### Text

| Token | Light | Dark |
|-------|-------|------|
| `--text-primary` | `#26231B` | `#ECE8DC` |
| `--text-secondary` | `#7C7565` | `#9C9482` |
| `--text-tertiary` | `#ABA492` | `#6B6455` |

### Borders

| Token | Light | Dark |
|-------|-------|------|
| `--border` | `#E4E1D8` | `#2E2A22` |
| `--border-strong` | `#D6D2C4` | `#3C372B` |
| `--border-input` | `#D6D2C4` | `#3C372B` |

### Brand

| Token | Light | Dark | Semantic Alias |
|-------|-------|------|----------------|
| `--color-primary` | `#1D1A14` | `#ECE8DC` | `brand` |
| `--color-primary-hover` | `#0F0D09` | `#F5F2EA` | — |
| `--color-primary-light` | `#EFEDE5` | `rgba(236,232,220,0.12)` | `brand-light` |
| `--color-primary-contrast` | `#FDFCFA` | `#15130F` | — |

> Brand 背景上的文字**必须**用 `--color-primary-contrast`。Dark 下 `--color-primary` 是浅色，`text-white` 会不可见（WCAG 对比度要求）。

### Status Colors

| Token | Light | Dark | Usage |
|-------|-------|------|-------|
| `--color-success` / `-light` | `#22C55E` / `rgba(34,197,94,0.14)` | 同（`-light` 0.15） | Online, running |
| `--color-warning` / `-light` | `#F59E0B` / `rgba(245,158,11,0.16)` | 同（`-light` 0.15） | Degraded, pending |
| `--color-danger` / `-light` | `#EF4444` / `rgba(239,68,68,0.12)` | 同（`-light` 0.15） | Error, stopped |

Status 文字在浅色背景用 `*-600` 系（`text-red-600`），暗色背景用 `*-400` 系（`dark:text-red-400`）——这是允许的 `dark:` 场景（无语义别名）。

### Radius

| Token | Value | Semantic Alias |
|-------|-------|----------------|
| `--radius-card` | `10px` | `rounded-card` |
| `--radius-button` | `8px` | `rounded-btn` |
| `--radius-input` | `8px` | `rounded-input` |
| `--radius-tag` | `6px` | `rounded-tag` |
| `--radius-nav` | `8px` | `rounded-nav` |

### Shadows

| Token | Semantic Alias | Usage |
|-------|----------------|-------|
| `--shadow-card` | `shadow-card` | Card resting state (cards are shadowless by design; hover gains shadow) |
| `--shadow-card-hover` | `shadow-card-hover` | Card hover state |
| `--shadow-input-focus` | `shadow-input-focus` | Input focus ring |

### Typography — single source of truth

**Desktop truth = Tailwind `text-*` utilities** (defined in `tailwind.config.js`, scaled by `--ui-scale` via `useFontSize`):

| Utility | Base | Utility | Base |
|---------|------|---------|------|
| `text-xs` | 12px | `text-lg` | 18px |
| `text-sm` | 14px | `text-xl` | 20px |
| `text-base` | 16px | | |

All font sizes are `calc(Npx * var(--ui-scale))`. **Use `text-*` in templates — do not use `var(--font-size-xs…xl)`** (legacy parallel scale: 9/10/12/14/16px, deprecated, migrate on touch).

Semantic size tokens (defined in `style.css`, also `--ui-scale`-scaled):

| Token | Base | Usage |
|-------|------|-------|
| `--font-size-title` | 14px | Page/panel titles |
| `--font-size-card-title` | 14px | Card headers |
| `--font-size-body` | 13px | Body copy |
| `--font-size-label` | 12px | Form labels |
| `--font-size-tag` | 11px | Tags, meta |

### Layout

| Token | Value |
|-------|-------|
| `--sidebar-width` | `240px` |
| `--header-height` | `48px` |
| `--page-padding-x` | `24px` |
| `--page-padding-y` | `24px` |

### Component Sizes

| Token | Value |
|-------|-------|
| `--input-height` | `36px` |
| `--button-height` | `32px` |
| `--tag-height` | `24px` |
| `--nav-item-height` | `40px` |
| `--action-button-size` | `32px` |

### Palette System

Desktop supports color palettes via `data-palette` attribute on `<html>`. The default palette is "warm" (defined in `:root`). Additional palettes override all tokens via `[data-palette='xxx']` selectors:

- **warm** (default) — beige/amber tones
- **cool** — gray/blue tones, `--color-primary: #2563EB` (blue)

New palette: add a `:root[data-palette='name']` block overriding all token categories.

### Desktop Utility Classes

Defined in `style.css`, available project-wide:

| Class | Usage |
|-------|-------|
| `.wb-toolbar` | 48px toolbar with border-bottom |
| `.wb-section-title` | Uppercase section headers |
| `.wb-sidebar-section` | Sidebar group labels |
| `.wb-btn-ghost` / `.wb-btn-primary` | 28px action buttons |
| `.wb-mono` | Monospace for technical values |

---

## Mobile Tokens

Defined in `bedcode-mobile/src/styles/mobile.css`. Dark mode is default (`:root`), light mode via `html:not(.dark) .mobile-ui` / `.mobile-app`. Mobile theme is **Dracula-inspired** (near-black `#0a0a0f` base + cyan accent), separate from the desktop warm theme.

### Tailwind Semantic Aliases

Mobile's `tailwind.config.js` maps tokens to `mobile.*` namespace — prefer these in templates:

```
bg.mobile.primary     → var(--mobile-bg-primary)
bg.mobile.secondary   → var(--mobile-bg-secondary)
bg.mobile.card        → var(--mobile-bg-card)
bg.mobile.elevated    → var(--mobile-bg-elevated)
text.mobile.primary   → var(--mobile-text-primary)
text.mobile.secondary → var(--mobile-text-secondary)
text.mobile.muted     → var(--mobile-text-muted)
border.mobile         → var(--mobile-border)
```

### Backgrounds

| Token | Dark | Light |
|-------|------|-------|
| `--mobile-bg-primary` | `#0a0a0f` | `#f5f7fa` |
| `--mobile-bg-secondary` | `#12121a` | `#eef1f5` |
| `--mobile-bg-tertiary` | `#1a1a2e` | `#e4e8ee` |
| `--mobile-bg-card` | `#12121a` | `#ffffff` |
| `--mobile-bg-elevated` | `#1f2937` | `#f8f9fb` |

### Text

| Token | Dark | Light |
|-------|------|-------|
| `--mobile-text-primary` | `#ffffff` | `#1e293b` (10.7:1) |
| `--mobile-text-secondary` | `#9ca3af` | `#475569` (7.1:1) |
| `--mobile-text-muted` | `#6b7280` | `#64748b` (4.5:1) |
| `--mobile-text-disabled` | `#4b5563` | `#94a3b8` |
| `--mobile-text-on-accent` | `#0a0a0f` | `#ffffff` |

> Accent 背景上的文字**必须**用 `--mobile-text-on-accent`。

### Borders & Card Shadows

| Token | Dark | Light |
|-------|------|-------|
| `--mobile-border` | `rgba(34,211,238,0.1)` | `rgba(15,23,42,0.12)` |
| `--mobile-border-hover` | `rgba(34,211,238,0.2)` | `rgba(15,23,42,0.2)` |
| `--mobile-border-active` | `rgba(34,211,238,0.3)` | `rgba(8,145,178,0.4)` |
| `--mobile-card-shadow` / `-hover` / `-connected` | 见 mobile.css | 见 mobile.css |

### Accent & Status

| Token | Dark | Light |
|-------|------|-------|
| `--mobile-accent` | `#00d4ff` (neon cyan) | `#0891b2` |
| `--mobile-accent-muted` | `rgba(0,212,255,0.1)` | `rgba(8,145,178,0.1)` |
| `--mobile-accent-secondary` | `rgba(0,212,255,0.2)` | `rgba(8,145,178,0.18)` |
| `--mobile-success` (+`-muted`, `-connected-bg`, `-connected-border`) | `#10b981` | `#059669` |
| `--mobile-warning` (+`-muted`) | `#f59e0b` | `#d97706` |
| `--mobile-error` (+`-muted`) | `#ef4444` | `#dc2626` |

### Overlays

| Token | Usage |
|-------|-------|
| `--mobile-overlay` | Standard backdrop |
| `--mobile-overlay-heavy` | Heavy backdrop (modals) |
| `--mobile-overlay-light` | Light backdrop (tooltips) |

### Navigation & Input

| Token | Dark | Light |
|-------|------|-------|
| `--mobile-nav-bg` | `rgba(10,10,15,0.95)` | 见 mobile.css |
| `--mobile-nav-border` | `rgba(34,211,238,0.2)` | 见 mobile.css |
| `--mobile-nav-active` | `#00d4ff` | `#0891b2` |
| `--mobile-nav-inactive` | `#6b7280` | 见 mobile.css |
| `--mobile-input-bg` | `#16161e` | 见 mobile.css |
| `--mobile-input-border` | `rgba(139,233,253,0.25)` | 见 mobile.css |
| `--mobile-input-focus` | `rgba(139,233,253,0.45)` | 见 mobile.css |
| `--mobile-input-placeholder` | `#5c5c6d` | 见 mobile.css |

### Typography — fluid scale, single source of truth

**Mobile truth = `--font-size-*` tokens** (fluid `clamp()` + `--mobile-font-scale` via `useFontSize`). **Tailwind `text-*` must be mapped to these tokens** in `tailwind.config.js` — never use Tailwind's default px values on mobile:

```js
fontSize: {
  xs: 'var(--font-size-xs)', sm: 'var(--font-size-sm)', base: 'var(--font-size-base)',
  lg: 'var(--font-size-lg)', xl: 'var(--font-size-xl)',
}
```

| Token | Formula (phone → tablet) | 360px / 768px / 1200px |
|-------|---------------------------|------------------------|
| `--font-size-xs` | `clamp(0.5625rem, 0.625rem + (100vw - 360px) / 840 * 2, 0.75rem)` | 9 / 10 / 12px |
| `--font-size-sm` | `clamp(0.6875rem, 0.75rem + (100vw - 360px) / 840 * 2, 0.875rem)` | 11 / 12 / 14px |
| `--font-size-base` | `clamp(0.8125rem, 0.875rem + (100vw - 360px) / 840 * 2, 1rem)` | 13 / 14 / 16px |
| `--font-size-lg` | `clamp(0.9375rem, 1rem + (100vw - 360px) / 840 * 2, 1.125rem)` | 15 / 16 / 18px |
| `--font-size-xl` | `clamp(1.0625rem, 1.125rem + (100vw - 360px) / 840 * 2, 1.25rem)` | 17 / 18 / 20px |

All tokens wrap in `calc(... * var(--mobile-font-scale, 1))`. **Root-level tokens use `100vw` by design** (the app is full-screen) — this is the documented exception; components embedded in panels use `cqw` (see [`MOBILE.md`](MOBILE.md)).

> **Readability floor**: `--font-size-xs` bottoms out at 9px at 360px — below the ~11px industry minimum for auxiliary text. Prefer `--font-size-sm`+ for anything user-facing; raise the xs minimum on touch (code pending).

### Touch Targets — spec tokens (defined in `mobile.css`)

Constant ergonomic minimums; **never** scale with screen size:

| Token | Value | Status |
|-------|-------|--------|
| `--mobile-input-height` | `44px` | ✓ 已定义 |
| `--mobile-nav-item-height` | `44px` | ✓ 已定义 |
| `--mobile-touch-target-min` | `44px` | ✓ 已定义，TaskPickerModal 已引用 |

### Namespace Discipline

Mobile code must not reference desktop tokens (`--bg-*`, `--text-*`, `--color-*`). Current violations to migrate: `--text-tertiary` (4), `--bg-hover` (3), `--text-secondary` (2), `--color-primary-light` (2), `--color-danger-light` (2) — replace with `--mobile-*` equivalents.

### Component Token Groups (`--mobile-<component>-*`)

| Prefix | Usage | Example |
|--------|-------|---------|
| `--mobile-terminal-*` | Terminal background/header | `--mobile-terminal-bg` |
| `--mobile-code-*` | Code viewer gutter | `--mobile-code-gutter-*` |
| `--mobile-shortcut-*` | Shortcut panel (violet) | `-color`, `-bg`, `-border`, `-active-bg` |
| `--mobile-custom-cmd-*` | Custom commands (green) | 同 shortut 结构 |
| `--mobile-arrow-*` | Arrow keys (yellow) | 同上 |
| `--mobile-send-*` | Send (blue) | 同上 |
| `--mobile-execute-*` | Execute (orange) | 同上 |
| `--mobile-add-cmd-*` / `--mobile-edit-cmd-*` | Command CRUD (cyan/orange) | 同上 |
| `--mobile-danger-*` / `--mobile-confirm-*` | Destructive/confirm actions (red/green) | 同上 |
| `--mobile-input-assist-*` | Input assistant FAB (violet gradient) | `--mobile-input-assist-bg` |
| `--mobile-group-*` | Group cards | `-bg`, `-border`, `-divider`, `-row-active` |
| `--mobile-chip-*` | Icon chip tints (12% alpha bg) | `--mobile-chip-cyan`, `-cyan-bg`, `-emerald`… |
| `--mobile-badge-*` | Status badge (zinc) | `--mobile-badge-zinc-color`, `-bg` |
| `--mobile-row-*` | List row text | `-title`, `-sub`, `-value` |
| `--mobile-section-title-color` | Section headers | — |
| `--scrollbar-*` | Scrollbar | `-track-bg`, `-thumb-bg`, `-thumb-hover-bg` |

Component patterns build on these: `.group-card`, `.icon-chip + .chip-{cyan,emerald,amber,violet,red,zinc}`, `.status-badge` (all in `mobile.css`).

### Mobile Component Classes

Pre-built CSS classes in `mobile.css` for common mobile patterns:

| Class | Usage |
|-------|-------|
| `.group-section-title` | Section header text |
| `.group-card` / `.group-row` | List card + row |
| `.icon-chip` + `.chip-{cyan,emerald,amber,violet,red,zinc}` | Icon badges |
| `.status-badge` | Status pill (zinc) |
| `.status-dot` + `.dot-emerald` | With glow `box-shadow` |
| `.page-title` / `.page-subtitle` | Page headers |
| `.settings-section-title` / `.settings-group` / `.settings-row` | Settings UI |
| `.mobile-loading-overlay` / `.mobile-loading-spinner` / `.mobile-loading-text` | Shared loading state |

### Mobile Utility Classes (safe area & keyboard)

Defined in `bedcode-mobile/src/style.css` (not `mobile.css`):

| Class | Purpose |
|-------|---------|
| `mobile-header-safe` | Top padding for status bar |
| `mobile-nav-safe` | Bottom padding for home indicator |
| `mobile-input-bar` | Bottom input container with keyboard avoidance |

---

## Token Selection Flow

1. Semantic Tailwind alias (`bg-card`, `bg.mobile.card`, `rounded-btn`, `shadow-card`) — most concise
2. Raw token (`bg-[var(--mobile-bg-card)]`)
3. `color-mix()` for one-off derived values
4. New token — only when the same value appears in 3+ components
