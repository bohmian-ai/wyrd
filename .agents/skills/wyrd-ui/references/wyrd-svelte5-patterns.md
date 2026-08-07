# Wyrd Svelte 5 Patterns

## TypeScript

- Use strict TypeScript.
- Type props, route data, endpoint responses, and component events.
- Keep feature types close to their feature until multiple features share them.
- Avoid `any`; use `unknown` at boundaries and narrow it.

## Runes

Use Svelte 5 runes intentionally:

```svelte
<script lang="ts">
  let { items = [] }: { items?: Item[] } = $props();
  let query = $state('');
  let filtered = $derived(items.filter((item) => item.name.includes(query)));
</script>
```

- Use `$state` for mutable local component state.
- Use `$derived(expression)` or `$derived.by(() => expression)` for pure derived values.
- Keep derived values side-effect free.
- Use `$effect` for external side effects: browser APIs, observers, charts, editors, timers, subscriptions, and network-triggering reactions.

## Props and snippets

- Prefer typed props over implicit object shapes.
- Use snippets for layout slots and reusable render fragments.
- Keep callback names action-oriented: `onSelect`, `onDismiss`, `onRetry`.

## Shared state

- Prefer route data and component state first.
- Use `.svelte.ts` modules for shared reactive state only when multiple components need the same live state.
- Keep persistence, browser storage, and URL synchronization isolated behind small helpers.

## Anti-patterns

- Mutating props directly.
- Putting fetch logic inside visual leaf components.
- Using `$effect` for pure computations.
- Adding global stores for route-local state.
- Hiding important state changes inside broad reactive blocks.
