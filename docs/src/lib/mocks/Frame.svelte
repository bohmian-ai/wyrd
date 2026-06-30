<script lang="ts">
  import { onMount } from 'svelte';
  import Fuji from './Fuji.svelte';

  let {
    dir = 'a',
    product = 'wyrd',
    children
  }: {
    dir?: 'a' | 'b' | 'c' | 'd';
    product?: 'wyrd' | 'fathom';
    children?: import('svelte').Snippet;
  } = $props();

  let theme = $state<'light' | 'dark'>('dark');
  let drawer = $state(false);

  onMount(() => {
    const t = document.documentElement.dataset.theme;
    theme = t === 'light' ? 'light' : 'dark';
  });

  function toggle() {
    theme = theme === 'dark' ? 'light' : 'dark';
    document.documentElement.dataset.theme = theme;
    try {
      localStorage.setItem('wyrd:theme', theme);
    } catch {
      /* ignore */
    }
  }
</script>

<div class={`mk ${dir}`}>
  {#if dir === 'd'}
    <header class="hud">
      <a class="brand" href="#top">
        <span class="p1">P1</span>
        <Fuji size={20} />
        <span class="wm">WYRD</span>
      </a>
      <span class="hstat world"><i>WORLD</i><b>1-1</b></span>
      <span class="hstat"><i>SCORE</i><b>013370</b></span>
      <span class="hstat coins"><span class="coin-spr" aria-hidden="true"></span><b>×07</b></span>
      <span class="lives" aria-hidden="true"><i></i><i></i><i></i></span>
      <nav class="switch" aria-label="Product">
        <a href="#top" data-k="wyrd" class={product === 'wyrd' ? 'on' : ''}>Wyrd</a>
        <a href="#top" data-k="fathom" class={product === 'fathom' ? 'on' : ''}>
          Fathom <span class="soon">locked</span>
        </a>
      </nav>
      <div class="right">
        <span class="search" role="searchbox" tabindex="0">▸ FIND<span class="k">⌘K</span></span>
        <button class="iconbtn" aria-label="Toggle theme" onclick={toggle}>
          {#if theme === 'dark'}
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round">
              <circle cx="12" cy="12" r="4.5" />
              <path d="M12 2v2M12 20v2M2 12h2M20 12h2M5 5l1.5 1.5M17.5 17.5 19 19M19 5l-1.5 1.5M6.5 17.5 5 19" />
            </svg>
          {:else}
            <svg viewBox="0 0 24 24" fill="currentColor">
              <path d="M20 14.5A8 8 0 1 1 9.5 4a6.5 6.5 0 0 0 10.5 10.5z" />
            </svg>
          {/if}
        </button>
        <button
          class="iconbtn menu"
          aria-label="Menu"
          aria-expanded={drawer}
          onclick={() => (drawer = !drawer)}
        >
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round">
            {#if drawer}<path d="M5 5l14 14M19 5 5 19" />{:else}<path d="M3 6h18M3 12h18M3 18h18" />{/if}
          </svg>
        </button>
      </div>
    </header>
  {:else}
  <header class="marquee">
    {#if dir === 'c'}
      <span class="term-dots" aria-hidden="true"><i></i><i></i><i></i></span>
    {/if}
    <a class="brand" href="#top">
      <Fuji size={22} />
      <span class="wm">MITARI</span>
      <span class="co">docs</span>
    </a>
    <nav class="switch" aria-label="Product">
      <a href="#top" data-k="wyrd" class={product === 'wyrd' ? 'on' : ''}>Wyrd</a>
      <a href="#top" data-k="fathom" class={product === 'fathom' ? 'on' : ''}>
        Fathom <span class="soon">soon</span>
      </a>
    </nav>
    <div class="right">
      <span class="search" role="searchbox" tabindex="0">
        {dir === 'c' ? 'grep docs…' : 'Search docs…'}
        <span class="k">⌘K</span>
      </span>
      <button class="iconbtn" aria-label="Toggle theme" onclick={toggle}>
        {#if theme === 'dark'}
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round">
            <circle cx="12" cy="12" r="4.5" />
            <path d="M12 2v2M12 20v2M2 12h2M20 12h2M5 5l1.5 1.5M17.5 17.5 19 19M19 5l-1.5 1.5M6.5 17.5 5 19" />
          </svg>
        {:else}
          <svg viewBox="0 0 24 24" fill="currentColor">
            <path d="M20 14.5A8 8 0 1 1 9.5 4a6.5 6.5 0 0 0 10.5 10.5z" />
          </svg>
        {/if}
      </button>
      <a class="iconbtn" href="#top" aria-label="GitHub">
        <svg viewBox="0 0 24 24" fill="currentColor"
          ><path
            d="M12 2a10 10 0 0 0-3.16 19.49c.5.09.68-.22.68-.48v-1.7c-2.78.6-3.37-1.34-3.37-1.34-.45-1.16-1.1-1.47-1.1-1.47-.9-.62.07-.6.07-.6 1 .07 1.53 1.03 1.53 1.03.9 1.53 2.36 1.09 2.94.83.09-.65.35-1.09.63-1.34-2.22-.25-4.55-1.11-4.55-4.94 0-1.09.39-1.98 1.03-2.68-.1-.25-.45-1.27.1-2.65 0 0 .84-.27 2.75 1.02a9.5 9.5 0 0 1 5 0c1.91-1.29 2.75-1.02 2.75-1.02.55 1.38.2 2.4.1 2.65.64.7 1.03 1.59 1.03 2.68 0 3.84-2.34 4.69-4.57 4.94.36.31.68.92.68 1.85v2.74c0 .27.18.58.69.48A10 10 0 0 0 12 2z"
          /></svg
        >
      </a>
      <button
        class="iconbtn menu"
        aria-label="Menu"
        aria-expanded={drawer}
        onclick={() => (drawer = !drawer)}
      >
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round">
          {#if drawer}<path d="M5 5l14 14M19 5 5 19" />{:else}<path d="M3 6h18M3 12h18M3 18h18" />{/if}
        </svg>
      </button>
    </div>
  </header>
  {/if}

  <div class={`drawer ${drawer ? 'open' : ''}`}>
    <a href="#top">Wyrd · Start here</a>
    <a href="#top">Concepts</a>
    <a href="#top">Guides</a>
    <a href="#top">Reference</a>
    <a href="#top">For agents</a>
    <a href="#top">Fathom · coming soon</a>
  </div>

  <div id="top">{@render children?.()}</div>

  <footer class="foot">
    <div class="foot-in">
      <div>
        <div class="fb"><Fuji size={22} /><span class="wm">MITARI</span></div>
        <div class="ci">© 2026 Mitari. All rights reserved.<br />Seattle, WA</div>
      </div>
      <div class="fcols">
        <div class="fcol">
          <span class="ch">WYRD</span><a href="#top">Quickstart</a><a href="#top">Concepts</a
          ><a href="#top">Reference</a><a href="#top">For agents</a>
        </div>
        <div class="fcol">
          <span class="ch">FATHOM</span><a href="#top">Overview</a><a href="#top">Coming soon</a>
        </div>
        <div class="fcol">
          <span class="ch">PROJECT</span><a href="#top">GitHub</a><a href="#top">Changelog</a
          ><a href="#top">llms.txt</a>
        </div>
      </div>
    </div>
  </footer>
</div>
