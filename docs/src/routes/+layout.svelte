<script lang="ts">
  // The single chrome host: header (wordmark, search, theme), sidebar nav, the
  // content column with prev/next pagination, and a right-rail TOC. Global CSS
  // chain (wyrd.css → wyrd-tokens.css + fonts) is imported once here.
  import '../styles/wyrd.css';
  import { onMount } from 'svelte';
  import { afterNavigate } from '$app/navigation';
  import { base } from '$app/paths';
  import { page } from '$app/state';
  import { initLang } from '$lib/lang.svelte';
  import WyrdMark from '$lib/components/WyrdMark.svelte';
  import ThemeToggle from '$lib/components/ThemeToggle.svelte';
  import Search from '$lib/components/Search.svelte';
  import Sidebar from '$lib/components/Sidebar.svelte';
  import Toc from '$lib/components/Toc.svelte';
  import Pagination from '$lib/components/Pagination.svelte';

  let { children } = $props();

  let menuOpen = $state(false);

  // The home route is full-width "splash": no sidebar, no TOC, no pagination.
  const isHome = $derived(
    page.url.pathname === `${base}/` || page.url.pathname === base || page.url.pathname === '/'
  );

  // Error pages (404, etc.) render full-width without sidebar or TOC.
  const isError = $derived(page.error !== null);

  // Mock routes (design-direction previews) render bare — their own Frame owns chrome.
  const isMock = $derived(page.url.pathname.includes('/mocks'));

  onMount(initLang);
  // close the mobile nav drawer on navigation
  afterNavigate(() => (menuOpen = false));
</script>

{#if isMock}
  {@render children()}
{:else}
<a class="skip-link" href="#doc-main">Skip to content</a>

<header class="doc-header" data-pagefind-ignore>
  <a class="brand" href={`${base}/`} aria-label="Wyrd docs home">
    <WyrdMark size={22} />
    <span class="brand-name">WYRD</span>
    <span class="brand-tag">docs</span>
  </a>
  <div class="header-right">
    <Search />
    <ThemeToggle />
    {#if !isHome && !isError}
      <button
        class="menu-toggle"
        type="button"
        aria-expanded={menuOpen}
        aria-controls="doc-side"
        onclick={() => (menuOpen = !menuOpen)}
      >
        {menuOpen ? 'Close' : 'Menu'}
      </button>
    {/if}
  </div>
</header>

{#if isHome || isError}
  <main id="doc-main" class="doc-home">
    {@render children()}
  </main>
{:else}
  <div class="doc-shell">
    <aside id="doc-side" class="doc-side" class:open={menuOpen} data-pagefind-ignore>
      <Sidebar />
    </aside>
    <main id="doc-main" class="doc-main">
      {@render children()}
      <Pagination />
    </main>
    <aside class="doc-toc" data-pagefind-ignore>
      <Toc />
    </aside>
  </div>
{/if}
{/if}

<style>
  .skip-link {
    position: absolute;
    left: -9999px;
    top: 0;
    z-index: 100;
    background: var(--lime);
    color: var(--lime-ink);
    font-family: var(--font-mono);
    font-weight: 700;
    padding: 8px 12px;
    border: 2px solid var(--border);
    border-radius: var(--r);
  }
  .skip-link:focus {
    left: 8px;
    top: 8px;
  }

  .doc-header {
    position: sticky;
    top: 0;
    z-index: 40;
    display: flex;
    align-items: center;
    gap: 16px;
    padding: 12px 20px;
    background: var(--surface);
    border-bottom: 3px solid var(--client-bar);
  }
  .brand {
    display: inline-flex;
    align-items: center;
    gap: 9px;
    text-decoration: none;
    color: var(--text);
  }
  .brand-name {
    font-family: var(--font-display);
    font-size: 1.05rem;
    letter-spacing: -0.3px;
  }
  .brand-tag {
    font-family: var(--font-mono);
    font-size: 0.62rem;
    font-weight: 700;
    letter-spacing: 0.1em;
    text-transform: uppercase;
    color: var(--muted);
    border: 2px solid var(--border);
    border-radius: 4px;
    padding: 2px 6px;
  }
  .header-right {
    margin-left: auto;
    display: flex;
    align-items: center;
    gap: 10px;
  }
  .menu-toggle {
    display: none;
    font-family: var(--font-mono);
    font-size: 0.62rem;
    font-weight: 700;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--text);
    background: var(--surface);
    border: 2px solid var(--border);
    border-radius: var(--r);
    box-shadow: 2px 2px 0 0 var(--shadow);
    padding: 6px 10px;
    cursor: pointer;
  }

  .doc-shell {
    display: grid;
    grid-template-columns: 248px minmax(0, 1fr) 220px;
    gap: 0;
    max-width: 1400px;
    margin: 0 auto;
  }
  .doc-side {
    border-right: 2px solid var(--border);
    background: var(--surface);
    align-self: start;
    position: sticky;
    top: 60px;
    max-height: calc(100vh - 60px);
    overflow-y: auto;
  }
  .doc-main {
    min-width: 0;
    padding: 28px 36px 64px;
  }
  .doc-home {
    max-width: 1080px;
    margin: 0 auto;
    padding: 28px 24px 72px;
  }
  .doc-toc {
    padding: 28px 18px;
    min-width: 0;
  }

  @media (max-width: 1100px) {
    .doc-shell {
      grid-template-columns: 248px minmax(0, 1fr);
    }
    .doc-toc {
      display: none;
    }
  }

  @media (max-width: 820px) {
    .menu-toggle {
      display: inline-block;
    }
    .doc-shell {
      grid-template-columns: minmax(0, 1fr);
    }
    .doc-side {
      display: none;
      position: static;
      max-height: none;
      border-right: 0;
      border-bottom: 2px solid var(--border);
    }
    .doc-side.open {
      display: block;
    }
    .doc-main {
      padding: 20px 18px 56px;
    }
  }

  /* Phone chrome: keep the header bar on one row at ~320px by dropping the
     "docs" badge and tightening the gaps. The wordmark, search, theme, and menu
     controls stay; geometry/shadows are untouched. */
  @media (max-width: 640px) {
    .doc-header {
      gap: 10px;
      padding: 10px 14px;
    }
    .header-right {
      gap: 8px;
    }
    .brand-tag {
      display: none;
    }
    .menu-toggle {
      padding: 9px 11px;
    }
    .doc-home {
      padding: 20px 16px 56px;
    }
  }
</style>
