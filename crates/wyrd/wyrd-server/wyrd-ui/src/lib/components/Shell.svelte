<script lang="ts">
  import type { Snippet } from 'svelte';

  let {
    sidebar,
    topbar,
    children
  }: { sidebar?: Snippet; topbar?: Snippet; children?: Snippet } = $props();
</script>

<div class="wy-shell">
  <div class="shell-inner">
    {#if sidebar}<aside class="shell-side">{@render sidebar()}</aside>{/if}
    <div class="shell-main">
      {#if topbar}<header class="shell-top">{@render topbar()}</header>{/if}
      <div class="shell-content">{@render children?.()}</div>
    </div>
  </div>
</div>

<style>
  .wy-shell {
    border: 3px solid var(--border);
    border-radius: var(--r);
    background: var(--bg);
    overflow: hidden;
    color: var(--text);
    /* Adapt to our own width, not the viewport: robust inside cards/panels/A2UI layouts. */
    container-type: inline-size;
  }
  .shell-inner {
    display: flex;
  }
  .shell-side {
    flex: 0 0 212px;
    border-right: 2px solid var(--border);
    background: var(--surface);
  }
  .shell-main {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
  }
  .shell-top {
    border-bottom: 2px solid var(--border);
    background: var(--surface);
  }
  .shell-content {
    flex: 1;
    min-width: 0;
    padding: 16px;
  }

  /* Below the workbench floor: stack the sidebar above the content instead of clipping. */
  @container (max-width: 720px) {
    .shell-inner {
      flex-direction: column;
    }
    .shell-side {
      flex: 0 0 auto;
      border-right: 0;
      border-bottom: 2px solid var(--border);
    }
  }
</style>
