# Token Catalog

Complete design token reference for BedCode. Consult when choosing a specific token.

## Desktop Tokens

Defined in `bedcode-desktop/src/style.css`.

### Backgrounds

| Token | Light | Dark | Semantic Alias |
|-------|-------|------|----------------|
| `--bg-page` | `#F5F4F0` | `#15130F` | `bg-page` |
| `--bg-sidebar` | `#EFEDE7` | `#1A1814` | `bg-sidebar` |
| `--bg-card` | `#FDFCFA` | `#1F1D18` | `bg-card` |
| `--bg-hover` | `#E7E4DC` | `#2A2720` | — |
| `--bg-input` | `#FDFCFA` | `#1F1D18` | — |

### Text

| Token | Light | Dark |
|-------|-------|------|
| `--text-primary` | `#26231B` | `#ECE8DC` |
| `--text-secondary` | `#7C7565` | `#A09880` |
| `--text-tertiary` | `#ABA492` | `#6B6452` |

### Borders

| Token | Light | Dark |
|-------|-------|------|
| `--border` | `#E2DDD2` | `#2A2720` |
| `--border-strong` | `#CCC6B8` | `#3D3830` |
| `--border-input` | `#DDD8CC` | `#33302A` |

### Brand

| Token | Light | Dark | Semantic Alias |
|-------|-------|------|----------------|
| `--color-primary` | `#1D1A14` | `#ECE8DC` | `brand` |
| `--color-primary-hover` | `#33302A` | `#D4D0C4` | — |
| `--color-primary-light` | `rgba(29,26,20,0.08)` | `rgba(236,232,220,0.1)` | `brand-light` |
| `--color-primary-contrast` | `#FFFFFF` | `#1D1A14` | — |

### Status Colors

| Token | Light | Dark | Usage |
|-------|-------|------|-------|
| `--color-success` / `-light` | green | green | Online, running |
| `--color-warning` / `-light` | amber | amber | Degraded, pending |
| `--color-danger` / `-light` | red | red | Error, stopped |

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
| `--shadow-card` | `shadow-card` | Card resting state |
| `--shadow-card-hover` | `shadow-card-hover` | Card hover state |
| `--shadow-input-focus` | `shadow-input-focus` | Input focus ring |

### Typography

All font sizes use `calc(Npx * var(--ui-scale))` for dynamic scaling via the `useFontSize` composable.

| Token | Base | Semantic Alias |
|-------|------|----------------|
| `--font-size-xs` | 11px | — |
| `--font-size-sm` | 12px | — |
| `--font-size-base` | 13px | — |
| `--font-size-lg` | 14px | — |
| `--font-size-xl` | 16px | — |
| `--font-size-title` | 16px | — |
| `--font-size-card-title` | 14px | — |
| `--font-size-body` | 13px | — |
| `--font-size-label` | 12px | — |
| `--font-size-tag` | 11px | — |

### Layout

| Token | Value |
|-------|-------|
| `--sidebar-width` | `240px` |
| `--header-height` | `48px` |
| `--page-padding-x` | `24px` |
| `--page-padding-y` | `20px` |

### Component Sizes

| Token | Value |
|-------|-------|
| `--input-height` | `36px` |
| `--button-height` | `32px` |
| `--tag-height` | `24px` |
| `--action-button-size` | `28px` |

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

Defined in `bedcode-mobile/src/styles/mobile.css`. Dark mode is default (`:root`), light mode via `html:not(.dark) .mobile-ui`.

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
| `--mobile-bg-primary` | `#0f172a` | `#f8fafc` |
| `--mobile-bg-secondary` | `#1e293b` | `#f1f5f9` |
| `--mobile-bg-tertiary` | `#334155` | `#e2e8f0` |
| `--mobile-bg-card` | `#1e293b` | `#ffffff` |
| `--mobile-bg-elevated` | `#334155` | `#f1f5f9` |

### Text

| Token | Dark | Light |
|-------|------|-------|
| `--mobile-text-primary` | `#e2e8f0` | `#1e293b` |
| `--mobile-text-secondary` | `#94a3b8` | `#475569` |
| `--mobile-text-muted` | `#64748b` | `#94a3b8` |
| `--mobile-text-disabled` | `#475569` | `#cbd5e1` |
| `--mobile-text-on-accent` | `#0f172a` | `#ffffff` |

### Borders

| Token | Dark | Light |
|-------|------|-------|
| `--mobile-border` | `#334155` | `#e2e8f0` |
| `--mobile-border-hover` | `#475569` | `#cbd5e1` |
| `--mobile-border-active` | `#00d4ff` | `#0891b2` |

