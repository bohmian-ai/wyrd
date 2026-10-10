<script lang="ts">
  import { setContext, type Snippet } from 'svelte';
  import { lang as langStore, LANGS, type Lang } from '$lib/lang.svelte';

  // The multi-language device: a segmented control above mirrored code panels
  // (<LangTab>). `langs` names the panels this block has, e.g.
  // `<LangTabs langs={['python', 'rust', 'typescript']}>`. The choice is shared
  // and persisted, so flipping it here flips every block on every page; a block
  // without the reader's language shows its first panel. Prose stays neutral;
  // only code forks.
  let { langs, children }: { langs: Lang[]; children?: Snippet } = $props();

  const offered = $derived(LANGS.filter((l) => langs.includes(l.id)));
  const active = $derived(langs.includes(langStore.value) ? langStore.value : langs[0]);

  setContext('wyrd:langtabs', {
    get active() {
      return active;
    }
  });
</script>

<div class="langtabs">
  <div class="lt-strip" role="tablist" aria-label="Language">
    {#each offered as l (l.id)}
      <button
        type="button"
        role="tab"
        aria-selected={active === l.id}
        class:active={active === l.id}
        onclick={() => langStore.set(l.id)}
      >
        {l.label}
      </button>
    {/each}
  </div>
  <div class="lt-panels">
    {@render children?.()}
  </div>
</div>

<style>
  .langtabs {
    margin: 1.5rem 0;
  }
  .lt-strip {
    display: inline-flex;
    gap: 0;
    border: 1px solid var(--border);
    border-radius: var(--r);
    overflow: hidden;
    margin-bottom: 12px;
    background: var(--surface);
  }
  .lt-strip button {
    font-size: 13px;
    font-weight: 600;
    color: var(--muted);
    background: var(--surface);
    border: 0;
    border-right: 1px solid var(--border-soft);
    padding: 7px 16px;
    cursor: pointer;
  }
  .lt-strip button:last-child {
    border-right: 0;
  }
  .lt-strip button:hover {
    color: var(--text);
    background: var(--surface-2);
  }
  .lt-strip button.active {
    color: var(--text);
    background: var(--declare-soft);
  }
  /* Phone: taller tap target on the language toggle. */
  @media (max-width: 640px) {
    .lt-strip button {
      padding: 11px 20px;
    }
  }
</style>
