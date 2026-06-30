<script lang="ts">
  import { onMount } from 'svelte';
  import { base } from '$app/paths';

  // Bespoke Pagefind search: a brutalist arcade modal over the Pagefind JS API
  // (loaded from /pagefind/pagefind.js at the base path). Pagefind only indexes
  // the built site, so search is inert in `vite dev` — the modal degrades to a
  // notice. Cmd/Ctrl+K opens; Esc closes.
  type Result = { url: string; title: string; excerpt: string };

  let open = $state(false);
  let query = $state('');
  let results = $state<Result[]>([]);
  let unavailable = $state(false);

  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  let pf: any = null;
  let inputEl = $state<HTMLInputElement>();
  let timer: ReturnType<typeof setTimeout> | undefined;

  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  async function ensure(): Promise<any> {
    if (pf) return pf;
    try {
      const mod = await import(/* @vite-ignore */ `${base}/pagefind/pagefind.js`);
      await mod.init?.();
      pf = mod;
      return pf;
    } catch {
      unavailable = true;
      return null;
    }
  }

  function resultHref(url: string): string {
    // Pagefind records build-root-relative urls (e.g. `/cards/`); the deployed
    // site lives under `base`, so prefix unless it's already there.
    if (base && !url.startsWith(base)) return `${base}${url}`;
    return url;
  }

  async function run(): Promise<void> {
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const lib: any = await ensure();
    if (!lib || !query.trim()) {
      results = [];
      return;
    }
    const search = await lib.search(query);
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const data = await Promise.all(search.results.slice(0, 8).map((r: any) => r.data()));
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    results = data.map((d: any) => ({
      url: resultHref(d.url),
      title: d.meta?.title ?? d.url,
      excerpt: d.excerpt ?? ''
    }));
  }

  function onInput(): void {
    clearTimeout(timer);
    timer = setTimeout(run, 160);
  }

  function show(): void {
    open = true;
    queueMicrotask(() => inputEl?.focus());
  }
  function hide(): void {
    open = false;
  }

  onMount(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === 'k') {
        e.preventDefault();
        open ? hide() : show();
      } else if (e.key === 'Escape' && open) {
        hide();
      }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  });
</script>

<button class="search-trigger" type="button" onclick={show} aria-label="Search">
  <span class="st-icon" aria-hidden="true">⌕</span>
  <span class="st-text">Search</span>
  <kbd class="st-kbd">⌘K</kbd>
</button>

{#if open}
  <div
    class="search-overlay"
    role="button"
    tabindex="-1"
    aria-label="Close search"
    onclick={hide}
    onkeydown={(e) => e.key === 'Enter' && hide()}
  ></div>
  <div class="search-modal" role="dialog" aria-modal="true" aria-label="Search docs">
    <div class="sm-bar">
      <span class="sm-icon" aria-hidden="true">⌕</span>
      <!-- svelte-ignore a11y_autofocus -->
      <input
        bind:this={inputEl}
        bind:value={query}
        oninput={onInput}
        type="search"
        placeholder="Search the docs…"
        autocomplete="off"
        spellcheck="false"
      />
      <button class="sm-close" type="button" onclick={hide} aria-label="Close">ESC</button>
    </div>
    <div class="sm-results">
      {#if unavailable}
        <p class="sm-note">Search index is built with the site — run a production build to use it.</p>
      {:else if query.trim() && results.length === 0}
        <p class="sm-note">No results for “{query}”.</p>
      {:else}
        {#each results as r (r.url)}
          <a class="sm-hit" href={r.url} onclick={hide}>
            <span class="sm-hit-title">{r.title}</span>
            {#if r.excerpt}<span class="sm-hit-ex">{@html r.excerpt}</span>{/if}
          </a>
        {/each}
      {/if}
    </div>
  </div>
{/if}

<style>
  .search-trigger {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    font-family: var(--font-mono);
    font-size: 0.72rem;
    font-weight: 700;
    color: var(--muted);
    background: var(--surface);
    border: 2px solid var(--border);
    border-radius: var(--r);
    box-shadow: 2px 2px 0 0 var(--shadow);
    padding: 6px 10px;
    cursor: pointer;
  }
  .search-trigger:hover {
    color: var(--text);
  }
  .st-icon {
    font-size: 0.9rem;
  }
  .st-kbd {
    font-family: var(--font-mono);
    font-size: 0.6rem;
    color: var(--muted);
    background: var(--surface-2);
    border: 2px solid var(--border);
    border-radius: 4px;
    padding: 1px 5px;
  }

  .search-overlay {
    position: fixed;
    inset: 0;
    background: color-mix(in srgb, var(--border) 45%, transparent);
    z-index: 50;
  }
  .search-modal {
    position: fixed;
    top: 12vh;
    left: 50%;
    transform: translateX(-50%);
    width: min(620px, 92vw);
    z-index: 51;
    background: var(--surface);
    border: 3px solid var(--border);
    border-radius: var(--r);
    box-shadow: 10px 10px 0 0 var(--shadow);
    overflow: hidden;
  }
  .sm-bar {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 12px 14px;
    border-bottom: 2px solid var(--border);
  }
  .sm-icon {
    font-size: 1.1rem;
    color: var(--muted);
  }
  .sm-bar input {
    flex: 1;
    min-width: 0;
    font-family: var(--font-mono);
    font-size: 0.9rem;
    color: var(--text);
    background: transparent;
    border: 0;
    outline: none;
  }
  .sm-close {
    font-family: var(--font-mono);
    font-size: 0.6rem;
    font-weight: 700;
    color: var(--muted);
    background: var(--surface-2);
    border: 2px solid var(--border);
    border-radius: 4px;
    padding: 3px 7px;
    cursor: pointer;
  }
  .sm-results {
    max-height: 60vh;
    overflow-y: auto;
    padding: 8px;
  }
  .sm-note {
    font-family: var(--font-mono);
    font-size: 0.78rem;
    color: var(--muted);
    padding: 14px;
    margin: 0;
  }
  .sm-hit {
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding: 10px 12px;
    text-decoration: none;
    color: var(--text);
    border: 2px solid transparent;
    border-radius: var(--r);
  }
  .sm-hit:hover {
    background: var(--rune-soft);
    border-color: var(--border);
  }
  .sm-hit-title {
    font-family: var(--font-display);
    font-size: 0.92rem;
  }
  .sm-hit-ex {
    font-family: var(--font-sans);
    font-size: 0.78rem;
    line-height: 1.45;
    color: var(--muted);
  }
  .sm-hit-ex :global(mark) {
    background: var(--lime);
    color: var(--lime-ink);
    border-radius: 3px;
    padding: 0 2px;
  }

  /* Phone: collapse the trigger to an icon-only button with a comfortable hit
     area; the ⌘K hint and "Search" label are desktop affordances. The modal is
     already viewport-relative (min(620px, 92vw)) so it stays full-width here. */
  @media (max-width: 640px) {
    .st-text,
    .st-kbd {
      display: none;
    }
    .search-trigger {
      padding: 9px 11px;
    }
    .st-icon {
      font-size: 1.05rem;
    }
    .sm-hit {
      padding: 12px 12px;
    }
  }
</style>
