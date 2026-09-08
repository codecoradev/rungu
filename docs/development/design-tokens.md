# Design Tokens

Rungu's web UI uses a semantic token system defined in `web/src/app.css`. All v0.5.0 board components must consume tokens instead of one-off values.

## Tiers

| Tier | Example | Rule |
|------|---------|------|
| Semantic (colors) | `--color-primary`, `--color-status-planned`, `--color-muted-foreground` | The only colors components may reference. Defined for light **and** `.dark`. |
| Component (board) | `--vote-rail-width`, `--board-row-pad-y`, `--sort-tab-active-bg` | Component-scoped values for the board surface. May alias semantic tokens. |
| Type roles | `--board-title-size`, `--board-meta-size` | Named roles mapped onto the existing Tailwind text scale. |

Primitives (raw OKLCH values) live **only** inside the token definitions in `app.css` — never in components.

## Board component tokens (#199)

### Card-list row (used by the post list)

| Token | Light | Purpose |
|-------|-------|---------|
| `--board-row-min-height` | `7rem` | Minimum row height (density target: ≥ 4.5 rows at 1440×900) |
| `--board-row-pad-y` | `0.875rem` | Row vertical padding |
| `--board-row-pad-x` | `1rem` | Row horizontal padding |
| `--board-row-divider-color` | `var(--color-border)` | Hairline between rows |

### Vote rail

| Token | Light | Purpose |
|-------|-------|---------|
| `--vote-rail-width` | `3.5rem` | Rail column width (56px) |
| `--vote-target-min` | `2.75rem` | Minimum tap target, 44px HIG minimum |
| `--vote-count-size` | `1.125rem` | Vote count font size (18px semibold) |
| `--vote-voted-bg` / `--vote-voted-fg` / `--vote-voted-border` | primary tints | Voted state |
| `--vote-hover-bg` | neutral tint | Hover state |
| `--vote-rail-divider-color` | `var(--color-border)` | Vertical divider between rail and content |

### Sort tabs

| Token | Light | Purpose |
|-------|-------|---------|
| `--sort-tab-height` | `2.25rem` | Tab height (36px desktop; 44px on mobile hit areas) |
| `--sort-tab-radius` | `9999px` | Pill shape |
| `--sort-tab-active-bg` / `--sort-tab-active-fg` | `var(--color-primary)` / `var(--color-primary-foreground)` | Selected state (must be obvious in both themes) |
| `--sort-tab-inactive-fg` | `var(--color-muted-foreground)` | Unselected text |
| `--sort-tab-inactive-hover-bg` | neutral tint | Unselected hover |

### Type roles

| Token | Value | Role |
|-------|-------|------|
| `--board-title-size` | `1rem` | Post title on cards (pair with `font-semibold`) |
| `--board-body-size` | `0.875rem` | Clamped post body |
| `--board-meta-size` | `0.75rem` | Author/time/comment meta row |
| `--board-section-label-size` | `0.75rem` | Uppercase section labels |

## Rules for contributors

1. Reference tokens via Tailwind arbitrary values, e.g. `h-[var(--vote-target-min)]` — or map new utilities in the `@theme inline` block if a value is reused across components.
2. Never hardcode OKLCH, px, or hex values in `.svelte` files; add a token instead.
3. Every new token needs a light **and** dark value (or must alias a token that adapts).
4. Contrast: body text ≥ 4.5:1, large text/UI ≥ 3:1 (WCAG AA), in both themes.
5. Interactive targets: ≥ 44×44px on touch surfaces.

## Verification

- `grep -rnE '#[0-9a-fA-F]{3,8}|oklch\(' web/src --include='*.svelte'` should return no component-level color literals.
- Light/dark screenshot parity is checked per feature PR (playwright recipe: two contexts with `color_scheme='light'` / `'dark'`).
