<script lang="ts">
  import { getContext } from 'svelte';
  import { enhance } from '$app/forms';
  import ConsoleProblem from '$lib/ConsoleProblem.svelte';
  import type { ConsoleReauth } from '$lib/console-reauth.svelte';
  import type { PageProps } from './$types';
  let { data, form }: PageProps = $props();
  const reauth = getContext<ConsoleReauth>('console-reauth');
  const problem = $derived(form?.problem?.code === 'WYRD_AUTH_403_RECENT_LOGIN_REQUIRED' ? null : form?.problem);
  // Each switch starts from the stored settings and is the operator's after that.
  // svelte-ignore state_referenced_locally
  let challenge = $state(Boolean(data.settings.signup_challenge));
  // svelte-ignore state_referenced_locally
  let signup = $state(data.settings.signup_enabled);
  // svelte-ignore state_referenced_locally
  let password = $state(data.settings.password_enabled);
</script>

<header class="pagehead"><div><h1>Overview</h1><p>The state of this Wyrd deployment.</p></div></header>

{#if problem}<ConsoleProblem issue={problem} />{/if}
<div role="status" aria-live="polite">{#if form?.notice}<p class="done" data-testid="notice"><span class="ok">✓</span> {form.notice}</p>{/if}</div>

<div class="stats">
  <a class="stat" href="/platform/ui/workspaces"><span class="v">{data.workspaces}</span><span class="k">Active workspaces</span></a>
  <a class="stat" href="/platform/ui/providers"><span class="v">{data.providers.enabled} of {data.providers.total}</span><span class="k">Sign-in providers on</span></a>
  <a class="stat" class:warn={!data.relay} href="/platform/ui/mail"><span class="v">{data.relay ? '✓' : '!'}</span><span class="k">{data.relay ? 'Mail relay set' : 'No mail relay'}</span></a>
</div>

<section class="card" aria-labelledby="signup">
  <header><h2 id="signup">Sign-up</h2></header>
  <form method="POST" action="?/save" use:enhance={reauth.guard('Changing login and signup settings', false)}>
    <div class="row">
      <div class="label"><b id="signup-label">Allow new workspaces</b><span>Anyone can create a workspace from the sign-in page. Email sign-up also needs password sign-in and a mail relay; provider sign-up needs a provider that is on.</span></div>
      <div class="ctl"><label class="switch"><input type="checkbox" role="switch" name="signup_enabled" aria-labelledby="signup-label" bind:checked={signup} />{signup ? 'On' : 'Off'}</label></div>
    </div>
    <div class="row">
      <div class="label"><b id="cap-label">Workspaces per person</b><span>How many workspaces one identity can create. Failed sign-ups count; workspaces you create here never do.</span></div>
      <div class="ctl"><input class="app-input" style:width="80px" name="signup_workspace_cap" type="number" min="1" required aria-labelledby="cap-label" defaultValue={data.settings.signup_workspace_cap} /></div>
    </div>
    <div class="row">
      <div class="label"><b id="password-label">Password sign-in</b><span>Offer username and password on every workspace's sign-in page.</span></div>
      <div class="ctl"><label class="switch"><input type="checkbox" role="switch" name="password_enabled" aria-labelledby="password-label" bind:checked={password} />{password ? 'On' : 'Off'}</label></div>
    </div>
    <div class="row">
      <div class="label"><b id="challenge-label">Human challenge</b><span>Require a Cloudflare Turnstile challenge for browser workspace creation.</span></div>
      <div class="ctl"><label class="switch"><input type="checkbox" role="switch" name="challenge" aria-labelledby="challenge-label" bind:checked={challenge} />{challenge ? 'On' : 'Off'}</label></div>
    </div>
    {#if challenge}
      <div class="row">
        <div class="field grow">
          <label for="site-key">Site key</label><input id="site-key" class="app-input mono" name="site_key" required autocomplete="off" defaultValue={data.settings.signup_challenge?.site_key ?? ''} />
          <label for="site-secret">Secret key</label>
          <!-- A new result empties the write-only secret. -->
          {#key form}<input id="site-secret" class="app-input mono" name="secret" type="password" autocomplete="off" />{/key}
          <span class="hint">The stored secret is never shown. Leave it blank to keep it.</span>
        </div>
      </div>
    {/if}
    <div class="row end"><button class="app-control primary" type="submit">Save settings</button></div>
  </form>
</section>

<style>
  .grow {
    flex: 1;
    max-width: 480px;
  }
  form > .row:first-child {
    border-top: 0;
  }
  .end {
    justify-content: flex-end;
  }
</style>
