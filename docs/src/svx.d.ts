// Ambient type for mdsvex modules imported directly (not via the content glob),
// e.g. the home route's `$lib/HomeContent.svx`. Kept in its own file with no
// top-level import/export so the `declare module` is global, not module-scoped.
declare module '*.svx' {
  import type { Component } from 'svelte';
  const component: Component;
  export default component;
  export const metadata: Record<string, unknown>;
}
