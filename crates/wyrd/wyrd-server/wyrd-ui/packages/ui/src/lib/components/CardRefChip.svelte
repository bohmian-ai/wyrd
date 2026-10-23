<!-- One exact Card identity, `kind name@version`; a link only when the host supplies one. -->
<script lang="ts">
  import type { CardRef } from '../contract.js';
  import { fmtCardRef } from '../format.js';

  let { cardRef, href }: { cardRef: CardRef; href?: string } = $props();
</script>

<svelte:element this={href ? 'a' : 'span'} class="chip" {href} title={fmtCardRef(cardRef)}>
  <span class="kind">{cardRef.kind}</span>
  <span class="ref">{fmtCardRef(cardRef)}</span>
</svelte:element>

<style>
  .chip {
    display: inline-flex;
    max-width: 100%;
    min-width: 0;
    align-items: center;
    gap: 6px;
    border: 1px solid var(--border);
    border-radius: var(--r);
    padding: 3px 9px;
    background: var(--surface-2);
    color: var(--text);
    font: 12px var(--font-mono);
    text-decoration: none;
  }
  a.chip:hover {
    background: var(--surface-hover);
    color: var(--declare);
  }
  .kind {
    color: var(--muted);
  }
  .ref {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>
