<script module>
  // Re-export the full component set so every .svx/.md page can use them without
  // per-page import blocks. MDsveX merges this module scope into the compiled
  // markdown file, putting all names in scope for content authors.
  //
  // CodeBlock is also re-exported here because the svelte.config.js highlighter
  // emits <CodeBlock html={...} /> for every fenced code fence — it must be in
  // scope in every processed file.
  export { default as CodeBlock } from '$lib/mdsvex/CodeBlock.svelte';
  export { default as Aside } from '$lib/components/Aside.svelte';
  export { default as LangTabs } from '$lib/components/LangTabs.svelte';
  export { default as LangTab } from '$lib/components/LangTab.svelte';
  export { default as CodeFromFile } from '$lib/components/CodeFromFile.svelte';
  export { default as CardTileGrid } from '$lib/components/CardTileGrid.svelte';
  export { default as CardTile } from '$lib/components/CardTile.svelte';
  export { default as Pagination } from '$lib/components/Pagination.svelte';
  export { default as Toc } from '$lib/components/Toc.svelte';
  export { default as Tabs } from '$lib/components/Tabs.svelte';
  export { default as Tab } from '$lib/components/Tab.svelte';
  export { default as Steps } from '$lib/components/Steps.svelte';
</script>

<script lang="ts">
  import type { Snippet } from 'svelte';

  // Passthrough layout — content renders as-is; this file's sole job is to
  // expose the component set via the module script above. SSR/prerender-safe:
  // no top-level browser API calls, no onMount, no browser globals.
  let { children }: { children?: Snippet } = $props();
</script>

{@render children?.()}
