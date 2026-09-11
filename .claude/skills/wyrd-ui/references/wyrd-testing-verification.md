# Wyrd Testing And Verification

## Choosing the check

- Pure utilities: add unit tests.
- Components with interaction: use component tests when the test harness exists.
- Routes and server helpers: test parsing, auth/session branches, and error normalization.
- Visual changes: run build/checks and inspect the UI manually in relevant modes.

## Verification commands

From `crates/wyrd/wyrd-server/wyrd-ui`:

```bash
pnpm check
pnpm build
```

Run both when changing route data, TypeScript contracts, Svelte components, or shared styling. For tiny copy-only changes, use judgment.

## Manual UI checks

- Text fits inside buttons, nav items, cards, and narrow mobile layouts.
- Focus is visible.
- Loading, empty, and error states are reachable.
- Theme tokens resolve correctly in both light and dark modes when theme support exists.
- Brutalist rules hold: zero radius, 2px borders, hard-offset zero-blur shadows.
- Controls do not shift layout when labels, counts, or icons change.

## Performance checks

- Large lists use bounded payloads and virtualization when needed.
- Filters do not trigger unbounded request bursts.
- Charts and editors clean up observers and subscriptions.
- Components avoid expensive work during every render.
