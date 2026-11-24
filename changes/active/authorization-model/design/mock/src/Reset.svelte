<!-- #/t/<slug>/reset: ask for a reset code at the verified recovery email, or paste one (mailed or administrator-issued). -->
<script>
  import Entry from './Entry.svelte';
  import PasswordInput from './PasswordInput.svelte';
  import { P, passwordError } from './data.js';
  let { tenant, query } = $props();
  // App re-creates the page on every hash change, so reading the props once is the whole lifetime.
  const q = (key) => query.get(key);
  const preview = q('preview');
  // GET /auth/browser/methods?tenant=<slug>: password_reset is false without a relay or when company SSO is required.
  const canRequest = preview !== 'nomail' && preview !== 'sso';
  let issue = $state.raw({ throttled: P.resetThrottled, invalid: P.resetCode, sso: P.resetOff }[preview] ?? null);
  let username = $state(preview === 'sent' || preview === 'throttled' ? 'sam' : '');
  let sent = $state(preview === 'sent');
  let code = $state(preview === 'invalid' ? 'used-or-expired-code' : '');
  let password = $state('');
  let checked = $state(false);
  let setting = $state(false);
  const pwError = $derived(checked ? passwordError(password) : '');

  // Simulates POST /auth/browser/password/reset/request {tenant, username}: the same 202 for every account.
  function request(event) {
    event.preventDefault();
    if (!username.trim()) return document.getElementById('username').focus();
    issue = null;
    sent = true;
  }
  // Simulates POST /auth/browser/password/reset {tenant, reset_code, new_password}: 204, signs nothing in.
  function redeem(event) {
    event.preventDefault();
    checked = true;
    if (!code.trim()) return document.getElementById('reset-code').focus();
    if (pwError) return document.getElementById('new-password').focus();
    issue = null;
    setting = true;
    setTimeout(() => {
      setting = false;
      if (code.trim() === 'used-or-expired-code') { issue = P.resetCode; return; }
      location.hash = `#/t/${tenant}/login?reset=done`;
    }, 700);
  }
</script>

<Entry heading="Reset your {tenant} password" {issue}>
  {#if preview !== 'sso'}
    {#if canRequest}
      {#if sent}
        <p class="done" role="status" data-testid="reset-sent"><span class="ok">✓</span> If {tenant} has a password account named <b>{username.trim()}</b> with a verified recovery email, a code is on its way there. It works once, for fifteen minutes.</p>
      {:else}
        <form class="fields" novalidate onsubmit={request}>
          <label for="username">Username</label>
          <input id="username" class="app-input" name="username" autocomplete="username" bind:value={username} />
          <p>We email a reset code to your verified recovery email.</p>
          <button class="app-control wide" type="submit" data-testid="reset-request">Email me a code</button>
        </form>
      {/if}
      <p class="or">then</p>
    {/if}
    <form class="fields" novalidate onsubmit={redeem}>
      <label for="reset-code">Reset code</label>
      <input id="reset-code" class="app-input mono" type="password" name="resetCode" autocomplete="off" readonly={setting} bind:value={code} />
      <p>{canRequest ? 'Paste the code from the email, or one an administrator gave you.' : 'Ask a tenant administrator for a reset code.'} It works once, for fifteen minutes.</p>
      <label for="new-password">New password</label>
      <PasswordInput id="new-password" name="newPassword" autocomplete="new-password" readonly={setting} bind:value={password}
        aria-invalid={pwError ? 'true' : undefined} aria-describedby="pw-hint{pwError ? ' pw-error' : ''}" />
      {#if pwError}<p class="field-error" id="pw-error">⊘ {pwError}</p>{/if}
      <p id="pw-hint">15 to 128 characters. Setting it signs you out everywhere and ends CLI sign-ins; API keys keep working.</p>
      <button class="app-control primary wide" type="submit" disabled={setting} aria-busy={setting ? 'true' : undefined} data-testid="reset-set">{setting ? 'Setting password…' : 'Set password'}</button>
    </form>
  {/if}
  <nav class="links"><a href="#/t/{tenant}/login">Back to sign-in</a></nav>
</Entry>
