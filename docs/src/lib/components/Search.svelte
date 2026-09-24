<script lang="ts">
  import { onMount } from 'svelte';
  import { base } from '$app/paths';

  // Pagefind search: a header control opening a modal over the Pagefind JS API
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

<button class="search-open" type="button" onclick={show} aria-label="Search docs">
  <svg viewBox="0 0 20 20" aria-hidden="true">
    <circle cx="8.5" cy="8.5" r="5.5" fill="none" stroke="currentColor" stroke-width="1.6" />
    <path d="m13 13 4 4" fill="none" stroke="currentColor" stroke-width="1.6" />
  </svg>
  <span>Search docs</span>
  <kbd>⌘K</kbd>
</button>

{#if open}
  <button class="search-scrim" type="button" tabindex="-1" aria-label="Close search" onclick={hide}
  ></button>
  <div class="search-modal" role="dialog" aria-modal="true" aria-label="Search docs">
    <div class="sm-bar">
      <input
        bind:this={inputEl}
        bind:value={query}
        oninput={onInput}
        type="search"
        aria-label="Search the docs"
        placeholder="Search the docs"
        autocomplete="off"
        spellcheck="false"
      />
      <button class="sm-close" type="button" onclick={hide}>Close <kbd>Esc</kbd></button>
    </div>
    <div class="sm-results" aria-live="polite">
      {#if unavailable}
        <p class="sm-note">The search index is built with the site. Run a production build to use it.</p>
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
  .search-scrim {
    position: fixed;
    inset: 0;
    z-index: 50;
    border: 0;
    background: color-mix(in srgb, var(--bg) 72%, transparent);
    cursor: default;
  }
  .search-modal {
    position: fixed;
    top: 12vh;
    left: 50%;
    transform: translateX(-50%);
    width: min(620px, calc(100vw - 24px));
    z-index: 51;
    background: var(--surface);
    border: 1px solid var(--text);
    border-radius: var(--r);
    overflow: hidden;
  }
  .sm-bar {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px;
    border-bottom: 1px solid var(--border);
  }
  .sm-bar input {
    flex: 1;
    min-width: 0;
    height: 40px;
    padding: 0 10px;
    border: 1px solid var(--border);
    border-radius: var(--r);
    background: var(--bg);
    color: var(--text);
    font-size: 16px;
  }
  .sm-close {
    min-height: 40px;
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 0 10px;
    border: 1px solid var(--border);
    border-radius: var(--r);
    background: var(--surface);
    color: var(--text);
    font-size: 13px;
  }
  .sm-close kbd {
    color: var(--muted);
    font: 11px var(--font-mono);
  }
  .sm-results {
    max-height: 60vh;
    overflow-y: auto;
    padding: 6px;
  }
  .sm-note {
    margin: 0;
    padding: 14px;
    color: var(--muted);
    font-size: 14px;
  }
  .sm-hit {
    display: flex;
    flex-direction: column;
    gap: 3px;
    padding: 10px 12px;
    border: 1px solid transparent;
    border-radius: var(--r);
    color: var(--text);
    text-decoration: none;
  }
  .sm-hit:hover,
  .sm-hit:focus-visible {
    border-color: var(--declare);
    background: var(--declare-soft);
  }
  .sm-hit-title {
    font-weight: 700;
  }
  .sm-hit-ex {
    color: var(--muted);
    font-size: 13px;
    line-height: 1.45;
  }
  .sm-hit-ex :global(mark) {
    background: none;
    color: var(--text);
    font-weight: 700;
    text-decoration: underline 1px var(--declare);
    text-underline-offset: 2px;
  }
</style>
