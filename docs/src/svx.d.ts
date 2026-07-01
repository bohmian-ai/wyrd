// Ambient type for mdsvex modules imported directly or via the content glob.
// Kept in its own file with no top-level import/export so the `declare module`
// is global, not module-scoped.
declare module '*.svx' {
  import type { Component } from 'svelte';
  const component: Component;
  export default component;
  export const metadata: Record<string, unknown>;
}
