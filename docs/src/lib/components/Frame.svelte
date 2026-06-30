<script lang="ts">
  // Arcade chrome wrapper: Mitari marquee header + mobile drawer + footer.
  // "Mitari" is company/umbrella chrome only — wordmark and footer text; never
  // a route, class, package, or schema label.
  //
  // Theme toggle delegates to $lib/components/ThemeToggle (the FOUC-safe
  // persisted store from commit 01).  Drawer is local open/close state.
  // All colors are var(--*) tokens; cabinet chrome uses var(--cab-*) defined
  // in wyrd.css (always-dark fixed surface, both themes).

  import WyrdMark from './WyrdMark.svelte';
  import ThemeToggle from './ThemeToggle.svelte';
  import Search from './Search.svelte';

  let {
    product = 'wyrd',
    children
  }: {
    product?: 'wyrd' | 'fathom';
    children?: import('svelte').Snippet;
  } = $props();

  let drawer = $state(false);

  function closeDrawer() {
    drawer = false;
  }
</script>

<div class="frame">
  <header class="marquee" data-pagefind-ignore>
    <a class="brand" href="/">
      <WyrdMark size={22} />
      <span class="wm">MITARI</span>
      <span class="co">docs</span>
    </a>

    <nav class="switch" aria-label="Product">
      <a
        href="/"
        data-k="wyrd"
        class:on={product === 'wyrd'}
        aria-current={product === 'wyrd' ? 'page' : undefined}
      >Wyrd</a>
      <a
        href="/fathom/"
        data-k="fathom"
        class:on={product === 'fathom'}
        aria-current={product === 'fathom' ? 'page' : undefined}
      >Fathom <span class="soon">soon</span></a>
    </nav>

    <div class="right">
      <Search />
      <ThemeToggle />
      <a
        class="iconbtn"
        href="https://github.com/wyrd-ml/wyrd"
        target="_blank"
        rel="noopener"
        aria-label="GitHub"
      >
        <svg viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">
          <path d="M12 2a10 10 0 0 0-3.16 19.49c.5.09.68-.22.68-.48v-1.7c-2.78.6-3.37-1.34-3.37-1.34-.45-1.16-1.1-1.47-1.1-1.47-.9-.62.07-.6.07-.6 1 .07 1.53 1.03 1.53 1.03.9 1.53 2.36 1.09 2.94.83.09-.65.35-1.09.63-1.34-2.22-.25-4.55-1.11-4.55-4.94 0-1.09.39-1.98 1.03-2.68-.1-.25-.45-1.27.1-2.65 0 0 .84-.27 2.75 1.02a9.5 9.5 0 0 1 5 0c1.91-1.29 2.75-1.02 2.75-1.02.55 1.38.2 2.4.1 2.65.64.7 1.03 1.59 1.03 2.68 0 3.84-2.34 4.69-4.57 4.94.36.31.68.92.68 1.85v2.74c0 .27.18.58.69.48A10 10 0 0 0 12 2z"/>
        </svg>
      </a>
      <button
        class="iconbtn menu"
        type="button"
        aria-label="Menu"
        aria-expanded={drawer}
        onclick={() => (drawer = !drawer)}
      >
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" aria-hidden="true">
          {#if drawer}
            <path d="M5 5l14 14M19 5 5 19" />
          {:else}
            <path d="M3 6h18M3 12h18M3 18h18" />
          {/if}
        </svg>
      </button>
    </div>
  </header>

  <div class="drawer" class:open={drawer} role="navigation" aria-label="Mobile navigation" data-pagefind-ignore>
    <a href="/" onclick={closeDrawer}>Wyrd · Start here</a>
    <a href="/concepts/" onclick={closeDrawer}>Concepts</a>
    <a href="/guides/" onclick={closeDrawer}>Guides</a>
    <a href="/api/" onclick={closeDrawer}>Reference</a>
    <a href="/agents/" onclick={closeDrawer}>For agents</a>
    <a href="/fathom/" onclick={closeDrawer}>Fathom <span class="soon">soon</span></a>
  </div>

  <div class="content">{@render children?.()}</div>

  <footer class="foot" data-pagefind-ignore>
    <div class="foot-in">
      <div>
        <div class="fb"><WyrdMark size={22} /><span class="wm">MITARI</span></div>
        <div class="ci">© 2026 Mitari. All rights reserved.<br />Seattle, WA</div>
      </div>
      <div class="fcols">
        <div class="fcol">
          <span class="ch">WYRD</span>
          <a href="/quickstart/">Quickstart</a>
          <a href="/concepts/">Concepts</a>
          <a href="/api/">Reference</a>
          <a href="/agents/">For agents</a>
        </div>
        <div class="fcol">
          <span class="ch">FATHOM</span>
          <a href="/fathom/">Overview</a>
          <a href="/fathom/">Coming soon</a>
        </div>
        <div class="fcol">
          <span class="ch">PROJECT</span>
          <a href="https://github.com/wyrd-ml/wyrd" target="_blank" rel="noopener">GitHub</a>
          <a href="/changelog/">Changelog</a>
          <a href="/llms.txt">llms.txt</a>
        </div>
      </div>
    </div>
  </footer>
</div>

<style>
  /* Cabinet chrome: uses --cab-* vars (always-dark fixed surface defined in
     wyrd.css).  All other values are standard palette tokens from wyrd-tokens.css.
     No raw hex anywhere in this file. */

  .frame {
    background: var(--bg);
    color: var(--text);
    font-family: var(--font-sans);
    font-weight: 500;
    line-height: 1.55;
    min-height: 100vh;
    -webkit-font-smoothing: antialiased;
  }
  .frame *,
  .frame *::before,
  .frame *::after {
    box-sizing: border-box;
  }
  .frame a {
    color: inherit;
    text-decoration: none;
  }

  /* ---- Marquee (always-dark cabinet header) ---- */
  .marquee {
    position: sticky;
    top: 0;
    z-index: 50;
    background: var(--cab);
    border-bottom: 3px solid var(--lime);
    display: flex;
    align-items: center;
    gap: 16px;
    padding: 11px 22px;
  }

  .brand {
    display: inline-flex;
    align-items: center;
    gap: 10px;
  }
  .brand :global(.wyrd-mark) {
    width: 22px;
    height: 22px;
    display: block;
  }
  .brand .wm {
    font-family: var(--font-arcade);
    font-size: 13px;
    letter-spacing: 1.5px;
    color: var(--cab-ink);
  }
  .brand .co {
    font-family: var(--font-mono);
    font-size: 9px;
    font-weight: 700;
    letter-spacing: 0.12em;
    text-transform: uppercase;
    color: var(--cab-dim);
    border-left: 2px solid var(--cab-line);
    padding-left: 8px;
  }

  /* product switch */
  .switch {
    display: inline-flex;
    gap: 5px;
    margin-left: 6px;
  }
  .switch a {
    position: relative;
    font-family: var(--font-mono);
    font-size: 10.5px;
    font-weight: 700;
    letter-spacing: 0.05em;
    text-transform: uppercase;
    color: var(--cab-dim);
    padding: 6px 11px;
    border: 2px solid var(--cab-line);
    border-radius: var(--r);
    display: inline-flex;
    align-items: center;
    gap: 7px;
  }
  .switch a.on[data-k='wyrd'] {
    color: var(--lime-ink);
    background: var(--lime);
    border-color: var(--lime);
  }
  .switch a .soon {
    font-size: 7.5px;
    letter-spacing: 0.08em;
    padding: 2px 4px;
    border-radius: 3px;
    background: var(--rune-strong);
    color: var(--rune-btn-ink);
  }

  /* right controls */
  .right {
    margin-left: auto;
    display: flex;
    align-items: center;
    gap: 9px;
  }
  .iconbtn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 34px;
    height: 34px;
    color: var(--cab-ink);
    background: var(--cab-line);
    border: 2px solid var(--cab-line);
    border-radius: var(--r);
    cursor: pointer;
    text-decoration: none;
  }
  .iconbtn:hover {
    border-color: var(--lime);
    color: var(--lime);
  }
  .iconbtn svg {
    width: 17px;
    height: 17px;
    display: block;
  }
  .menu {
    display: none;
  }

  /* ---- Mobile drawer ---- */
  .drawer {
    display: none;
    flex-direction: column;
    gap: 6px;
    padding: 14px 22px 18px;
    background: var(--cab);
    border-bottom: 3px solid var(--lime);
  }
  .drawer.open {
    display: flex;
  }
  .drawer a {
    font-family: var(--font-mono);
    font-size: 12px;
    font-weight: 700;
    text-transform: uppercase;
    color: var(--cab-dim);
    padding: 10px 12px;
    border: 2px solid var(--cab-line);
    border-radius: var(--r);
    display: inline-flex;
    align-items: center;
    gap: 7px;
  }
  .drawer a:hover {
    color: var(--lime-ink);
    background: var(--lime);
    border-color: var(--lime);
  }
  .drawer a .soon {
    font-size: 7.5px;
    letter-spacing: 0.08em;
    padding: 2px 4px;
    border-radius: 3px;
    background: var(--rune-strong);
    color: var(--rune-btn-ink);
  }

  /* ---- Footer (always-dark cabinet) ---- */
  .foot {
    background: var(--cab);
    border-top: 3px solid var(--border);
    padding: 36px 28px;
  }
  .foot-in {
    max-width: 1180px;
    margin: 0 auto;
    display: flex;
    justify-content: space-between;
    gap: 32px;
    flex-wrap: wrap;
  }
  .fb {
    display: flex;
    align-items: center;
    gap: 10px;
    margin-bottom: 10px;
  }
  .fb :global(.wyrd-mark) {
    width: 22px;
    height: 22px;
  }
  .fb .wm {
    font-family: var(--font-arcade);
    font-size: 12px;
    letter-spacing: 1.5px;
    color: var(--cab-ink);
  }
  .ci {
    font-family: var(--font-mono);
    font-size: 11px;
    color: var(--cab-dim);
    line-height: 1.7;
  }
  .fcols {
    display: flex;
    gap: 48px;
    flex-wrap: wrap;
  }
  .fcol {
    display: flex;
    flex-direction: column;
    gap: 9px;
  }
  .fcol .ch {
    font-family: var(--font-arcade);
    font-size: 8px;
    color: var(--cab-dim);
    letter-spacing: 0.06em;
    margin-bottom: 2px;
  }
  .fcol a {
    font-family: var(--font-mono);
    font-size: 12px;
    color: var(--cab-dim);
  }
  .fcol a:hover {
    color: var(--lime);
  }

  /* ---- Responsive ---- */
  @media (max-width: 820px) {
    .switch {
      display: none;
    }
    .menu {
      display: inline-flex;
    }
  }
  @media (max-width: 560px) {
    .brand .co {
      display: none;
    }
  }
</style>
