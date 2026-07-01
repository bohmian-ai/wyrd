<script lang="ts">
  import type { Snippet } from 'svelte';

  // Passthrough layout — content renders as-is. The authoring component set is
  // supplied per-page as bare imports by the `injectMdsvexComponents`
  // preprocessor (see svelte.config.js / $lib/mdsvex/components.js), not via
  // mdsvex layout-module re-exports. SSR/prerender-safe: no browser globals.
  let { children }: { children?: Snippet } = $props();
</script>

{@render children?.()}
