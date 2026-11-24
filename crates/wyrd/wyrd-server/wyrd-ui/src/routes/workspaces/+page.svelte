<!-- Ask for a link listing the workspaces an address may sign in to. Every address gets the same answer. -->
<script lang="ts">
  import Entry from '$lib/Entry.svelte';
  import CheckEmail from '$lib/CheckEmail.svelte';
  import { PROBLEMS, auth, emailError, openProof, refusal } from '$lib/onboarding';
  import type { WyrdProblem } from '$lib/server/problem';
  import type { PageProps } from './$types';
  let { data }: PageProps = $props();

  // svelte-ignore state_referenced_locally -- the load's answer is the page's first state
  let issue: WyrdProblem | null = $state.raw(data.problem ?? (data.discovery ? null : PROBLEMS.discoveryOff));
  let email = $state('');
  let checked = $state(false);
  let sent = $state(false);
  let busy = $state(false);
  let input: HTMLInputElement | undefined = $state();
  const error = $derived(checked ? emailError(email) : '');

  async function request(event: SubmitEvent) {
    event.preventDefault();
    checked = true;
    if (error) return input?.focus();
    busy = true;
    const answer = await auth('/auth/browser/workspaces/request', { email: email.trim() });
    busy = false;
    sent = answer.status === 202;
    issue = sent ? null : refusal(answer, { WYRD_AUTH_403_METHOD_UNAVAILABLE: PROBLEMS.discoveryOff });
  }
</script>

<svelte:head><title>Find your workspaces · Wyrd</title></svelte:head>
<Entry heading={sent ? 'Check your email' : 'Find your workspaces'} {issue}>
  {#if sent}
    <CheckEmail address={email.trim()} onverify={async (code) => openProof('/workspaces/verify', code)} again="/workspaces" />
  {:else if issue === PROBLEMS.discoveryOff}
    <a class="app-control primary wide" href="/">Enter a workspace URL</a>
  {:else}
    <p class="intro">We email a link that lists the workspaces where this address is a verified recovery email. The message itself names no workspace.</p>
    <form class="fields" novalidate onsubmit={request}>
      <label for="email">Email</label>
      <input id="email" class="app-input" type="email" name="email" autocomplete="email" readonly={busy} bind:this={input} bind:value={email}
        aria-invalid={error ? 'true' : undefined} aria-describedby={error ? 'email-error' : undefined} />
      {#if error}<p class="field-error" id="email-error">⊘ {error}</p>{/if}
      <button class="app-control primary wide" type="submit" disabled={busy}>Email me a link</button>
    </form>
    <nav class="links"><a href="/">Enter a workspace URL</a></nav>
  {/if}
</Entry>

<style>
  .intro {
    text-align: center;
  }
</style>
