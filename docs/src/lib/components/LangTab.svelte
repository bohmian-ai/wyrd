<script lang="ts">
  import { getContext, type Snippet } from 'svelte';
  import type { Lang } from '$lib/lang.svelte';

  // One language panel inside a <LangTabs>. Every panel renders into the DOM
  // (so prerendered HTML + Pagefind see each code variant); inactive ones are
  // hidden. `lang` here is the panel's language; the parent block decides which
  // panel is active.
  let { lang, children }: { lang: Lang; children?: Snippet } = $props();

  const block = getContext<{ readonly active: Lang }>('wyrd:langtabs');
</script>

<div class="langtab" data-lang={lang} hidden={block.active !== lang}>
  {@render children?.()}
</div>

<style>
  .langtab :global(pre.shiki),
  .langtab :global(.cff) {
    margin-top: 0;
  }
</style>
