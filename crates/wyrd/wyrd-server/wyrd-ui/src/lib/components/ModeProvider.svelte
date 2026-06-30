<script lang="ts">
  import type { Snippet } from 'svelte';
  import { theme, type Mode } from '$lib/theme.svelte';

  // `mode` pins this subtree to a fixed mode (e.g. the styleguide showing light + dark
  // side by side). Omit it to follow the app-wide `theme.mode`.
  let { mode, children }: { mode?: Mode; children?: Snippet } = $props();

  let active = $derived(mode ?? theme.mode);
</script>

<div data-mode={active} class="wy-root">
  {@render children?.()}
</div>

<style>
  .wy-root {
    /* font shorthands used by every Wyrd component, matching the source-of-truth HTML */
    --fm: var(--font-mono, 'JetBrains Mono', monospace);
    --fh: var(--font-display, 'Archivo Black', sans-serif);
    min-height: 100%;
    background: var(--bg);
    color: var(--text);
    font-family: var(--font-sans, 'Archivo', system-ui, sans-serif);
  }
</style>
