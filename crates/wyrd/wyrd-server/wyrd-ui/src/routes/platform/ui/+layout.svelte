<!--
  The platform console shell: the plane badge, the console views, the
  signed-in operator and sign-out. Rust owns the session; this renders only
  its safe metadata.
-->
<script lang="ts">
  import { page } from '$app/state';
  import logo from '../../../../brand/logo.svg?inline';
  import { setContext } from 'svelte';
  import ConsoleProblem from '$lib/ConsoleProblem.svelte';
  import PasswordInput from '$lib/PasswordInput.svelte';
  import { ConsoleReauth } from '$lib/console-reauth.svelte';
  import type { LayoutProps } from './$types';
  let { data, children }: LayoutProps = $props();
  const reauth = setContext('console-reauth', new ConsoleReauth());
  async function confirm(event: SubmitEvent & { currentTarget: HTMLFormElement }) {
    event.preventDefault();
    await reauth.password(String(new FormData(event.currentTarget).get('password') ?? ''));
  }
  const views = [
    ['/platform/ui', 'Overview'],
    ['/platform/ui/providers', 'Social sign-in'],
    ['/platform/ui/mail', 'Mail relay'],
    ['/platform/ui/signup', 'Signup']
  ] as const;
  const label = $derived(views.find(([path]) => path === page.url.pathname)?.[1] ?? 'Overview');
</script>

<svelte:head><title>{label} · Platform admin · Wyrd</title></svelte:head>
<div class="console">
  <header>
    <span class="brand"><img src={logo} alt="" width="22" height="22" /><span class="wordmark">Wyrd</span><span class="plane">Platform admin</span></span>
    <nav aria-label="Console">
      {#each views as [path, name] (path)}
        <a href={path} aria-current={page.url.pathname === path ? 'page' : undefined}>{name}</a>
      {/each}
    </nav>
    <span class="who">
      <span data-testid="operator">{data.session.name}</span>
      <form method="POST" action="/platform/ui/logout"><button class="app-control" type="submit">Sign out</button></form>
    </span>
  </header>
  <main class="body">
    <h1>{label}</h1>
    {#if reauth.pending}
      <div class="reauth" role="dialog" aria-labelledby="reauth-title">
        <h2 id="reauth-title">Confirm it is you</h2>
        <p><b>{reauth.pending.label}</b> is a destructive platform change. Confirm your own sign-in again; it stays fresh for five minutes. Nothing changes until you do.</p>
        {#if reauth.issue}<ConsoleProblem issue={reauth.issue} />{/if}
        {#if data.session.method === 'password'}
          <form class="stack" onsubmit={confirm}>
            <label for="reauth-password">Your password</label>
            <PasswordInput id="reauth-password" name="password" required autocomplete="current-password" readonly={reauth.confirming} />
            <div class="row">
              <button class="app-control primary" type="submit" disabled={reauth.confirming} aria-busy={reauth.confirming ? 'true' : undefined}>{reauth.confirming ? 'Confirming…' : 'Confirm'}</button>
              <button class="app-control" type="button" onclick={() => reauth.cancel()}>Cancel</button>
            </div>
          </form>
        {:else}
          <p>Your identity provider asks you to sign in again, even if you are signed in there.</p>
          <div class="row">
            <button class="app-control primary" type="button" onclick={() => reauth.sso()} disabled={reauth.confirming} aria-busy={reauth.confirming ? 'true' : undefined}>{reauth.confirming ? 'Waiting for platform SSO…' : 'Continue with platform SSO'}</button>
            <button class="app-control" type="button" onclick={() => reauth.cancel()}>Cancel</button>
          </div>
        {/if}
      </div>
    {/if}
    {@render children()}
  </main>
</div>

<style>
  .console {
    min-height: 100dvh;
    background: var(--bg);
  }
  header {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px 20px;
    padding: 10px 24px;
    border-bottom: 1px solid var(--border);
    border-top: 4px solid var(--declare);
    background: var(--surface);
  }
  .brand {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .brand img {
    border-radius: var(--r);
  }
  .wordmark {
    font: 600 16px var(--font-sans);
  }
  .plane {
    font: var(--weight-strong) 10px var(--font-mono);
    border: 2px solid var(--border);
    border-radius: var(--r);
    padding: 3px 10px;
    background: var(--declare);
    color: var(--declare-ink);
  }
  nav {
    display: flex;
    flex-wrap: wrap;
    gap: 4px 16px;
    flex: 1;
    font: 13px var(--font-sans);
  }
  nav a {
    color: var(--muted);
    text-decoration: none;
    padding: 4px 0;
    border-bottom: 2px solid transparent;
  }
  nav a[aria-current='page'] {
    color: var(--text);
    border-bottom-color: var(--declare);
  }
  .who {
    display: flex;
    align-items: center;
    gap: 10px;
    font: 12px var(--font-mono);
    color: var(--muted);
  }
  .body {
    display: grid;
    gap: 20px;
    align-content: start;
    max-width: 960px;
    padding: 24px;
  }
  h1 {
    margin: 0;
    font: var(--weight-strong) 24px var(--font-sans);
  }
  .reauth {
    display: grid;
    gap: 12px;
    padding: 16px;
    border: 2px solid var(--warn);
    border-radius: var(--r);
    background: var(--surface);
  }
  .body :global(section) {
    display: grid;
    gap: 12px;
    min-width: 0;
    padding: 16px;
    border: 1px solid var(--border);
    border-radius: var(--r);
    background: var(--surface);
  }
  .body :global(h2) {
    margin: 0;
    font: var(--weight-strong) 13px var(--font-sans);
  }
  .body :global(p) {
    margin: 0;
    font: 12px/18px var(--font-sans);
    color: var(--muted);
  }
  .body :global(dl) {
    display: grid;
    grid-template-columns: max-content minmax(0, 1fr);
    gap: 6px 16px;
    margin: 0;
    font: 12px var(--font-mono);
  }
  .body :global(dt) {
    color: var(--muted);
  }
  .body :global(dd) {
    margin: 0;
    overflow-wrap: anywhere;
  }
  .body :global(.stack) {
    display: grid;
    gap: 8px;
    max-width: 480px;
  }
  .body :global(.stack > .app-control) {
    justify-self: start;
  }
  .body :global(label),
  .body :global(legend) {
    font: var(--weight-strong) 11px var(--font-mono);
  }
  .body :global(.check) {
    display: flex;
    align-items: center;
    gap: 8px;
    font: 13px var(--font-sans);
  }
  .body :global(.check input) {
    accent-color: var(--declare);
    margin: 0;
  }
  .body :global(.row) {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }
  .body :global(.app-problem p),
  .body :global(.app-problem code) {
    margin: 6px 0 0;
  }
  /* Zero-specificity base styles, as the mock's global rules: a page's own rule wins. */
  :where(.body) :global(code) {
    font: 11px var(--font-mono);
    color: var(--text);
    overflow-wrap: anywhere;
  }
  :where(.body) :global(b) {
    color: var(--text);
    font-weight: var(--weight-strong);
    overflow-wrap: anywhere;
  }
  .body :global(p.done) {
    color: var(--text);
  }
  .body :global(.ok) {
    color: var(--ok);
  }
  .body :global(.bad) {
    color: var(--danger);
  }
  .body :global(.app-input[readonly]) {
    opacity: 0.6;
  }
  .body :global([role='status']:empty) {
    display: none;
  }
  @media (max-width: 767px) {
    header,
    .body {
      padding-left: 16px;
      padding-right: 16px;
    }
  }
</style>
