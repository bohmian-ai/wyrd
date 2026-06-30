<script lang="ts">
  import type { Snippet } from 'svelte';
  import { lang as langStore, type Lang } from '$lib/lang.svelte';

  // One language panel inside a <LangTabs>. Both panels render into the DOM
  // (so prerendered HTML + Pagefind see both code variants); the inactive one
  // is hidden. `lang` here is the panel's language, not the shared store.
  let { lang, children }: { lang: Lang; children?: Snippet } = $props();
</script>

<div class="langtab" data-lang={lang} hidden={langStore.value !== lang}>
  {@render children?.()}
</div>

<style>
  .langtab :global(pre.shiki),
  .langtab :global(.cff) {
    margin-top: 0;
  }
</style>
