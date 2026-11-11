<!--
  One node of a Verifier graph inside Svelte Flow: a stage heading, an input
  slot, a step with its attached requirements, or a whole-Verifier
  requirement. Declared facts only; selection comes from the owning graph.
-->
<script lang="ts" module>
  import type { DefinitionInput, DefinitionRequirement, DefinitionStep } from '../verifier.js';

  /** What the owning graph shares with its nodes and edges. */
  export interface GraphSelection {
    readonly selected: string;
    select(key: string): void;
  }
  /** The context key of a graph's `GraphSelection`. */
  export const SELECTION = Symbol('wyrd-verifier-graph');

  /** The data of one graph node. */
  export type GraphNodeData =
    | { readonly type: 'stage'; readonly text: string }
    | { readonly type: 'input'; readonly node: DefinitionInput }
    | { readonly type: 'step'; readonly node: DefinitionStep; readonly needs: string }
    | { readonly type: 'req'; readonly node: DefinitionRequirement };
</script>

<script lang="ts">
  import { Handle, Position } from '@xyflow/svelte';
  import { getContext } from 'svelte';

  let { data }: { data: GraphNodeData } = $props();
  const graph = getContext<GraphSelection>(SELECTION);
</script>

{#if data.type === 'stage'}
  <div class="stage" data-stage={data.text}>{data.text}</div>
{:else}
  {@const node = data.node}
  {@const on = graph.selected === node.key}
  <div class="gn" class:sel={on}>
    {#if data.type !== 'input'}<Handle type="target" position={Position.Left} isConnectable={false} />{/if}
    <button type="button" class="main" data-node={node.key} aria-pressed={on} title={'statement' in node ? node.statement : node.detail} onclick={() => graph.select(node.key)}>
      <span class="row">
        {#if data.type === 'req'}<span class="glyph" aria-hidden="true">◇</span>{/if}
        <span class="id">{node.id}</span><span class="kind">{node.kind}</span>
      </span>
      {#if data.type === 'step'}
        <span class="row needs">{data.needs}</span>
        <span class="sub mono">{data.node.detail}</span>
        {#if data.node.chips.length}
          <span class="row">{#each data.node.chips as chip (chip)}<span class="chip">{chip}</span>{/each}</span>
        {/if}
      {:else}
        <span class="sub">{data.type === 'req' ? data.node.statement : data.node.detail}</span>
      {/if}
    </button>
    {#if data.type === 'step'}
      {#each data.node.requirements as each (each.key)}
        <button type="button" class="attached" class:sel={graph.selected === each.key} data-node={each.key} aria-pressed={graph.selected === each.key} title={each.statement} onclick={() => graph.select(each.key)}>
          <span class="glyph" aria-hidden="true">◇</span><span class="id small">{each.id}</span><span class="kind">{each.kind}</span>
        </button>
      {/each}
    {/if}
    {#if data.type !== 'req'}<Handle type="source" position={Position.Right} isConnectable={false} />{/if}
  </div>
{/if}

<style>
  .stage {
    color: var(--muted);
    font: 11px/20px var(--font-mono);
    letter-spacing: 0.06em;
    text-transform: uppercase;
    white-space: nowrap;
  }
  .gn {
    display: flex;
    flex-direction: column;
    width: 100%;
    height: 100%;
    overflow: hidden;
    border: 1px solid var(--border);
    border-radius: var(--r);
    background: var(--bg);
  }
  .gn.sel {
    border-color: var(--declare);
    background: var(--declare-soft);
    box-shadow: inset 2px 0 0 var(--declare);
  }
  button {
    display: flex;
    width: 100%;
    min-width: 0;
    border: 0;
    background: transparent;
    color: var(--text);
    font-family: var(--font-sans);
    text-align: left;
    cursor: pointer;
  }
  button:hover {
    background: var(--surface-hover);
  }
  button:focus-visible {
    outline: 2px solid var(--declare);
    outline-offset: -2px;
  }
  .main {
    flex: 1;
    flex-direction: column;
    gap: 2px;
    padding: 6px 10px;
  }
  .row {
    display: flex;
    min-width: 0;
    height: 20px;
    flex: none;
    align-items: center;
    gap: 6px;
  }
  .id {
    overflow: hidden;
    font: 13px var(--font-mono);
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .id.small {
    font-size: 12px;
  }
  .kind {
    margin-left: auto;
    border: 1px solid var(--selected-border);
    border-radius: var(--r);
    padding: 0 6px;
    background: var(--declare-soft);
    font: 10px/16px var(--font-mono);
    letter-spacing: 0.04em;
    text-transform: uppercase;
    white-space: nowrap;
  }
  .needs {
    color: var(--muted);
    font: 11px var(--font-mono);
    white-space: nowrap;
  }
  .sub {
    overflow: hidden;
    color: var(--muted);
    font-size: 12px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .sub.mono {
    font: 11px var(--font-mono);
  }
  .chip {
    border: 1px dashed var(--border);
    border-radius: var(--r);
    padding: 0 6px;
    color: var(--muted);
    font: 10px/15px var(--font-mono);
    white-space: nowrap;
  }
  .glyph {
    width: 12px;
    flex: none;
    color: var(--muted);
    font: 12px var(--font-mono);
    text-align: center;
  }
  .attached {
    height: 28px;
    flex: none;
    align-items: center;
    gap: 6px;
    border-top: 1px solid var(--border-soft);
    padding: 0 10px;
  }
  .attached.sel {
    background: var(--declare-soft);
    box-shadow: inset 2px 0 0 var(--declare);
  }
</style>
