<!-- #/platform: the console entry, "Platform admin"; #/platform/ui/callback: where platform SSO returns.
     Platform operators only; a tenant session, API key or bootstrap key never signs in here. GET
     /platform/browser/methods says whether platform OIDC exists; the SSO button shows only when it does. -->
<script>
  import Entry from './Entry.svelte';
  import { P, PROBLEMS, platformSession } from './data.js';
  let { callback = false, query } = $props();
  // App re-creates the page on every hash change, so reading the props once is the whole lifetime.
  const q = (key) => query.get(key);
  const preview = q('preview');
  const opening = (() => callback ? (preview === 'refused' ? P.platformIdentity : null) : { refused: P.platformRefused, throttled: P.platformThrottled, nosso: P.platformNoSso, unavailable: PROBLEMS.unavailable }[preview])();
  let issue = $state.raw(opening ?? null);
  let username = $state(preview === 'refused' ? 'ops-oncall' : '');
  let password = $state('');
  // `?sso=off`: no platform OIDC configured. `nosso` is the 404 a stale page gets after it was removed.
  const oidc = q('sso') !== 'off' && preview !== 'nosso';
  // The callback page renders after the server finished the login: a sign-in opens the console, a
  // re-authentication in a pop-up reports to the console (`popup`), or one opened directly confirms (`reauth`).
  if (callback && !preview) location.hash = '#/platform/ui';
  let busy = $state('');

  // Simulates POST /platform/browser/password/login {username, password}: 200 with the session view and a fresh
  // Path=/platform cookie; every refusal is one 401.
  function signIn(event) {
    event.preventDefault();
    if (!username.trim()) return document.getElementById('username').focus();
    if (!password) return document.getElementById('password').focus();
    issue = null;
    busy = 'password';
    setTimeout(() => {
      if (password === 'wrong') { issue = P.platformRefused; busy = ''; return; }
      Object.assign(platformSession, { method: 'password', recent: true });
      location.hash = '#/platform/ui';
    }, 700);
  }
  // Simulates POST /platform/browser/oidc/login, the IdP, then /platform/ui/callback POSTing
  // /platform/browser/oidc/callback {code, state, iss}.
  function sso(event) {
    event.preventDefault();
    issue = null;
    busy = 'sso';
    setTimeout(() => { Object.assign(platformSession, { method: 'oidc', recent: true }); location.hash = '#/platform/ui'; }, 900);
  }
</script>

<Entry heading="Platform admin" {issue}>
  {#if callback}
    {#if preview === 'popup'}
      <p class="center" role="status" data-testid="callback">Returning to the console…</p>
    {:else if preview === 'reauth'}
      <p class="center" role="status" data-testid="callback">Your sign-in is confirmed for five minutes.</p>
      <a class="app-control primary wide" href="#/platform/ui">Back to the console</a>
    {:else}
      <a class="app-control primary wide" href="#/platform">Back to sign-in</a>
    {/if}
  {:else}
    {#if preview === 'ended'}<p class="center" role="status" data-testid="ended">Your console session ended. Sign in again.</p>{/if}
    <p class="center">For operators of this Wyrd deployment. Workspace accounts sign in on their workspace's page.</p>
    <form class="fields" novalidate onsubmit={signIn}>
      <label for="username">Username</label>
      <input id="username" class="app-input" name="username" autocomplete="username" readonly={!!busy} bind:value={username} />
      <label for="password">Password</label>
      <input id="password" class="app-input" type="password" name="password" autocomplete="current-password" readonly={!!busy} bind:value={password} />
      <button class="app-control primary wide" type="submit" disabled={!!busy} aria-busy={busy === 'password' ? 'true' : undefined} data-testid="platform-signin">{busy === 'password' ? 'Signing in…' : 'Sign in'}</button>
    </form>
    {#if oidc}
      <form onsubmit={sso}><button class="app-control wide" type="submit" disabled={!!busy} data-testid="platform-sso">Sign in with platform SSO</button></form>
    {/if}
  {/if}
</Entry>

<style>
  .center {
    text-align: center;
  }
</style>