### Accent (Brand)

| Token | Dark | Light |
|-------|------|-------|
| `--mobile-accent` | `#00d4ff` (neon cyan) | `#0891b2` (deep cyan) |
| `--mobile-accent-muted` | `rgba(0,212,255,0.1)` | `rgba(8,145,178,0.08)` |
| `--mobile-accent-secondary` | `#a78bfa` | `#7c3aed` |

### Status

| Token | Dark | Light | Usage |
|-------|------|-------|-------|
| `--mobile-success` | `#34d399` | `#10b981` | Online, connected |
| `--mobile-success-muted` | `rgba(52,211,153,0.1)` | `rgba(16,185,129,0.08)` | — |
| `--mobile-warning` | `#fbbf24` | `#f59e0b` | Degraded |
| `--mobile-warning-muted` | `rgba(251,191,36,0.1)` | `rgba(245,158,11,0.08)` | — |
| `--mobile-error` | `#f87171` | `#ef4444` | Error, disconnected |
| `--mobile-error-muted` | `rgba(248,113,113,0.1)` | `rgba(239,68,68,0.08)` | — |

### Overlays

| Token | Usage |
|-------|-------|
| `--mobile-overlay` | Standard backdrop |
| `--mobile-overlay-heavy` | Heavy backdrop (modals) |
| `--mobile-overlay-light` | Light backdrop (tooltips) |

### Navigation

| Token | Dark | Light |
|-------|------|-------|
| `--mobile-nav-bg` | `rgba(15,23,42,0.95)` | `rgba(248,250,252,0.95)` |
| `--mobile-nav-border` | `#1e293b` | `#e2e8f0` |
| `--mobile-nav-active` | `#00d4ff` | `#0891b2` |
| `--mobile-nav-inactive` | `#64748b` | `#94a3b8` |

### Input

| Token | Dark | Light |
|-------|------|-------|
| `--mobile-input-bg` | `#1e293b` | `#f1f5f9` |
| `--mobile-input-border` | `#334155` | `#cbd5e1` |
| `--mobile-input-focus` | `#00d4ff` | `#0891b2` |
| `--mobile-input-placeholder` | `#64748b` | `#94a3b8` |

### Component-Specific Token Groups

Mobile defines specialized token groups for component domains:

| Prefix | Usage | Example |
|--------|-------|---------|
| `--mobile-shortcut-*` | Shortcut panel buttons | `--mobile-shortcut-bg`, `-text`, `-border`, `-active` |
| `--mobile-arrow-*` | Arrow/navigation buttons | `--mobile-arrow-bg`, `-text`, `-border` |
| `--mobile-custom-cmd-*` | Custom command buttons | `--mobile-custom-cmd-bg`, `-text`, `-border` |
| `--mobile-group-*` | Group cards and rows | `--mobile-group-card-bg`, `-border`, `-shadow` |
| `--mobile-icon-chip-*` | Icon chip colors | `--mobile-icon-chip-bg`, `-text`, `-border` |
| `--mobile-code-*` | Code viewer | `--mobile-code-bg`, `-line-highlight` |
| `--mobile-env-*` | Environment tags | `--mobile-env-wsl`, `-win` |

### Mobile Component Classes

Pre-built CSS classes in `mobile.css` for common mobile patterns:

| Class | Usage |
|-------|-------|
| `.group-section-title` | Section header text |
| `.group-card` / `.group-row` | List card + row |
| `.icon-chip` + `.chip-{cyan,emerald,amber,violet,red,zinc}` | Icon badges |
| `.status-badge` + `.badge-{emerald,amber,cyan,zinc}` | Status pills |
| `.status-dot` + `.dot-emerald` | With glow `box-shadow` |
| `.env-tag` + `.env-wsl` / `.env-win` | Environment labels (uses `color-mix()`) |
| `.page-title` / `.page-subtitle` | Page headers |
| `.settings-section-title` / `.settings-group` / `.settings-row` | Settings UI |
| `.mobile-loading-overlay` / `.mobile-loading-spinner` / `.mobile-loading-text` | Shared loading state |

### Mobile Font Sizes

Mobile uses fixed `px` values (no `--ui-scale`):

| Token | Value |
|-------|-------|
| `--font-size-xs` | 10px |
| `--font-size-sm` | 12px |
| `--font-size-base` | 14px |
| `--font-size-lg` | 16px |
| `--font-size-xl` | 18px |

Component-level responsive scaling uses `clamp()` — see [`MOBILE.md`](MOBILE.md).
