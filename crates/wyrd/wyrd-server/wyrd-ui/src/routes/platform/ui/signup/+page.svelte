<script lang="ts">
  import { getContext } from 'svelte';
  import { enhance } from '$app/forms';
  import ConsoleProblem from '$lib/ConsoleProblem.svelte';
  import type { ConsoleReauth } from '$lib/console-reauth.svelte';
  import type { PageProps } from './$types';
  let { data, form }: PageProps = $props();
  const reauth = getContext<ConsoleReauth>('console-reauth');
  // svelte-ignore state_referenced_locally -- the toggle starts from the stored settings and is the operator's after that
  let challenge = $state(Boolean(data.settings.signup_challenge));
  const problem = $derived(form?.problem?.code === 'WYRD_AUTH_403_RECENT_LOGIN_REQUIRED' ? null : form?.problem);
</script>

{#if problem}<ConsoleProblem issue={problem} />{/if}
<div role="status" aria-live="polite">{#if form?.notice}<p>✓ {form.notice}</p>{/if}</div>

<section aria-labelledby="login">
  <h2 id="login">Login and signup</h2>
  <form class="stack" method="POST" action="?/save" use:enhance={reauth.guard('Changing login and signup settings', false)}>
    <label class="check"><input type="checkbox" name="password_enabled" defaultChecked={data.settings.password_enabled} /> Password sign-in for workspaces</label>
    <label class="check"><input type="checkbox" name="signup_enabled" defaultChecked={data.settings.signup_enabled} /> Public workspace signup</label>
    <p>Pages offer signup only while a creation method is usable: email signup also needs password sign-in and the mail relay; provider signup needs an enabled provider.</p>
    <label for="cap">Workspaces one identity may create</label>
    <input id="cap" class="app-input mono" name="signup_workspace_cap" type="number" min="1" required defaultValue={data.settings.signup_workspace_cap} />
    <p>Counts every signup-created workspace that is not retired, including failed ones. Operator-created workspaces never count.</p>
    <fieldset>
      <legend>Human challenge (Cloudflare Turnstile)</legend>
      <label class="check"><input type="checkbox" name="challenge" bind:checked={challenge} /> Require a challenge for browser workspace creation</label>
      {#if challenge}
        <label for="site-key">Site key</label><input id="site-key" class="app-input mono" name="site_key" required autocomplete="off" defaultValue={data.settings.signup_challenge?.site_key ?? ''} />
        <label for="site-secret">Secret key</label>
        <!-- A new result empties the write-only secret. -->
        {#key form}<input id="site-secret" class="app-input mono" name="secret" type="password" autocomplete="off" />{/key}
        <p>The stored secret is never shown. Leave it blank to keep it.</p>
      {/if}
    </fieldset>
    <button class="app-control primary" type="submit">Save settings</button>
  </form>
</section>

<style>
  fieldset {
    display: grid;
    gap: 8px;
    margin: 0;
    padding: 10px 12px;
    border: 1px solid var(--border-soft);
    border-radius: var(--r);
  }
  [role='status']:empty {
    display: none;
  }
</style>
