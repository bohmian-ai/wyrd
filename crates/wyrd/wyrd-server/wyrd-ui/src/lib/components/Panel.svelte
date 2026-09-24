<script lang="ts">
  import type { Snippet } from 'svelte';

  // The workbench container. `Panel`, not `Card`: a Wyrd Card is a registered domain
  // record, and a panel is only a surface it may be drawn on. Altitude is the panel's
  // job in the page — quiet is the default, raised marks the single attention surface,
  // flat is a subordinate block, wide spans the grid.
  // `accent` marks the page's single dominant work region with a brand-tinted head —
  // at most one accent panel per page, the way GitHub treats its merge box.
  type Variant = 'quiet' | 'raised' | 'flat' | 'wide' | 'accent';
  let {
    variant = 'quiet',
    title,
    head,
    children
  }: {
    variant?: Variant;
    title?: string;
    head?: Snippet;
    children?: Snippet;
  } = $props();
</script>

<section class="wy-panel" data-variant={variant}>
  {#if title || head}
    <header class="wy-panel-head">
      {#if title}<span class="t">{title}</span>{/if}
      {@render head?.()}
    </header>
  {/if}
  <div class="wy-panel-body">{@render children?.()}</div>
</section>

<style>
  .wy-panel {
    border: 1px solid var(--border);
    border-radius: var(--r);
    background: var(--surface);
    overflow: hidden;
    container-type: inline-size;
  }
  .wy-panel[data-variant='raised'] {
    border-color: var(--text);
  }
  .wy-panel[data-variant='flat'] {
    border-color: var(--border-soft);
  }
  .wy-panel[data-variant='wide'] {
    grid-column: 1 / -1;
  }
  /* GitHub-style anatomy: a tinted, bold header band over a plain body. The band,
     not the copy inside, is what separates panels from one another. */
  .wy-panel-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    flex-wrap: wrap;
    padding: 10px 14px;
    border-bottom: 1px solid var(--border);
    background: var(--surface-2);
    font: 700 13px var(--font-sans);
    color: var(--text);
  }
  /* accent = the page's dominant panel: a declare-soft head over a plain body. */
  .wy-panel[data-variant='accent'] .wy-panel-head {
    background: var(--declare-soft);
  }
  .wy-panel-body {
    padding: 16px;
  }
</style>
