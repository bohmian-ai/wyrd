<script lang="ts">
  import type { Snippet } from 'svelte';

  // The workbench container. `Panel`, not `Card`: a Wyrd Card is a registered domain
  // record, and a panel is only a surface it may be drawn on. Altitude is the panel's
  // job in the page — quiet is the default, raised marks the single attention surface,
  // flat is a subordinate block, wide spans the grid.
  type Variant = 'quiet' | 'raised' | 'flat' | 'wide';
  type Owner = 'wyrd' | 'fathom';
  let {
    variant = 'quiet',
    owner = 'wyrd',
    title,
    head,
    children
  }: {
    variant?: Variant;
    owner?: Owner;
    title?: string;
    head?: Snippet;
    children?: Snippet;
  } = $props();
</script>

<section class="wy-panel" data-variant={variant} data-owner={owner}>
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
    border: 2px solid var(--border);
    border-radius: var(--r);
    background: var(--surface);
    box-shadow: 3px 3px 0 0 var(--shadow);
    overflow: hidden;
    container-type: inline-size;
  }
  .wy-panel[data-variant='raised'] {
    box-shadow: 6px 6px 0 0 var(--shadow);
  }
  .wy-panel[data-variant='flat'] {
    box-shadow: none;
  }
  .wy-panel[data-variant='wide'] {
    grid-column: 1 / -1;
  }
  .wy-panel-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    flex-wrap: wrap;
    padding: 8px 11px;
    border-bottom: 2px solid var(--border);
    font-family: var(--fm);
    font-size: 9px;
    font-weight: 700;
    letter-spacing: 0.5px;
    text-transform: uppercase;
    color: var(--muted);
  }
  /* owner=fathom tints the head, marking the content as Fathom-authored. Its label stays
     --text, NOT --ink-on-fill: the band lands on a dark olive in dark mode. */
  .wy-panel[data-owner='fathom'] .wy-panel-head {
    background: color-mix(in srgb, var(--lime) 26%, var(--surface));
    color: var(--text);
  }
  .wy-panel-body {
    padding: 12px;
  }
</style>
