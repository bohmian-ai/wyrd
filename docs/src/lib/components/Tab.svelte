<script lang="ts">
  import type { Snippet } from 'svelte';
  import { getContext, untrack } from 'svelte';

  // One panel inside a <Tabs>. Registers itself with the parent's context and
  // is hidden when not the active tab. Both panels stay in the DOM so
  // prerendered HTML and Pagefind index all content variants.
  let { label, children }: { label: string; children?: Snippet } = $props();

  // Tab labels are intentionally static — `untrack` makes that explicit and
  // suppresses the Svelte reactive-capture warning without changing semantics.
  const id = untrack(() => label.toLowerCase().replace(/\s+/g, '-'));

  const ctx = getContext<{
    activeId: string;
    register: (id: string, label: string) => void;
    activate: (id: string) => void;
  }>('wyrd-tabs');

  ctx.register(id, untrack(() => label));
</script>

<div class="tab-panel" role="tabpanel" hidden={ctx.activeId !== id}>
  {@render children?.()}
</div>

<style>
  .tab-panel :global(pre.shiki),
  .tab-panel :global(.cb) {
    margin-top: 0;
  }
</style>
