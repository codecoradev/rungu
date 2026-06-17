# Keyboard Shortcuts

Rungu ships a small set of keyboard shortcuts for power users. Shortcuts are **disabled while typing** in any text field, and every shortcut is also reachable via a visible button — they supplement, never replace, the UI.

Press <kbd>?</kbd> anywhere in the app to open the in-app shortcut reference (auto-generated from the registry, so it never drifts from what's actually wired up).

## Global

| Key | Action |
|-----|--------|
| <kbd>?</kbd> | Open / close the shortcut help overlay |
| <kbd>Esc</kbd> | Close any modal or clear focus |

## Board (`/board/{slug}`)

| Key | Action | Notes |
|-----|--------|-------|
| <kbd>/</kbd> | Focus the search box | |
| <kbd>c</kbd> | Toggle the new-post composer | Login required |
| <kbd>j</kbd> | Focus the next post | Wraps at the last post |
| <kbd>k</kbd> | Focus the previous post | Wraps at the first post |
| <kbd>Enter</kbd> | Open the focused post | Navigates to post detail |
| <kbd>v</kbd> | Toggle your vote on the focused post | Login required |

## Post detail (`/board/{slug}/post/{id}`)

| Key | Action | Notes |
|-----|--------|-------|
| <kbd>r</kbd> | Focus the reply box | Login required |

## How it works

- All bindings live in a single [registry](https://github.com/codecoradev/rungu/blob/develop/web/src/lib/shortcuts.ts) — `web/src/lib/shortcuts.ts`.
- A global keydown handler in `+layout.svelte` resolves the active scope from the current URL, then either handles the shortcut directly (global) or dispatches a `rungu:shortcut` CustomEvent that the active page listens for.
- Shortcuts ignore events with <kbd>Ctrl</kbd>/<kbd>Cmd</kbd>/<kbd>Alt</kbd> held — we only own bare-key bindings, and we don't want to clobber browser or assistive-tech shortcuts.

## Accessibility

- Shortcuts never replace visible controls. Every action above is also available as a button or link.
- The handler skips events when an `<input>`, `<textarea>`, `<select>`, `[contenteditable]`, or `role="textbox"` element has focus — so screen-reader users typing into fields are never surprised.
- We deliberately avoid <kbd>Ctrl</kbd>/<kbd>Cmd</kbd> combinations and single-letter keys that conflict with common AT navigation when no field is focused.

## Customization

Not supported in v0.2 — every binding is fixed in the registry. If you need custom or app-wide rebindable shortcuts, please open an issue describing your use case.
