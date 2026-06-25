<script lang="ts">
  import Self from './Tree.svelte';

  type Kind = 'client' | 'server' | 'control';
  type TreeNode = { label: string; meta?: string; kind?: Kind; children?: TreeNode[]; open?: boolean };

  let { nodes, nested = false }: { nodes: TreeNode[]; nested?: boolean } = $props();
</script>

<div class="wy-tree" class:nested>
  {#each nodes as n (n.label)}
    {#if n.children}
      <details open={n.open}>
        <summary>
          <span class="fold">{n.label}</span>
          {#if n.meta}<span class="meta">{n.meta}</span>{/if}
        </summary>
        <div class="kids"><Self nodes={n.children} nested /></div>
      </details>
    {:else}
      <div class="leaf" data-k={n.kind}>
        {n.label}
        {#if n.meta}<span class="meta">{n.meta}</span>{/if}
      </div>
    {/if}
  {/each}
</div>

<style>
  .wy-tree {
    font-family: var(--fm);
    font-size: 11.5px;
    color: var(--text);
  }
  .wy-tree :global(details) {
    margin: 0;
  }
  summary {
    list-style: none;
    cursor: pointer;
    display: flex;
    align-items: center;
    gap: 7px;
    padding: 4px 6px;
    border-radius: 4px;
  }
  summary::-webkit-details-marker {
    display: none;
  }
  summary::before {
    content: '▸';
    color: var(--muted);
    font-size: 9px;
    width: 9px;
    display: inline-block;
  }
  details[open] > summary::before {
    content: '▾';
  }
  summary:hover {
    background: var(--surface-2);
  }
  .fold {
    font-weight: 700;
  }
  .meta {
    color: var(--muted);
    font-size: 9.5px;
    margin-left: auto;
  }
  .kids {
    padding-left: 15px;
    border-left: 2px dashed var(--border);
    margin-left: 8px;
  }
  .leaf {
    display: flex;
    align-items: center;
    gap: 7px;
    padding: 4px 6px 4px 8px;
    border-left: 3px solid var(--muted);
    margin: 2px 0;
  }
  .leaf[data-k='client'] {
    border-left-color: var(--client-bar);
  }
  .leaf[data-k='server'] {
    border-left-color: var(--server-bar);
  }
  .leaf[data-k='control'] {
    border-left-color: var(--control-bar);
  }
</style>
