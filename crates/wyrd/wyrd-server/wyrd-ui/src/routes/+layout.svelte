<script lang="ts">
  import '../app.css';
  import favicon from '../../brand/app-icon.svg?inline';
  import { page } from '$app/state';
  import { browser } from '$app/environment';
  import type { LayoutProps } from './$types';
  import ModeProvider from '$lib/components/ModeProvider.svelte';
  import { theme } from '$lib/theme.svelte';

  let { children, data }: LayoutProps = $props();

  // SSR paints the mode the request's cookie asked for; render is synchronous, so
  // writing the shared module state here is per-request. The client reads the same
  // cookie itself in theme.svelte.ts, so hydration agrees without a repaint.
  // svelte-ignore state_referenced_locally -- SSR-only initial read, by design
  if (!browser) theme.mode = data.mode;
</script>

<svelte:head><link rel="icon" type="image/svg+xml" href={favicon} /></svelte:head>

<ModeProvider>
  {@render children?.()}
  {#if data.development}
    <div class="dev-tools" role="region" aria-label="Development tools">
    <form method="POST" action="/?/mockData">
      <input type="hidden" name="csrf" value={data.devCsrf ?? ''} />
      <input type="hidden" name="enabled" value={String(!data.mockData)} />
      <input type="hidden" name="returnTo" value={page.url.pathname + page.url.search} />
      <span>DEV</span><button class="app-control" type="submit" aria-pressed={data.mockData}>Mock data: {data.mockData ? 'On' : 'Off'}</button>
    </form>
    {#if !data.mockData}
      <details><summary>Connection status</summary><div class="login-scenarios">The browser SSO connection is not connected yet.</div></details>
    {/if}
    {#if data.mockData}
      <details>
        <summary>Test login</summary>
        <form method="POST" action="/?/loginScenario" class="login-scenarios">
          <input type="hidden" name="csrf" value={data.devCsrf ?? ''} />
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
  {/if}
</ModeProvider>

<style>
  .dev-tools {
    position: fixed;
    bottom: 12px;
    right: 12px;
    z-index: 10;
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px;
    border: 2px solid var(--border);
    border-radius: var(--r);
    background: var(--surface);
    color: var(--muted);
    font: 700 10px var(--font-mono);
  }
  .dev-tools > form {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .dev-tools {
    max-width: calc(100vw - 24px);
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
    border: 2px solid var(--border);
    border-radius: var(--r);
  }
  .login-scenarios select {
    font-size: 10px;
  }
  .dev-tools button {
    padding: 5px 8px;
    font-size: 10px;
    box-shadow: none;
  }
</style>
