<script lang="ts">
  import { onMount } from 'svelte';

  // Theme toggle honoring the FOUC contract from app.html: the source of truth
  // is `document.documentElement.dataset.theme` ('light' | 'dark'), mirrored to
  // localStorage['wyrd:theme']. We read the pre-paint value on mount and write
  // both on toggle.
  type Theme = 'light' | 'dark';
  let theme = $state<Theme>('dark');

  onMount(() => {
    const t = document.documentElement.dataset.theme;
    theme = t === 'light' ? 'light' : 'dark';
  });

  function toggle(): void {
    theme = theme === 'dark' ? 'light' : 'dark';
    document.documentElement.dataset.theme = theme;
    try {
      localStorage.setItem('wyrd:theme', theme);
    } catch {
      /* storage unavailable */
    }
  }
</script>

<button
  class="theme-toggle"
  type="button"
  onclick={toggle}
  aria-label={`Switch to ${theme === 'dark' ? 'light' : 'dark'} theme`}
  title="Toggle theme"
>
  <span class="tt-face">{theme === 'dark' ? 'DARK' : 'LIGHT'}</span>
</button>

<style>
  .theme-toggle {
    font-family: var(--font-mono);
    font-size: 0.62rem;
    font-weight: 700;
    letter-spacing: 0.08em;
    color: var(--text);
    background: var(--surface);
    border: 2px solid var(--border);
    border-radius: var(--r);
    box-shadow: 2px 2px 0 0 var(--shadow);
    padding: 6px 10px;
    cursor: pointer;
    transition:
      transform 0.04s,
      box-shadow 0.04s;
  }
  .theme-toggle:hover {
    transform: translate(-1px, -1px);
    box-shadow: 4px 4px 0 0 var(--shadow);
  }
  .theme-toggle:active {
    transform: translate(1px, 1px);
    box-shadow: 1px 1px 0 0 var(--shadow);
  }
  @media (max-width: 640px) {
    .theme-toggle {
      padding: 9px 10px;
    }
  }
</style>
