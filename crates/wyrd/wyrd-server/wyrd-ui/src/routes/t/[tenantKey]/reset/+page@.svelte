<script lang="ts">
  import { enhance } from '$app/forms';
  import Entry from '$lib/Entry.svelte';
  import PasswordInput from '$lib/PasswordInput.svelte';
  import { passwordError } from '$lib/onboarding';
  import type { SubmitFunction } from '@sveltejs/kit';
  import type { PageProps } from './$types';
  let { data, form }: PageProps = $props();
  let password = $state('');
  let checked = $state(false);
  let setting = $state(false);
  const pwError = $derived(checked ? passwordError(password) : '');

  /** Check the new password before posting it, and show the request in flight. */
  const redeem: SubmitFunction = ({ cancel }) => {
    checked = true;
    if (passwordError(password)) {
      document.getElementById('new-password')?.focus();
      return cancel();
    }
    setting = true;
    return async ({ update }) => {
      await update();
      setting = false;
    };
  };
</script>

<svelte:head><title>Reset password · {data.tenant.key} · Wyrd</title></svelte:head>
<Entry heading="Reset your {data.tenant.key} password" issue={form?.problem ?? data.problem}>
  {#if data.password}
    {#if data.mail}
      {#if form?.sent}
        <p class="done" role="status"><span class="ok">✓</span> If {data.tenant.key} has a password account named <b>{form.sent}</b> with a verified recovery email, a code is on its way there. It works once, for fifteen minutes.</p>
      {:else}
        <form class="fields" method="POST" action="?/request">
          <label for="username">Username</label>
          <input id="username" class="app-input" name="username" required autocomplete="username" />
          <p>We email a reset code to your verified recovery email.</p>
          <button class="app-control wide" type="submit">Email me a code</button>
        </form>
      {/if}
      <p class="or">then</p>
    {/if}
    <form class="fields" method="POST" action="?/reset" use:enhance={redeem}>
      <label for="reset-code">Reset code</label>
      <input id="reset-code" class="app-input mono" type="password" name="resetCode" required autocomplete="off" readonly={setting} />
      <p>{data.mail ? 'Paste the code from the email, or one an administrator gave you.' : 'Ask a tenant administrator for a reset code.'} It works once, for fifteen minutes.</p>
      <label for="new-password">New password</label>
      <PasswordInput id="new-password" name="newPassword" required autocomplete="new-password" readonly={setting} bind:value={password}
        aria-invalid={pwError ? 'true' : undefined} aria-describedby="pw-hint{pwError ? ' pw-error' : ''}" />
      {#if pwError}<p class="field-error" id="pw-error">⊘ {pwError}</p>{/if}
      <p id="pw-hint">15 to 128 characters. Setting it signs you out everywhere and ends CLI sign-ins; API keys keep working.</p>
      <button class="app-control primary wide" type="submit" disabled={setting} aria-busy={setting ? 'true' : undefined}>{setting ? 'Setting password…' : 'Set password'}</button>
    </form>
  {/if}
  <nav class="links"><a href="/t/{encodeURIComponent(data.tenant.key)}/login">Back to sign-in</a></nav>
</Entry>
