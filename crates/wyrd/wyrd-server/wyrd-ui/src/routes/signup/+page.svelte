<script lang="ts">
  import Entry from '$lib/Entry.svelte';
  import CheckEmail from '$lib/CheckEmail.svelte';
  import { PROBLEMS, PROVIDERS, auth, emailError, refusal, verifySignup } from '$lib/onboarding';
  import type { WyrdProblem } from '$lib/server/problem';
  import type { PageProps } from './$types';
  let { data }: PageProps = $props();

  const names = $derived(data.providers.map((provider) => PROVIDERS[provider]).join(' or '));
  // svelte-ignore state_referenced_locally -- the load's problem is the page's first state
  let issue: WyrdProblem | null = $state.raw(data.problem ?? (data.failed ? PROBLEMS.failed : null));
  let email = $state('');
  let checked = $state(false);
  let sent = $state(false);
  let busy = $state(false);
  const error = $derived(checked ? emailError(email) : '');
  let input: HTMLInputElement | undefined = $state();

  async function byEmail(event: SubmitEvent) {
    event.preventDefault();
    checked = true;
    if (error) return input?.focus();
    busy = true;
    const answer = await auth('/auth/signup', { method: 'email', email: email.trim() });
    busy = false;
    issue = answer.status === 202 ? null : refusal(answer);
    sent = answer.status === 202;
  }

  async function byProvider(event: SubmitEvent, provider: string) {
    event.preventDefault();
    busy = true;
    const answer = await auth('/auth/signup', { method: 'social', provider });
    const target = answer.body?.authorization_url;
    if (answer.status === 200 && typeof target === 'string') return location.assign(target);
    busy = false;
    issue = refusal(answer);
  }

  async function verify(proof: string) {
    issue = await verifySignup(proof);
    if (!issue) location.assign('/signup/workspace');
  }
</script>

<svelte:head><title>Create a workspace · Wyrd</title></svelte:head>
<Entry heading={sent ? 'Check your email' : 'Create a workspace'} {issue}>
  {#if !data.signup}
    <p class="intro" role="status">Creating a workspace is turned off on this deployment. To join an existing workspace, open your invitation link or ask its administrator to invite you.</p>
    <a class="app-control wide" href="/">Sign in to an existing workspace</a>
  {:else if sent}
    <CheckEmail address={email.trim()} onverify={verify} again="/signup" />
  {:else}
    <p class="intro">Confirm {data.local ? `your email${data.providers.length ? `, or who you are with ${names},` : ''}` : `who you are with ${names},`} then name your workspace. You become its administrator.</p>
    {#if data.local}
      <form class="fields" novalidate onsubmit={byEmail}>
        <label for="email">Email</label>
        <input id="email" class="app-input" type="email" name="email" autocomplete="email" readonly={busy} bind:this={input} bind:value={email}
          aria-invalid={error ? 'true' : undefined} aria-describedby={error ? 'email-error' : undefined} />
        {#if error}<p class="field-error" id="email-error">⊘ {error}</p>{/if}
        <button class="app-control primary wide" type="submit" disabled={busy}>Continue with email</button>
      </form>
      {#if data.providers.length}<p class="or">or</p>{/if}
    {/if}
    {#each data.providers as provider (provider)}
      <form onsubmit={(event) => byProvider(event, provider)}>
        <button class="app-control wide" type="submit" disabled={busy}>Continue with {PROVIDERS[provider]}</button>
      </form>
    {/each}
    <nav class="links" aria-label="Existing workspace"><a href="/">Already have a workspace? Sign in</a></nav>
  {/if}
</Entry>

<style>
  .intro {
    text-align: center;
  }
</style>
