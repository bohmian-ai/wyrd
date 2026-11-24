<script lang="ts">
  import Entry from '$lib/Entry.svelte';
  import PasswordInput from '$lib/PasswordInput.svelte';
  import type { PageProps } from './$types';
  let { data, form }: PageProps = $props();
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
    <form class="fields" method="POST" action="?/reset">
      <label for="reset-code">Reset code</label>
      <input id="reset-code" class="app-input mono" type="password" name="resetCode" required autocomplete="off" />
      <p>{data.mail ? 'Paste the code from the email, or one an administrator gave you.' : 'Ask a tenant administrator for a reset code.'} It works once, for fifteen minutes.</p>
      <label for="new-password">New password</label>
      <PasswordInput id="new-password" name="newPassword" required autocomplete="new-password" aria-describedby="pw-hint" />
      <p id="pw-hint">15 to 128 characters. Setting it signs you out everywhere and ends CLI sign-ins; API keys keep working.</p>
      <button class="app-control primary wide" type="submit">Set password</button>
    </form>
  {/if}
  <nav class="links"><a href="/t/{encodeURIComponent(data.tenant.key)}/login">Back to sign-in</a></nav>
</Entry>
