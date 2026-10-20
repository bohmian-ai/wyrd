<script lang="ts">
  import '../app.css';
  import favicon from '../../brand/app-icon.svg?inline';
  import { browser } from '$app/environment';
  import type { LayoutProps } from './$types';
  import { theme } from '$lib/theme.svelte';

  let { children, data }: LayoutProps = $props();

  // SSR paints the mode the request's cookie asked for; render is synchronous, so
  // writing the shared module state here is per-request. The client reads the same
  // cookie itself in theme.svelte.ts, so hydration agrees without a repaint.
  // svelte-ignore state_referenced_locally -- SSR-only initial read, by design
  if (!browser) theme.mode = data.mode;
</script>

<svelte:head><link rel="icon" type="image/svg+xml" href={favicon} /></svelte:head>

<div class="app-root" data-mode={theme.mode}>
  {@render children?.()}
  {#if data.localAuth}
    <details class="dev-tools">
      <summary>DEV · Test login</summary>
      <form method="POST" action="/?/loginScenario" class="login-scenarios">
        <label for="login-scenario">Access after sign-in</label>
        <select id="login-scenario" class="app-input" name="scenario">
          <option value="single" selected={data.loginScenario === 'single'}>One tenant — go to Home</option>
          <option value="multiple" selected={data.loginScenario === 'multiple'}>Multiple tenants — choose</option>
          <option value="none" selected={data.loginScenario === 'none'}>No access — request access</option>
        </select>
        <button class="app-control" type="submit">Apply and return to sign-in</button>
        <p>This signs out the current test user.</p>
      </form>
    </details>
  {/if}
</div>

<style>
  .app-root {
    min-height: 100%;
    background: var(--bg);
    color: var(--text);
    font-family: var(--font-sans);
  }
  .dev-tools {
    position: fixed;
    bottom: 12px;
    right: 12px;
    z-index: 10;
    max-width: calc(100vw - 24px);
    padding: 6px;
    border: 1px solid var(--border);
    border-radius: var(--r);
    background: var(--surface);
    color: var(--muted);
    font: var(--weight-strong) 10px var(--font-mono);
  }
  summary {
    cursor: pointer;
    padding: 5px;
  }
  .login-scenarios {
    position: absolute;
    right: 0;
    bottom: calc(100% + 8px);
    width: 300px;
    max-width: calc(100vw - 24px);
    display: grid;
    gap: 10px;
    padding: 14px;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: var(--r);
  }
  .login-scenarios select,
  .login-scenarios button {
    font-size: 10px;
  }
</style>
