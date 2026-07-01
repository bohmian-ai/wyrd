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
  // Tab button elements, keyed by id, so arrow-key navigation can move focus to
  // the newly-activated tab (roving tabindex per the WAI-ARIA tabs pattern).
  let btns = $state<Record<string, HTMLButtonElement>>({});

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

  // Automatic activation: Left/Right wrap-move and Home/End jump along the
  // tablist, activating and focusing the target tab. The buttons all stay in the
  // DOM, so focusing the target is synchronous after activate().
  function onKeydown(e: KeyboardEvent): void {
    const handled = ['ArrowRight', 'ArrowLeft', 'Home', 'End'];
    if (!handled.includes(e.key)) return;
    e.preventDefault();
    const i = tabs.findIndex((t) => t.id === activeId);
    let next = i;
    if (e.key === 'ArrowRight') next = (i + 1) % tabs.length;
    else if (e.key === 'ArrowLeft') next = (i - 1 + tabs.length) % tabs.length;
    else if (e.key === 'Home') next = 0;
    else if (e.key === 'End') next = tabs.length - 1;
    const id = tabs[next].id;
    ctx.activate(id);
    btns[id]?.focus();
  }
</script>

<div class="tabs">
  {#if tabs.length > 0}
    <div class="tabs-strip" role="tablist">
      {#each tabs as t (t.id)}
        <button
          type="button"
          role="tab"
          id={`tab-${t.id}`}
          aria-controls={`tabpanel-${t.id}`}
          aria-selected={activeId === t.id}
          tabindex={activeId === t.id ? 0 : -1}
          class:active={activeId === t.id}
          bind:this={btns[t.id]}
          onclick={() => ctx.activate(t.id)}
          onkeydown={onKeydown}
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
