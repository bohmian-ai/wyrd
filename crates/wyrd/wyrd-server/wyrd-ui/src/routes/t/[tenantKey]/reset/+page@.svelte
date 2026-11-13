<script lang="ts">
  import Entry from '$lib/Entry.svelte';
  import PasswordInput from '$lib/PasswordInput.svelte';
  import type { PageProps } from './$types';
  let { data, form }: PageProps = $props();
</script>

<svelte:head><title>Reset password · {data.tenant.key} · Wyrd</title></svelte:head>
<Entry heading="Reset your {data.tenant.key} password" issue={form?.problem ?? data.problem}>
  <form class="fields" method="POST">
    <label for="reset-code">Reset code</label>
    <input id="reset-code" class="app-input mono" type="password" name="resetCode" required autocomplete="off" />
    <p>Ask a tenant administrator for a reset code; it works once, for fifteen minutes.</p>
    <label for="new-password">New password</label>
    <PasswordInput id="new-password" name="newPassword" required autocomplete="new-password" />
    <button class="app-control primary wide" type="submit">Set password</button>
  </form>
  <nav class="links"><a href="/t/{encodeURIComponent(data.tenant.key)}/login">Back to sign-in</a></nav>
</Entry>
