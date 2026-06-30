<script lang="ts">
  import type { Snippet } from 'svelte';
  import { setContext } from 'svelte';

  // Generic tabs. Children are <Tab label="..."> panels. The active tab is
  // tracked here; Tab children register themselves via context and are hidden
  // when inactive. SSR/prerender-safe: defaults to the first registered tab,
  // no browser API at the top level.
  let { children }: { children?: Snippet } = $props();

  let tabs = $state<Array<{ id: string; label: string }>>([]);
  let activeId = $state('');

  const ctx = {
    get activeId() {
      return activeId;
    },
    register(id: string, label: string): void {
      if (!tabs.find((t) => t.id === id)) {
        tabs.push({ id, label });
        if (!activeId) activeId = id;
      }
    },
    activate(id: string): void {
      activeId = id;
    }
  };

  setContext('wyrd-tabs', ctx);
</script>

<div class="tabs">
  {#if tabs.length > 0}
    <div class="tabs-strip" role="tablist">
      {#each tabs as t (t.id)}
        <button
          type="button"
          role="tab"
          aria-selected={activeId === t.id}
          class:active={activeId === t.id}
          onclick={() => ctx.activate(t.id)}
        >
          {t.label}
        </button>
      {/each}
    </div>
  {/if}
  {@render children?.()}
</div>

<style>
  .tabs {
    margin: 1.5rem 0;
  }
  .tabs-strip {
    display: inline-flex;
    gap: 0;
    border: 2px solid var(--border);
    border-radius: var(--r);
    box-shadow: 3px 3px 0 0 var(--shadow);
    overflow: hidden;
    margin-bottom: 12px;
    background: var(--surface);
  }
  .tabs-strip button {
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
  .tabs-strip button:last-child {
    border-right: 0;
  }
  .tabs-strip button:hover {
    color: var(--text);
    background: var(--surface-2);
  }
  .tabs-strip button.active {
    color: var(--text);
    background: var(--rune-soft);
  }
  @media (max-width: 640px) {
    .tabs-strip button {
      padding: 11px 20px;
    }
  }
</style>
