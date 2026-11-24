<!--
  The platform console: the shared shell with the console's destinations and
  the signed-in operator's name block, the fresh-proof prompt, and the page.
  Rust owns the session; this renders only its safe metadata.
-->
<script lang="ts">
  import '$lib/console.css';
  import { page } from '$app/state';
  import { setContext } from 'svelte';
  import ConsoleProblem from '$lib/ConsoleProblem.svelte';
  import PasswordInput from '$lib/PasswordInput.svelte';
  import { ConsoleReauth } from '$lib/console-reauth.svelte';
  import { ICONS } from '$lib/icons';
  import Shell, { type Destination } from '$lib/Shell.svelte';
  import type { LayoutProps } from './$types';
  let { data, children }: LayoutProps = $props();
  const reauth = setContext('console-reauth', new ConsoleReauth());
  async function confirm(event: SubmitEvent & { currentTarget: HTMLFormElement }) {
    event.preventDefault();
    await reauth.password(String(new FormData(event.currentTarget).get('password') ?? ''));
  }
  const views: readonly Destination[] = [
    { label: 'Overview', href: '/platform/ui', icon: ICONS.overview },
    { label: 'Workspaces', href: '/platform/ui/workspaces', icon: ICONS.workspaces },
    { label: 'Sign-in providers', href: '/platform/ui/providers', icon: ICONS.providers },
    { label: 'Mail', href: '/platform/ui/mail', icon: ICONS.mail },
    { label: 'Admins', href: '/platform/ui/admins', icon: ICONS.admins },
    { label: 'Stored secrets', href: '/platform/ui/secrets', icon: ICONS.secrets }
  ];
  const label = $derived(views.find(({ href }) => href === page.url.pathname)?.label ?? 'Overview');
  const me = $derived({ name: data.session.name, detail: 'Platform admin', href: '/platform/ui' });
</script>

<svelte:head><title>{label} · Platform admin · Wyrd</title></svelte:head>
<Shell home="/platform/ui" plane="platform" groups={[views]} {me}>
  <div class="ui-admin">
    <div class="work">
      {#if reauth.pending}
        <div class="card reauth" role="dialog" aria-labelledby="reauth-title">
          <header><h2 id="reauth-title">Confirm it is you</h2></header>
          <div class="prompt">
            <p><b>{reauth.pending.label}</b> is a destructive platform change. Confirm your own sign-in again; it stays fresh for five minutes. Nothing changes until you do.</p>
            {#if reauth.issue}<ConsoleProblem issue={reauth.issue} />{/if}
            {#if data.session.method === 'password'}
              <form class="field" onsubmit={confirm}>
                <label for="reauth-password">Your password</label>
                <PasswordInput id="reauth-password" name="password" required autocomplete="current-password" readonly={reauth.confirming} />
                <div class="buttons">
                  <button class="app-control primary" type="submit" disabled={reauth.confirming} aria-busy={reauth.confirming ? 'true' : undefined}>{reauth.confirming ? 'Confirming…' : 'Confirm'}</button>
                  <button class="app-control" type="button" onclick={() => reauth.cancel()}>Cancel</button>
                </div>
              </form>
            {:else}
              <p>Your identity provider asks you to sign in again, even if you are signed in there.</p>
              <div class="buttons">
                <button class="app-control primary" type="button" onclick={() => reauth.sso()} disabled={reauth.confirming} aria-busy={reauth.confirming ? 'true' : undefined}>{reauth.confirming ? 'Waiting for platform SSO…' : 'Continue with platform SSO'}</button>
                <button class="app-control" type="button" onclick={() => reauth.cancel()}>Cancel</button>
              </div>
            {/if}
          </div>
        </div>
      {/if}
      {@render children()}
    </div>
  </div>
</Shell>

<style>
  .reauth {
    border-left: 2px solid var(--warn);
  }
  .prompt {
    display: grid;
    gap: 12px;
    padding: 16px 20px;
    font-size: 13px;
  }
  .prompt .field {
    max-width: 360px;
  }
  .buttons {
    display: flex;
    gap: 8px;
  }
  /* Outcome lines every console page shares. */
  .work :global(.done) {
    margin-bottom: 20px;
    font-size: 13px;
  }
  .work :global(.ok) {
    color: var(--ok);
  }
  .work :global(.bad) {
    color: var(--danger);
  }
</style>
