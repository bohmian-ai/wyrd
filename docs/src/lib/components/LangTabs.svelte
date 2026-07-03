<script lang="ts">
  import type { Snippet } from 'svelte';
  import { lang as langStore, LANGS } from '$lib/lang.svelte';

  // The dual-language device: a Python/Rust segmented control above two mirrored
  // code panels (<LangTab>). The choice is shared+persisted, so flipping it here
  // flips every LangTabs block on every page. Prose stays neutral; only code
  // forks. See the authoring API in the header of this file's component docs.
  let { children }: { children?: Snippet } = $props();
</script>

<div class="langtabs">
  <div class="lt-strip" role="tablist" aria-label="Language">
    {#each LANGS as l (l.id)}
      <button
        type="button"
        role="tab"
        aria-selected={langStore.value === l.id}
        class:active={langStore.value === l.id}
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
    border: 2px solid var(--border);
    border-radius: var(--r);
    box-shadow: 3px 3px 0 0 var(--shadow);
    overflow: hidden;
    margin-bottom: 12px;
    background: var(--surface);
  }
  .lt-strip button {
    font-family: var(--font-mono);
    font-size: 0.72rem;
    font-weight: 700;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    color: var(--muted);
    background: var(--surface);
    border: 0;
    border-right: 2px solid var(--border);
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
    background: var(--rune-soft);
  }
  /* Phone: taller tap target on the language toggle. */
  @media (max-width: 640px) {
    .lt-strip button {
      padding: 11px 20px;
    }
  }
</style>
